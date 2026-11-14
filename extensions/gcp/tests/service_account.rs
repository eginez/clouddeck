mod common;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use clouddeck_gcp::{parse_service_account, sign_jwt};
use common::{fixture, service_account_key_json};
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, RsaKeyPair, UnparsedPublicKey};

const AUDIENCE: &str = "https://oauth2.googleapis.com/token";
const NOW: u64 = 1_700_000_000;

fn decode(part: &str) -> serde_json::Value {
    let bytes = URL_SAFE_NO_PAD.decode(part).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn key_file_parses_into_its_email_and_key() {
    let key = parse_service_account(&service_account_key_json()).unwrap();
    assert_eq!(key.client_email, "reader@demo-prod.iam.gserviceaccount.com");
    assert!(RsaKeyPair::from_pkcs8(&key.pkcs8).is_ok());
}

#[test]
fn user_login_files_are_not_service_account_keys() {
    let error = parse_service_account(&fixture("adc_authorized_user.json")).unwrap_err();
    assert!(error.message.contains("authorized_user"));
}

#[test]
fn key_file_without_a_private_key_is_refused() {
    let text = serde_json::json!({
        "type": "service_account",
        "client_email": "reader@demo-prod.iam.gserviceaccount.com",
    })
    .to_string();
    assert!(parse_service_account(&text).is_err());
}

#[test]
fn jwt_has_the_expected_header_and_claims() {
    let key = parse_service_account(&service_account_key_json()).unwrap();
    let jwt = sign_jwt(&key, NOW, AUDIENCE).unwrap();
    let parts: Vec<&str> = jwt.split('.').collect();
    assert_eq!(parts.len(), 3);

    assert_eq!(decode(parts[0])["alg"], "RS256");
    let claims = decode(parts[1]);
    assert_eq!(claims["iss"], "reader@demo-prod.iam.gserviceaccount.com");
    assert_eq!(claims["aud"], AUDIENCE);
    assert_eq!(
        claims["scope"],
        "https://www.googleapis.com/auth/cloud-platform.read-only"
    );
    assert_eq!(claims["iat"], NOW);
    assert_eq!(claims["exp"], NOW + 3600);
}

#[test]
fn jwt_signature_verifies_against_the_key_pair() {
    let key = parse_service_account(&service_account_key_json()).unwrap();
    let jwt = sign_jwt(&key, NOW, AUDIENCE).unwrap();
    let parts: Vec<&str> = jwt.split('.').collect();
    let signed = format!("{}.{}", parts[0], parts[1]);
    let signature = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();

    let pair = RsaKeyPair::from_pkcs8(&key.pkcs8).unwrap();
    UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, pair.public().as_ref())
        .verify(signed.as_bytes(), &signature)
        .unwrap();
}
