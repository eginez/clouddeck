//! Ed25519 signing of release manifests.

use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signer, SigningKey};
use rand_core::OsRng;

/// Creates a new signing key, returning the base64 secret seed and base64 public key.
pub fn generate_keypair() -> (String, String) {
    let signing = SigningKey::generate(&mut OsRng);
    let secret = STANDARD.encode(signing.to_bytes());
    let public = STANDARD.encode(signing.verifying_key().to_bytes());
    (secret, public)
}

/// Signs the bytes with the base64 secret seed, returning the base64 signature.
pub fn sign_bytes(bytes: &[u8], secret_b64: &str) -> Result<String, String> {
    let signing = signing_key(secret_b64)?;
    Ok(STANDARD.encode(signing.sign(bytes).to_bytes()))
}

/// The base64 public key matching the base64 secret seed.
pub fn public_from_secret(secret_b64: &str) -> Result<String, String> {
    let signing = signing_key(secret_b64)?;
    Ok(STANDARD.encode(signing.verifying_key().to_bytes()))
}

fn signing_key(secret_b64: &str) -> Result<SigningKey, String> {
    let seed: [u8; 32] = STANDARD
        .decode(secret_b64.trim())
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or("bad secret key")?;
    Ok(SigningKey::from_bytes(&seed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::verify::verify_manifest;

    #[test]
    fn generated_keypair_has_32_byte_base64_halves() {
        let (secret, public) = generate_keypair();
        assert_eq!(secret.len(), 44);
        assert_eq!(public.len(), 44);
    }

    #[test]
    fn signature_verifies_with_matching_public_key() {
        let (secret, public) = generate_keypair();
        let manifest = b"{\"version\":\"0.2.0\"}\n";
        let sig = sign_bytes(manifest, &secret).unwrap();
        assert_eq!(verify_manifest(manifest, &sig, &public), Ok(()));
    }

    #[test]
    fn public_key_is_derived_from_secret() {
        let (secret, public) = generate_keypair();
        assert_eq!(public_from_secret(&secret), Ok(public));
    }

    #[test]
    fn malformed_secret_is_rejected() {
        assert_eq!(
            sign_bytes(b"manifest", "xyz"),
            Err("bad secret key".to_string())
        );
    }

    #[test]
    fn each_keypair_is_fresh() {
        let (first, _) = generate_keypair();
        let (second, _) = generate_keypair();
        assert_ne!(first, second);
    }

    #[test]
    fn signing_is_deterministic() {
        let (secret, _) = generate_keypair();
        let first = sign_bytes(b"manifest", &secret).unwrap();
        let second = sign_bytes(b"manifest", &secret).unwrap();
        assert_eq!(first, second);
    }
}
