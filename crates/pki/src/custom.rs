//! Checking a certificate a person uploads for one of their domains (T05.17).
//!
//! Everything is decided here, before anything is stored or sent to a node:
//!
//! * the certificate field holds one to five certificates (the site's first, then
//!   the ones that vouch for it) and nothing else;
//! * the first covers the domain (`example.com`, or a wildcard such as
//!   `*.example.com` for exactly one label more), is valid now, and each
//!   certificate is signed by the next one in the list;
//! * the private key is a plain PKCS#8 key (not protected by a passphrase) and is
//!   the key of the first certificate.
//!
//! The answer is the text to keep (the certificates as written, the key
//! re-encoded as PKCS#8) and what to show: issuer and dates. The
//! private key is returned as a [`SecretString`] and never appears in `Debug`.

use secrecy::SecretString;
use x509_parser::{
    certificate::X509Certificate,
    extensions::{GeneralName, ParsedExtension},
    pem::Pem,
    prelude::FromDer as _,
};

/// Most bytes accepted in each of the two fields.
pub const MAX_PEM_BYTES: usize = 32 * 1024;
/// Most certificates in the chain, the site's own included.
pub const MAX_CHAIN: usize = 5;

/// Why an upload was refused. `field` says which input to fix, `code` is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CertificateError {
    /// Too much text.
    #[error("the certificate or the key is too large")]
    TooLarge,
    /// The certificate field is not PEM certificates.
    #[error("the certificate must be PEM text: `-----BEGIN CERTIFICATE-----` blocks")]
    Unreadable,
    /// The certificate field holds something else too (a key, for example).
    #[error(
        "the certificate field must hold certificates only; the private key goes in its own field"
    )]
    NotOnlyCertificates,
    /// Too many certificates.
    #[error("at most 5 certificates (the site's and 4 that vouch for it)")]
    ChainTooLong,
    /// The key is protected by a passphrase.
    #[error(
        "the private key is protected by a passphrase; remove it first (openssl pkcs8 -topk8 -nocrypt)"
    )]
    KeyEncrypted,
    /// Not a usable PKCS#8 private key.
    #[error(
        "the private key must be an unencrypted PKCS#8 PEM key (`-----BEGIN PRIVATE KEY-----`)"
    )]
    InvalidKey,
    /// The key belongs to another certificate.
    #[error("the private key does not belong to this certificate")]
    KeyMismatch,
    /// The first certificate is not for this domain.
    #[error("the certificate does not cover this domain")]
    NameMismatch,
    /// Its validity has ended.
    #[error("the certificate has expired")]
    Expired,
    /// Its validity has not begun.
    #[error("the certificate is not valid yet")]
    NotYetValid,
    /// A certificate is not signed by the next one.
    #[error("the certificates are not a chain: each must be signed by the next one")]
    BrokenChain,
}

impl CertificateError {
    /// The input to fix: `certificate` or `private_key`.
    #[must_use]
    pub fn field(self) -> &'static str {
        match self {
            Self::KeyEncrypted | Self::InvalidKey | Self::KeyMismatch => "private_key",
            _ => "certificate",
        }
    }

    /// Stable machine code.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::TooLarge => "too_large",
            Self::Unreadable => "invalid_certificate",
            Self::NotOnlyCertificates => "not_only_certificates",
            Self::ChainTooLong => "chain_too_long",
            Self::KeyEncrypted => "key_encrypted",
            Self::InvalidKey => "invalid_private_key",
            Self::KeyMismatch => "key_mismatch",
            Self::NameMismatch => "name_mismatch",
            Self::Expired => "certificate_expired",
            Self::NotYetValid => "certificate_not_yet_valid",
            Self::BrokenChain => "broken_chain",
        }
    }
}

/// An upload that passed every check.
pub struct Checked {
    /// The certificates, normalised, the site's first.
    pub certificate_pem: String,
    /// The private key as PKCS#8 PEM.
    pub private_key_pem: SecretString,
    /// The first certificate's issuer (common name).
    pub issuer: String,
    /// Valid from (Unix seconds).
    pub not_before: i64,
    /// Valid until (Unix seconds).
    pub not_after: i64,
}

impl std::fmt::Debug for Checked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Checked")
            .field("issuer", &self.issuer)
            .field("not_after", &self.not_after)
            .finish_non_exhaustive()
    }
}

/// Whether the names in `cert` cover `host` (exact, or a wildcard for one label).
fn covers(cert: &X509Certificate<'_>, host: &str) -> bool {
    let names: Vec<String> = cert
        .extensions()
        .iter()
        .filter_map(|e| match e.parsed_extension() {
            ParsedExtension::SubjectAlternativeName(san) => Some(san),
            _ => None,
        })
        .flat_map(|san| san.general_names.iter())
        .filter_map(|n| match n {
            GeneralName::DNSName(d) => Some(d.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    names.iter().any(|name| {
        name == host
            || name.strip_prefix("*.").is_some_and(|rest| {
                host.split_once('.')
                    .is_some_and(|(label, tail)| !label.is_empty() && tail == rest)
            })
    })
}

fn issuer_name(cert: &X509Certificate<'_>) -> String {
    let issuer = cert.issuer();
    let name = issuer
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map_or_else(|| issuer.to_string(), str::to_owned);
    name.chars().filter(|c| !c.is_control()).take(200).collect()
}

/// Checks an upload for `host` (lower-case ASCII) at time `now` (Unix seconds).
///
/// # Errors
/// [`CertificateError`] for the first problem found.
pub fn check(
    certificate_pem: &str,
    private_key_pem: &str,
    host: &str,
    now: i64,
) -> Result<Checked, CertificateError> {
    if certificate_pem.len() > MAX_PEM_BYTES || private_key_pem.len() > MAX_PEM_BYTES {
        return Err(CertificateError::TooLarge);
    }
    // The certificates.
    let mut blocks: Vec<Pem> = Vec::new();
    for block in Pem::iter_from_buffer(certificate_pem.as_bytes()) {
        blocks.push(block.map_err(|_| CertificateError::Unreadable)?);
    }
    if blocks.is_empty() {
        return Err(CertificateError::Unreadable);
    }
    if blocks.iter().any(|b| b.label != "CERTIFICATE") {
        return Err(CertificateError::NotOnlyCertificates);
    }
    if blocks.len() > MAX_CHAIN {
        return Err(CertificateError::ChainTooLong);
    }
    let certs: Vec<X509Certificate<'_>> = blocks
        .iter()
        .map(|b| {
            X509Certificate::from_der(&b.contents)
                .map(|(_, c)| c)
                .map_err(|_| CertificateError::Unreadable)
        })
        .collect::<Result<_, _>>()?;
    let leaf = &certs[0];
    if !covers(leaf, host) {
        return Err(CertificateError::NameMismatch);
    }
    let (not_before, not_after) = (
        leaf.validity().not_before.timestamp(),
        leaf.validity().not_after.timestamp(),
    );
    if now > not_after {
        return Err(CertificateError::Expired);
    }
    if now < not_before {
        return Err(CertificateError::NotYetValid);
    }
    for pair in certs.windows(2) {
        pair[0]
            .verify_signature(Some(pair[1].public_key()))
            .map_err(|_| CertificateError::BrokenChain)?;
    }

    // The key.
    if private_key_pem.contains("ENCRYPTED") {
        return Err(CertificateError::KeyEncrypted);
    }
    let key =
        rcgen::KeyPair::from_pem(private_key_pem).map_err(|_| CertificateError::InvalidKey)?;
    if key.public_key_raw() != leaf.public_key().subject_public_key.data.as_ref() {
        return Err(CertificateError::KeyMismatch);
    }
    Ok(Checked {
        // Already checked to be nothing but certificates: kept as written.
        certificate_pem: format!("{}\n", certificate_pem.trim()),
        private_key_pem: SecretString::from(key.serialize_pem()),
        issuer: issuer_name(leaf),
        not_before,
        not_after,
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair};
    use secrecy::ExposeSecret as _;
    use time::{Duration, OffsetDateTime};

    use super::*;

    struct Pki {
        root: Issuer<'static, KeyPair>,
        root_pem: String,
        mid: Issuer<'static, KeyPair>,
        mid_pem: String,
    }

    fn ca(name: &str, signer: Option<&Issuer<'_, KeyPair>>) -> (Issuer<'static, KeyPair>, String) {
        let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
        params.distinguished_name.push(DnType::CommonName, name);
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let key = KeyPair::generate().unwrap();
        let cert = match signer {
            Some(s) => params.signed_by(&key, s).unwrap(),
            None => params.self_signed(&key).unwrap(),
        };
        let pem = cert.pem();
        (Issuer::new(params, key), pem)
    }

    fn pki() -> Pki {
        let (root, root_pem) = ca("Test Root", None);
        let (mid, mid_pem) = ca("Test Intermediate", Some(&root));
        Pki {
            root,
            root_pem,
            mid,
            mid_pem,
        }
    }

    /// A leaf for `names`, valid from `from_days` to `to_days` from now, signed by `issuer`.
    fn leaf(
        issuer: &Issuer<'_, KeyPair>,
        names: &[&str],
        from_days: i64,
        to_days: i64,
    ) -> (String, KeyPair) {
        let mut params =
            CertificateParams::new(names.iter().map(|n| (*n).to_owned()).collect::<Vec<_>>())
                .unwrap();
        let now = OffsetDateTime::now_utc();
        params.not_before = now + Duration::days(from_days);
        params.not_after = now + Duration::days(to_days);
        let key = KeyPair::generate().unwrap();
        (params.signed_by(&key, issuer).unwrap().pem(), key)
    }

    fn now() -> i64 {
        OffsetDateTime::now_utc().unix_timestamp()
    }

    #[test]
    fn a_good_upload_is_accepted_and_normalised() {
        let p = pki();
        let (cert, key) = leaf(&p.mid, &["example.com", "www.example.com"], -1, 90);
        let chain = format!("{cert}{}{}", p.mid_pem, p.root_pem);
        let ok = check(&chain, &key.serialize_pem(), "example.com", now()).unwrap();
        assert_eq!(ok.issuer, "Test Intermediate");
        assert_eq!(ok.certificate_pem.matches("BEGIN CERTIFICATE").count(), 3);
        assert!(
            ok.private_key_pem
                .expose_secret()
                .contains("BEGIN PRIVATE KEY")
        );
        assert!((89 * 86_400..=91 * 86_400).contains(&(ok.not_after - now())));
        assert!(
            !format!("{ok:?}").contains("PRIVATE"),
            "the key is never shown"
        );
        // The normalised text can be checked again and gives the same.
        let again = check(
            &ok.certificate_pem,
            ok.private_key_pem.expose_secret(),
            "www.example.com",
            now(),
        )
        .unwrap();
        assert_eq!(again.certificate_pem, ok.certificate_pem);
        // A leaf alone, without its chain, is fine too (the server may add the chain itself).
        assert!(check(&cert, &key.serialize_pem(), "example.com", now()).is_ok());
    }

    #[test]
    fn the_key_must_be_the_certificates_and_unprotected() {
        let p = pki();
        let (cert, _) = leaf(&p.mid, &["example.com"], -1, 90);
        let other = KeyPair::generate().unwrap();
        let err = check(&cert, &other.serialize_pem(), "example.com", now()).unwrap_err();
        assert_eq!(err, CertificateError::KeyMismatch);
        assert_eq!((err.field(), err.code()), ("private_key", "key_mismatch"));
        let (_, key) = leaf(&p.mid, &["example.com"], -1, 90);
        assert_eq!(
            check(&cert, &key.serialize_pem(), "example.com", now()).unwrap_err(),
            CertificateError::KeyMismatch
        );
        for (bad, want) in [
            ("", CertificateError::InvalidKey),
            ("not a key", CertificateError::InvalidKey),
            (&cert, CertificateError::InvalidKey),
            (
                "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----\n",
                CertificateError::KeyEncrypted,
            ),
            (
                "-----BEGIN RSA PRIVATE KEY-----\nProc-Type: 4,ENCRYPTED\nDEK-Info: AES-128-CBC,00\n\nAAAA\n-----END RSA PRIVATE KEY-----\n",
                CertificateError::KeyEncrypted,
            ),
        ] {
            assert_eq!(
                check(&cert, bad, "example.com", now()).unwrap_err(),
                want,
                "{bad:.30}"
            );
        }
    }

    #[test]
    fn the_certificate_must_cover_the_domain_and_be_valid_now() {
        let p = pki();
        let (cert, key) = leaf(&p.mid, &["example.com", "*.shop.example.com"], -1, 90);
        let key = key.serialize_pem();
        assert!(check(&cert, &key, "example.com", now()).is_ok());
        assert!(
            check(&cert, &key, "a.shop.example.com", now()).is_ok(),
            "a wildcard covers one label"
        );
        for host in [
            "other.org",
            "www.example.com",
            "shop.example.com",
            "a.b.shop.example.com",
            "xexample.com",
        ] {
            assert_eq!(
                check(&cert, &key, host, now()).unwrap_err(),
                CertificateError::NameMismatch,
                "{host}"
            );
        }
        let (old, old_key) = leaf(&p.mid, &["example.com"], -90, -1);
        assert_eq!(
            check(&old, &old_key.serialize_pem(), "example.com", now()).unwrap_err(),
            CertificateError::Expired
        );
        let (future, future_key) = leaf(&p.mid, &["example.com"], 2, 90);
        assert_eq!(
            check(&future, &future_key.serialize_pem(), "example.com", now()).unwrap_err(),
            CertificateError::NotYetValid
        );
    }

    #[test]
    fn a_chain_must_be_signed_link_by_link_and_only_certificates() {
        let p = pki();
        let (cert, key) = leaf(&p.mid, &["example.com"], -1, 90);
        let key = key.serialize_pem();
        // Not signed by the next one: the root in place of the intermediate.
        let skipped = format!("{cert}{}", p.root_pem);
        assert_eq!(
            check(&skipped, &key, "example.com", now()).unwrap_err(),
            CertificateError::BrokenChain
        );
        // Order reversed.
        let reversed = format!("{}{cert}", p.mid_pem);
        assert_eq!(
            check(&reversed, &key, "example.com", now()).unwrap_err(),
            CertificateError::NameMismatch
        );
        // A key in the certificate field is refused with a pointer.
        let mixed = format!("{cert}{key}");
        assert_eq!(
            check(&mixed, &key, "example.com", now()).unwrap_err(),
            CertificateError::NotOnlyCertificates
        );
        // Garbage, empty, too long a chain, too much text.
        for bad in [
            "",
            "hello",
            "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n",
        ] {
            assert_eq!(
                check(bad, &key, "example.com", now()).unwrap_err(),
                CertificateError::Unreadable,
                "{bad:?}"
            );
        }
        let long = format!("{cert}{}", p.mid_pem.repeat(5));
        assert_eq!(
            check(&long, &key, "example.com", now()).unwrap_err(),
            CertificateError::ChainTooLong
        );
        let huge = "x".repeat(MAX_PEM_BYTES + 1);
        assert_eq!(
            check(&huge, &key, "example.com", now()).unwrap_err(),
            CertificateError::TooLarge
        );
        assert_eq!(
            check(&cert, &huge, "example.com", now()).unwrap_err(),
            CertificateError::TooLarge
        );
        let _ = &p.root;
    }
}
