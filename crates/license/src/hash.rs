//! SHA-256, through ring (the one crypto backend): file hashes in release manifests and the
//! license key's hash in an activation proof.

use ring::digest::{SHA256, digest};

/// The SHA-256 of `bytes`.
#[must_use]
pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(digest(&SHA256, bytes).as_ref());
    out
}

#[cfg(test)]
mod tests {
    use super::sha256;

    #[test]
    fn it_is_sha256() {
        let hex: String = sha256(b"abc").iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(
            hex,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
