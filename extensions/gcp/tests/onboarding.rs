mod common;

use std::sync::Arc;

use clouddeck_extension_api::{
    Auth, AuthInput, AuthMethod, Browse, ConnectionId, ErrorKind, Provider, Selection,
};
use common::{
    FakeHttp, MemoryHost, OwnedSettingsHost, TestHost, call, fixture, provider, provider_on_disk,
    service_account_key_json, temp_folder,
};

const SUBTITLE: &str =
    "Choose a project. Only ACTIVE projects your credentials can see are listed.";
const FOOTER: &str =
    "Credentials are read from gcloud’s Application Default Credentials. CloudDeck stores nothing.";
const SIGN_IN: &str = "gcloud auth application-default login";

fn no_input() -> AuthInput {
    AuthInput {
        connection: None,
        values: Default::default(),
    }
}

fn method_named(p: &impl Auth, id: &str) -> AuthMethod {
    p.auth_methods().into_iter().find(|m| m.id == id).unwrap()
}

#[test]
fn onboarding_text_is_the_designed_copy() {
    let info = provider(Arc::new(FakeHttp::default())).info();
    assert_eq!(info.onboarding.title, None);
    assert_eq!(info.onboarding.subtitle.as_deref(), Some(SUBTITLE));
    assert_eq!(info.onboarding.footer.as_deref(), Some(FOOTER));
}

#[test]
fn project_rows_show_the_project_id_as_detail() {
    let p = provider(Arc::new(FakeHttp::default()));
    let rows = p.connections(&call(&TestHost)).unwrap();
    let ids: Vec<(&str, Option<&str>)> = rows
        .iter()
        .map(|r| (r.id.as_str(), r.detail.as_deref()))
        .collect();
    assert_eq!(
        ids,
        [
            ("demo-prod", Some("demo-prod")),
            ("demo-sandbox", Some("demo-sandbox")),
        ]
    );
}

#[test]
fn missing_adc_file_gets_the_sign_in_hint() {
    let dir = temp_folder("hint-missing");
    let host = OwnedSettingsHost(vec![("config_dir", dir.display().to_string())]);
    let p = provider_on_disk(Arc::new(FakeHttp::default()));
    let error = p.connections(&call(&host)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Auth);
    let hint = error.hint.expect("missing ADC carries a hint");
    assert_eq!(
        hint.title,
        "Google Cloud needs Application Default Credentials"
    );
    assert_eq!(hint.command.as_deref(), Some(SIGN_IN));
    assert!(error.advice.unwrap().contains(SIGN_IN));
}

#[test]
fn service_account_adc_gets_the_unsupported_hint() {
    let dir = temp_folder("hint-service-account");
    std::fs::write(
        dir.join("application_default_credentials.json"),
        fixture("adc_service_account.json"),
    )
    .unwrap();
    let host = OwnedSettingsHost(vec![("config_dir", dir.display().to_string())]);
    let p = provider_on_disk(Arc::new(FakeHttp::default()));
    let error = p.connections(&call(&host)).unwrap_err();
    let hint = error.hint.expect("service_account ADC carries a hint");
    assert_eq!(
        hint.title,
        "Service-account credentials aren’t supported yet"
    );
    assert!(hint.detail.unwrap().contains("service_account key"));
    assert_eq!(hint.command.as_deref(), Some(SIGN_IN));
}

#[test]
fn adc_identity_comes_from_tokeninfo() {
    let http = Arc::new(FakeHttp::default());
    let p = provider(Arc::clone(&http));
    let who = p.identity(
        &call(&TestHost),
        &ConnectionId::from("demo-prod"),
        &Selection::new(),
    );
    assert_eq!(who, Ok("esteban@example.com".to_string()));
    assert_eq!(http.count("/tokeninfo"), 1);
}

#[test]
fn emulator_mode_has_no_identity() {
    let host = common::SettingsHost(vec![("auth_mode", "emulator")]);
    let p = provider(Arc::new(FakeHttp::default()));
    let error = p
        .identity(
            &call(&host),
            &ConnectionId::from("demo-prod"),
            &Selection::new(),
        )
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Unsupported);
}

#[test]
fn key_file_identity_is_its_client_email_without_network() {
    let dir = temp_folder("identity-key");
    let key_path = dir.join("key.json");
    std::fs::write(&key_path, service_account_key_json()).unwrap();
    let http = Arc::new(FakeHttp::default());
    let p = provider(Arc::clone(&http));
    let host = MemoryHost::with_secret("service_account_key_file", &key_path.display().to_string());
    let who = p.identity(
        &call(&host),
        &ConnectionId::from("demo-prod"),
        &Selection::new(),
    );
    assert_eq!(
        who,
        Ok("reader@demo-prod.iam.gserviceaccount.com".to_string())
    );
    assert_eq!(http.count("tokeninfo"), 0);
}

#[test]
fn auth_methods_offer_adc_then_the_key_file() {
    let p = provider(Arc::new(FakeHttp::default()));
    let ids: Vec<String> = p.auth_methods().into_iter().map(|m| m.id).collect();
    assert_eq!(ids, ["adc", "service_account_key"]);
}

#[test]
fn key_method_prompts_for_a_key_file_path() {
    let p = provider(Arc::new(FakeHttp::default()));
    let method = method_named(&p, "service_account_key");
    let step = p.begin_auth(&call(&TestHost), &method, no_input()).unwrap();
    let clouddeck_extension_api::AuthStep::Prompt { flow, fields } = step else {
        panic!("expected a prompt");
    };
    assert_eq!(flow.as_str(), "service_account_key");
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].id, "key_file");
    assert!(!fields[0].secret);
}

#[test]
fn cancelled_key_prompt_is_an_auth_error() {
    let p = provider(Arc::new(FakeHttp::default()));
    let flow = clouddeck_extension_api::FlowId::from("service_account_key");
    let error = p
        .continue_auth(
            &call(&TestHost),
            &flow,
            clouddeck_extension_api::AuthAnswer::Cancel,
        )
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Auth);
    assert_eq!(error.message, "sign-in cancelled");
}

#[test]
fn unreadable_key_file_names_its_path() {
    let dir = temp_folder("bad-key");
    let bad = dir.join("missing.json");
    let p = provider(Arc::new(FakeHttp::default()));
    let flow = clouddeck_extension_api::FlowId::from("service_account_key");
    let values = [("key_file".to_string(), bad.display().to_string())]
        .into_iter()
        .collect();
    let error = p
        .continue_auth(
            &call(&MemoryHost::default()),
            &flow,
            clouddeck_extension_api::AuthAnswer::Values(values),
        )
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::Auth);
    assert!(error.message.contains("missing.json"));
}

#[test]
fn key_file_sign_in_stores_the_path_and_lists_projects() {
    let dir = temp_folder("key-flow");
    let key_path = dir.join("key.json");
    std::fs::write(&key_path, service_account_key_json()).unwrap();
    let http = Arc::new(FakeHttp::default());
    let p = provider(Arc::clone(&http));
    let host = MemoryHost::default();
    let flow = clouddeck_extension_api::FlowId::from("service_account_key");
    let values = [("key_file".to_string(), key_path.display().to_string())]
        .into_iter()
        .collect();
    let step = p
        .continue_auth(
            &call(&host),
            &flow,
            clouddeck_extension_api::AuthAnswer::Values(values),
        )
        .unwrap();
    assert_eq!(step, clouddeck_extension_api::AuthStep::Done);
    assert_eq!(
        host.secret_value("service_account_key_file"),
        Some(key_path.display().to_string())
    );
    let forms = http.posted();
    let jwt_form = forms.last().unwrap();
    assert!(jwt_form.contains(&(
        "grant_type".to_string(),
        "urn:ietf:params:oauth:grant-type:jwt-bearer".to_string()
    )));
    assert!(
        jwt_form
            .iter()
            .any(|(k, v)| k == "assertion" && v.matches('.').count() == 2)
    );

    let projects = p.connections(&call(&host)).unwrap();
    assert_eq!(projects.len(), 2);
}

#[test]
fn sign_out_clears_the_key_file_path() {
    let host = MemoryHost::with_secret("service_account_key_file", "/tmp/key.json");
    let p = provider(Arc::new(FakeHttp::default()));
    p.sign_out(&call(&host), None).unwrap();
    assert_eq!(host.secret_value("service_account_key_file"), None);
}
