//! Service-account keys: parse the downloaded JSON key file and sign the JWT that exchanges it for a token.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use clouddeck_extension_api::{ErrorKind, ProviderError};
use ring::rand::SystemRandom;
use ring::signature::{RSA_PKCS1_SHA256, RsaKeyPair};
use serde::Deserialize;

pub const JWT_BEARER_GRANT: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";
const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform.read-only";
const LIFETIME_SECS: u64 = 3600;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaKey {
    pub client_email: String,
    pub pkcs8: Vec<u8>,
}

#[derive(Deserialize)]
struct KeyFile {
    #[serde(rename = "type")]
    kind: String,
    client_email: Option<String>,
    private_key: Option<String>,
}

fn auth_error(message: impl Into<String>) -> ProviderError {
    ProviderError::new(ErrorKind::Auth, message)
}

/// Pure: read a service-account key file; the private key is decoded from its PEM body.
pub fn parse_service_account(text: &str) -> Result<SaKey, ProviderError> {
    let file: KeyFile = serde_json::from_str(text)
        .map_err(|e| auth_error(format!("unreadable service account key: {e}")))?;
    if file.kind != "service_account" {
        return Err(auth_error(format!(
            "key file is a {:?} credential, not a service account key",
            file.kind
        )));
    }
    let missing = || auth_error("service account key lacks client_email or private_key");
    let client_email = file.client_email.ok_or_else(missing)?;
    let pem = file.private_key.ok_or_else(missing)?;
    let body: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .flat_map(|line| line.chars().filter(|c| !c.is_whitespace()))
        .collect();
    let pkcs8 = STANDARD
        .decode(body)
        .map_err(|_| auth_error("service account private_key is not a PEM key"))?;
    Ok(SaKey {
        client_email,
        pkcs8,
    })
}

pub fn load_key(path: &Path) -> Result<SaKey, ProviderError> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        auth_error(format!(
            "cannot read service account key {}: {e}",
            path.display()
        ))
    })?;
    parse_service_account(&text).map_err(|e| {
        auth_error(format!(
            "service account key {} is not usable: {}",
            path.display(),
            e.message
        ))
    })
}

/// Pure: a leading `~/` is the home directory.
pub fn expand_home(raw: &str, home: Option<&str>) -> PathBuf {
    match (raw.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(raw),
    }
}

/// Pure: an RS256 JWT for the token endpoint, valid for an hour from `now_secs`.
pub fn sign_jwt(key: &SaKey, now_secs: u64, audience: &str) -> Result<String, ProviderError> {
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = serde_json::json!({
        "iss": key.client_email,
        "scope": SCOPE,
        "aud": audience,
        "iat": now_secs,
        "exp": now_secs + LIFETIME_SECS,
    });
    let signing_input = format!("{header}.{}", URL_SAFE_NO_PAD.encode(claims.to_string()));
    let rejected = || auth_error("service account private key was rejected");
    let pair = RsaKeyPair::from_pkcs8(&key.pkcs8).map_err(|_| rejected())?;
    let mut signature = vec![0; pair.public().modulus_len()];
    pair.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        signing_input.as_bytes(),
        &mut signature,
    )
    .map_err(|_| rejected())?;
    Ok(format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature)
    ))
}
