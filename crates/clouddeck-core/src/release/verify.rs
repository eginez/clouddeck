//! Release asset and manifest verification.

use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

/// Release public key generated 2026-10-31. The private key lives in the RELEASE_SIGNING_KEY Actions secret and on the maintainer's Mac.
pub const RELEASE_PUBLIC_KEY: &str = "QQmck07601yw+Q7ZGL9ZtsZjOmLSFvGHMUZDaoj2DME=";

/// Checks that the manifest bytes carry a valid ed25519 signature from the given public key.
pub fn verify_manifest(
    manifest_bytes: &[u8],
    signature_b64: &str,
    public_key_b64: &str,
) -> Result<(), String> {
    if decode(public_key_b64).is_ok_and(|key| key == [0u8; 32]) {
        return Err("release key not configured".into());
    }

    let key_bytes: [u8; 32] = decode(public_key_b64)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or("bad public key")?;
    let key = VerifyingKey::from_bytes(&key_bytes).map_err(|_| "bad public key")?;

    let sig_bytes = decode(signature_b64).map_err(|_| "bad signature encoding")?;
    let signature = Signature::from_slice(&sig_bytes).map_err(|_| "bad signature encoding")?;

    key.verify_strict(manifest_bytes, &signature)
        .map_err(|_| "signature does not match".into())
}

/// The lowercase hex SHA-256 digest of the bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Checks that the bytes have the expected size and SHA-256 digest.
pub fn check_asset(bytes: &[u8], expected_sha256: &str, expected_size: u64) -> Result<(), String> {
    let actual_size = bytes.len() as u64;
    if actual_size != expected_size {
        return Err(format!(
            "size mismatch: expected {expected_size}, got {actual_size}"
        ));
    }
    if !sha256_hex(bytes).eq_ignore_ascii_case(expected_sha256) {
        return Err("checksum mismatch".into());
    }
    Ok(())
}

fn decode(encoded: &str) -> Result<Vec<u8>, base64::DecodeError> {
    STANDARD.decode(encoded.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use rand_core::OsRng;

    fn keypair() -> (SigningKey, String) {
        let signing = SigningKey::generate(&mut OsRng);
        let public = STANDARD.encode(signing.verifying_key().to_bytes());
        (signing, public)
    }

    fn sign_b64(signing: &SigningKey, bytes: &[u8]) -> String {
        STANDARD.encode(signing.sign(bytes).to_bytes())
    }

    #[test]
    fn valid_signature_is_accepted() {
        let (signing, public) = keypair();
        let manifest = b"{\"version\":\"0.2.0\"}\n";
        let sig = sign_b64(&signing, manifest);
        assert_eq!(verify_manifest(manifest, &sig, &public), Ok(()));
    }

    #[test]
    fn tampered_manifest_is_rejected() {
        let (signing, public) = keypair();
        let mut manifest = b"{\"version\":\"0.2.0\"}\n".to_vec();
        let sig = sign_b64(&signing, &manifest);
        manifest[2] ^= 0x01;
        assert_eq!(
            verify_manifest(&manifest, &sig, &public),
            Err("signature does not match".to_string())
        );
    }

    #[test]
    fn signature_from_another_key_is_rejected() {
        let (_, public) = keypair();
        let (other, _) = keypair();
        let manifest = b"manifest";
        let sig = sign_b64(&other, manifest);
        assert_eq!(
            verify_manifest(manifest, &sig, &public),
            Err("signature does not match".to_string())
        );
    }

    #[test]
    fn malformed_signature_encoding_is_rejected() {
        let (_, public) = keypair();
        assert_eq!(
            verify_manifest(b"manifest", "not base64!", &public),
            Err("bad signature encoding".to_string())
        );
    }

    #[test]
    fn wrong_length_public_key_is_rejected() {
        let (signing, _) = keypair();
        let manifest = b"manifest";
        let sig = sign_b64(&signing, manifest);
        let short_key = STANDARD.encode([7u8; 31]);
        assert_eq!(
            verify_manifest(manifest, &sig, &short_key),
            Err("bad public key".to_string())
        );
    }

    #[test]
    fn zero_release_key_is_not_configured() {
        let (signing, _) = keypair();
        let manifest = b"manifest";
        let sig = sign_b64(&signing, manifest);
        let zero_key = STANDARD.encode([0u8; 32]);
        assert_eq!(
            verify_manifest(manifest, &sig, &zero_key),
            Err("release key not configured".to_string())
        );
    }

    #[test]
    fn sha256_hex_matches_known_digest() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn matching_asset_is_accepted() {
        let bytes = b"abc";
        let digest = sha256_hex(bytes);
        assert_eq!(check_asset(bytes, &digest, 3), Ok(()));
    }

    #[test]
    fn wrong_asset_size_is_rejected() {
        let bytes = b"abc";
        let digest = sha256_hex(bytes);
        let err = check_asset(bytes, &digest, 4).unwrap_err();
        assert!(err.starts_with("size mismatch"), "{err}");
    }

    #[test]
    fn wrong_asset_hash_is_rejected() {
        let bytes = b"abc";
        let other = sha256_hex(b"abd");
        assert_eq!(
            check_asset(bytes, &other, 3),
            Err("checksum mismatch".to_string())
        );
    }

    #[test]
    fn uppercase_expected_hash_is_accepted() {
        let bytes = b"abc";
        let digest = sha256_hex(bytes).to_uppercase();
        assert_eq!(check_asset(bytes, &digest, 3), Ok(()));
    }
}
