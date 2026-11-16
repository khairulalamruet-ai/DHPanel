//! The entitlement a license authority issues (T17.04, T17.09): claims, signed, and checked
//! by the DHPanel installation it was issued to.
//!
//! A [`Document`] is the claims in canonical JSON plus the key id and the Ed25519 signature
//! over a domain-separated form of those bytes. [`Document::verify`] refuses an unknown key,
//! a signature that does not match, a document for another issuer, audience or installation,
//! one that is not yet valid or has expired, and anything that is not exactly what the
//! authority wrote (a changed byte, an added claim).

use std::collections::BTreeMap;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    canonical::{CanonicalError, canonical_json},
    keys::{PublicKeys, SigningKey},
};

/// The schema version of the claims.
pub const SCHEMA_VERSION: u32 = 1;

/// What a signature covers is prefixed with this, so a signature made for one purpose is
/// never valid for another (entitlements, release manifests and so on use their own).
pub const ENTITLEMENT_DOMAIN: &[u8] = b"dhpanel-entitlement-v1\n";

/// The claims of an entitlement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Claims {
    /// Schema version ([`SCHEMA_VERSION`]).
    pub v: u32,
    /// Who issued it (the authority's name).
    pub iss: String,
    /// Who it is for (`dhpanel`).
    pub aud: String,
    /// The license.
    pub license_id: Uuid,
    /// The installation it was issued to.
    pub installation_id: Uuid,
    /// What it switches on.
    pub features: Vec<String>,
    /// Numeric limits (`max_nodes`, `max_tenants`); `-1` is no limit.
    pub limits: BTreeMap<String, i64>,
    /// Issued at (Unix seconds).
    pub iat: i64,
    /// Valid until (Unix seconds).
    pub exp: i64,
    /// This document's id.
    pub jti: Uuid,
    /// The key that signed it.
    pub kid: String,
}

/// Why a document was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocumentError {
    /// The claims could not be made canonical.
    #[error("claims: {0}")]
    Canonical(#[from] CanonicalError),
    /// The key could not sign.
    #[error("signing failed")]
    Sign,
    /// Not a document: unreadable, or not in the canonical form.
    #[error("not a valid document")]
    Malformed,
    /// Signed by a key that is not trusted, or the signature does not match.
    #[error("the signature is not valid")]
    Signature,
    /// For another issuer, audience or installation.
    #[error("the document is not for this installation")]
    Wrong,
    /// Not valid yet.
    #[error("the document is not valid yet")]
    NotYet,
    /// Out of date.
    #[error("the document has expired")]
    Expired,
}

/// A signed entitlement, as it travels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    /// The key id.
    pub kid: String,
    /// `EdDSA`.
    pub alg: String,
    /// The canonical claims, base64url.
    pub payload: String,
    /// The signature, base64url.
    pub signature: String,
}

fn signed_bytes(domain: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut m = domain.to_vec();
    m.extend_from_slice(payload);
    m
}

impl Document {
    /// Signs `claims` (their `kid` is set to the key's).
    ///
    /// # Errors
    /// [`DocumentError`].
    pub fn sign(claims: &Claims, key: &dyn SigningKey) -> Result<Self, DocumentError> {
        let mut claims = claims.clone();
        claims.kid = key.kid().to_owned();
        let value = serde_json::to_value(&claims).map_err(|_| DocumentError::Malformed)?;
        let payload = canonical_json(&value)?;
        let signature = key
            .sign(&signed_bytes(ENTITLEMENT_DOMAIN, &payload))
            .map_err(|_| DocumentError::Sign)?;
        Ok(Self {
            kid: claims.kid,
            alg: "EdDSA".to_owned(),
            payload: URL_SAFE_NO_PAD.encode(&payload),
            signature: URL_SAFE_NO_PAD.encode(signature),
        })
    }

    /// Checks the signature and the claims and returns them.
    ///
    /// # Errors
    /// [`DocumentError`].
    pub fn verify(
        &self,
        keys: &PublicKeys,
        expect: &Expect<'_>,
        now: i64,
    ) -> Result<Claims, DocumentError> {
        if self.alg != "EdDSA" {
            return Err(DocumentError::Malformed);
        }
        let payload = URL_SAFE_NO_PAD
            .decode(&self.payload)
            .map_err(|_| DocumentError::Malformed)?;
        let signature = URL_SAFE_NO_PAD
            .decode(&self.signature)
            .map_err(|_| DocumentError::Malformed)?;
        if !keys.verify(
            &self.kid,
            &signed_bytes(ENTITLEMENT_DOMAIN, &payload),
            &signature,
        ) {
            return Err(DocumentError::Signature);
        }
        let claims: Claims =
            serde_json::from_slice(&payload).map_err(|_| DocumentError::Malformed)?;
        // What was signed must be the canonical form (the one way to write it), so a copy that
        // was re-serialised or padded is not accepted, and the key id inside is the one outside.
        let value = serde_json::to_value(&claims).map_err(|_| DocumentError::Malformed)?;
        if canonical_json(&value)? != payload
            || claims.kid != self.kid
            || claims.v != SCHEMA_VERSION
        {
            return Err(DocumentError::Malformed);
        }
        if claims.iss != expect.issuer
            || claims.aud != expect.audience
            || claims.installation_id != expect.installation_id
        {
            return Err(DocumentError::Wrong);
        }
        if claims.iat > now + expect.clock_skew {
            return Err(DocumentError::NotYet);
        }
        if claims.exp <= now {
            return Err(DocumentError::Expired);
        }
        Ok(claims)
    }
}

/// What a verifier expects the claims to say.
#[derive(Debug, Clone, Copy)]
pub struct Expect<'a> {
    /// The issuer's name.
    pub issuer: &'a str,
    /// The audience.
    pub audience: &'a str,
    /// This installation.
    pub installation_id: Uuid,
    /// How far ahead of this clock `iat` may be (seconds).
    pub clock_skew: i64,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::keys::FileSigningKey;

    fn claims(installation: Uuid) -> Claims {
        Claims {
            v: SCHEMA_VERSION,
            iss: "dhpanel-la".into(),
            aud: "dhpanel".into(),
            license_id: Uuid::from_u128(1),
            installation_id: installation,
            features: vec!["resellers".into(), "mail".into()],
            limits: BTreeMap::from([("max_nodes".into(), 3), ("max_tenants".into(), -1)]),
            iat: 1_000,
            exp: 2_000,
            jti: Uuid::from_u128(2),
            kid: String::new(),
        }
    }

    fn setup() -> (FileSigningKey, PublicKeys, Uuid) {
        let dir = std::env::temp_dir().join(format!("dh-license-doc-{}", Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let key = FileSigningKey::generate_to_file(&dir.join("k"), "lic-1").unwrap();
        let keys = PublicKeys::new().with("lic-1", key.public_key()).unwrap();
        (key, keys, Uuid::from_u128(77))
    }

    fn expect(installation: Uuid) -> Expect<'static> {
        Expect {
            issuer: "dhpanel-la",
            audience: "dhpanel",
            installation_id: installation,
            clock_skew: 60,
        }
    }

    #[test]
    fn a_signed_document_verifies_and_gives_the_claims_back() {
        let (key, keys, inst) = setup();
        let doc = Document::sign(&claims(inst), &key).unwrap();
        let got = doc.verify(&keys, &expect(inst), 1_500).unwrap();
        assert_eq!(got.features, ["resellers", "mail"]);
        assert_eq!(got.kid, "lic-1");
        // The same claims always make the same document.
        assert_eq!(doc, Document::sign(&claims(inst), &key).unwrap());
    }

    #[test]
    fn a_forged_or_changed_document_is_refused() {
        let (key, keys, inst) = setup();
        let doc = Document::sign(&claims(inst), &key).unwrap();
        // A changed claim (more nodes), with the old signature.
        let mut forged = claims(inst);
        forged.limits.insert("max_nodes".into(), 300);
        forged.kid = "lic-1".into();
        let payload = canonical_json(&serde_json::to_value(&forged).unwrap()).unwrap();
        let changed = Document {
            payload: URL_SAFE_NO_PAD.encode(&payload),
            ..doc.clone()
        };
        assert_eq!(
            changed.verify(&keys, &expect(inst), 1_500),
            Err(DocumentError::Signature)
        );
        // A flipped bit anywhere in the payload or the signature.
        for field in 0..2 {
            let mut d = doc.clone();
            let target = if field == 0 {
                &mut d.payload
            } else {
                &mut d.signature
            };
            let mut bytes = target.clone().into_bytes();
            bytes[4] = if bytes[4] == b'A' { b'B' } else { b'A' };
            *target = String::from_utf8(bytes).unwrap();
            assert!(d.verify(&keys, &expect(inst), 1_500).is_err());
        }
        // Signed by a key that is not in the bundle, or a key id that was swapped.
        let (other_key, _, _) = setup();
        let stranger = Document::sign(&claims(inst), &other_key).unwrap();
        assert_eq!(
            stranger.verify(&keys, &expect(inst), 1_500),
            Err(DocumentError::Signature)
        );
        let swapped = Document {
            kid: "lic-2".into(),
            ..doc.clone()
        };
        assert_eq!(
            swapped.verify(&keys, &expect(inst), 1_500),
            Err(DocumentError::Signature)
        );
        // A correct signature over claims that are not in the canonical form is not accepted.
        let loose = serde_json::to_vec_pretty(
            &serde_json::to_value({
                let mut c = claims(inst);
                c.kid = "lic-1".into();
                c
            })
            .unwrap(),
        )
        .unwrap();
        let sig = key.sign(&signed_bytes(ENTITLEMENT_DOMAIN, &loose)).unwrap();
        let padded = Document {
            kid: "lic-1".into(),
            alg: "EdDSA".into(),
            payload: URL_SAFE_NO_PAD.encode(&loose),
            signature: URL_SAFE_NO_PAD.encode(sig),
        };
        assert_eq!(
            padded.verify(&keys, &expect(inst), 1_500),
            Err(DocumentError::Malformed)
        );
        // A signature made for another purpose does not pass as an entitlement.
        let other_purpose = key
            .sign(&signed_bytes(b"dhpanel-release-v1\n", &payload))
            .unwrap();
        let reused = Document {
            signature: URL_SAFE_NO_PAD.encode(other_purpose),
            ..changed
        };
        assert_eq!(
            reused.verify(&keys, &expect(inst), 1_500),
            Err(DocumentError::Signature)
        );
    }

    #[test]
    fn a_document_for_another_installation_or_time_is_refused() {
        let (key, keys, inst) = setup();
        let doc = Document::sign(&claims(inst), &key).unwrap();
        assert_eq!(
            doc.verify(&keys, &expect(Uuid::from_u128(78)), 1_500),
            Err(DocumentError::Wrong)
        );
        let mut wrong_audience = expect(inst);
        wrong_audience.audience = "something-else";
        assert_eq!(
            doc.verify(&keys, &wrong_audience, 1_500),
            Err(DocumentError::Wrong)
        );
        assert_eq!(
            doc.verify(&keys, &expect(inst), 2_000),
            Err(DocumentError::Expired)
        );
        assert_eq!(
            doc.verify(&keys, &expect(inst), 100),
            Err(DocumentError::NotYet)
        );
        assert!(
            doc.verify(&keys, &expect(inst), 950).is_ok(),
            "a little clock skew is fine"
        );
    }
}
