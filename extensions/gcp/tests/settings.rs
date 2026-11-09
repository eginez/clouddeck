mod common;

use std::sync::Arc;

use clouddeck_core::extension_host::Manifest;
use clouddeck_extension_api::conformance::check_manifest_settings;
use clouddeck_extension_api::{
    Auth, AuthStatus, Browse, ConnectionId, ErrorKind, KindId, ProviderError, Selection,
};
use common::{FakeHttp, SettingsHost, call, provider, provider_with};

const PROJECTS: &str = "/v1/projects?filter=lifecycleState%3AACTIVE";

fn urls(http: &FakeHttp) -> Vec<String> {
    http.urls.lock().unwrap().clone()
}

#[test]
fn manifest_settings_match_the_code() {
    let manifest = Manifest::parse(include_str!("../extension.toml")).unwrap();
    let p = provider(Arc::new(FakeHttp::default()));
    let violations = check_manifest_settings(&p, &manifest.settings);
    assert!(violations.is_empty(), "{}", violations.join("\n"));
}

#[test]
fn defaults_use_the_google_endpoints() {
    let http = Arc::new(FakeHttp::default());
    let p = provider(Arc::clone(&http));
    p.connections(&call(&SettingsHost(vec![]))).unwrap();
    let urls = urls(&http);
    assert!(urls.contains(&format!(
        "https://cloudresourcemanager.googleapis.com{PROJECTS}"
    )));
    assert!(urls.contains(&"https://oauth2.googleapis.com/token".to_string()));
}

#[test]
fn custom_endpoints_receive_every_request() {
    let host = SettingsHost(vec![
        ("compute_endpoint", "http://localhost:4588"),
        ("resource_manager_endpoint", "http://localhost:4588/"),
        ("token_endpoint", "http://localhost:4588/token"),
    ]);
    let http = Arc::new(FakeHttp::default());
    let p = provider(Arc::clone(&http));
    assert!(!p.connections(&call(&host)).unwrap().is_empty());
    let listed = p.list(
        &call(&host),
        &ConnectionId::from("demo-prod"),
        &KindId::instances(),
        &Selection::new(),
    );
    assert!(!listed.resources.is_empty());
    let urls = urls(&http);
    assert!(urls.contains(&"http://localhost:4588/token".to_string()));
    assert!(urls.contains(&format!("http://localhost:4588{PROJECTS}")));
    assert!(urls.contains(
        &"http://localhost:4588/compute/v1/projects/demo-prod/aggregated/instances".to_string()
    ));
    assert!(urls.iter().all(|u| u.starts_with("http://localhost:4588")));
}

#[test]
fn local_emulator_addresses_are_accepted() {
    for base in ["http://127.0.0.1:4588", "http://host.docker.internal:4588"] {
        let host = SettingsHost(vec![
            ("resource_manager_endpoint", base),
            ("compute_endpoint", base),
            ("token_endpoint", base),
        ]);
        let http = Arc::new(FakeHttp::default());
        let p = provider(Arc::clone(&http));
        assert!(!p.connections(&call(&host)).unwrap().is_empty(), "{base}");
        assert!(urls(&http).iter().all(|u| u.starts_with(base)), "{base}");
    }
}

#[test]
fn plain_http_to_other_hosts_is_rejected_before_any_request() {
    for bad in [
        "http://compute.googleapis.com",
        "http://localhost.evil.example",
        "http://localhost:4588@evil.example",
        "http://127.0.0.1:4588@evil.example/",
        "http://localhost:4588x",
        "ftp://localhost:4588",
        "https://",
    ] {
        let host = SettingsHost(vec![("compute_endpoint", bad)]);
        let http = Arc::new(FakeHttp::default());
        let p = provider(Arc::clone(&http));
        let error = p.connections(&call(&host)).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Other, "{bad}");
        assert!(error.message.contains("compute_endpoint"), "{bad}");
        assert!(urls(&http).is_empty(), "{bad} reached the network");
        assert_eq!(p.auth_status(&call(&host), None), AuthStatus::Unknown);
    }
}

#[test]
fn emulator_mode_sends_the_fixed_token_without_reading_adc() {
    let host = SettingsHost(vec![
        ("auth_mode", "emulator"),
        ("resource_manager_endpoint", "http://localhost:4588"),
        ("compute_endpoint", "http://localhost:4588"),
    ]);
    let http = Arc::new(FakeHttp {
        bearer: "floci-emulator",
        ..Default::default()
    });
    let missing = ProviderError::new(ErrorKind::Auth, "no ADC");
    let p = provider_with(Arc::clone(&http), Err(missing));
    assert!(p.connections(&call(&host)).is_ok());
    assert_eq!(
        p.auth_status(&call(&host), None),
        AuthStatus::SignedIn { expires: None }
    );
    let urls = urls(&http);
    assert!(!urls.is_empty());
    assert!(urls.iter().all(|u| u.starts_with("http://localhost:4588")));
}

#[test]
fn unknown_auth_mode_is_rejected_before_any_request() {
    let host = SettingsHost(vec![("auth_mode", "service-account")]);
    let http = Arc::new(FakeHttp::default());
    let p = provider(Arc::clone(&http));
    let error = p.connections(&call(&host)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Other);
    assert!(error.message.contains("auth_mode"));
    assert!(urls(&http).is_empty());
}

#[test]
fn switching_the_token_endpoint_does_not_reuse_the_cached_token() {
    let http = Arc::new(FakeHttp::default());
    let p = provider(Arc::clone(&http));
    p.connections(&call(&SettingsHost(vec![]))).unwrap();
    let host = SettingsHost(vec![("token_endpoint", "http://localhost:4588/token")]);
    p.connections(&call(&host)).unwrap();
    assert_eq!(http.count("oauth2.googleapis.com"), 1);
    assert_eq!(http.count("localhost:4588/token"), 1);
}
