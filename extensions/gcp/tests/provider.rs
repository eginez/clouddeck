mod common;

use std::sync::Arc;

use clouddeck_extension_api::{
    AuthInput, AuthStatus, AuthStep, ConnectionId, ErrorKind, KindId, Provider, ResourceId,
    Selection, StateKind, ValueId,
};
use common::{FakeHttp, TestHost, call, fixture, provider, provider_with};

fn prod() -> ConnectionId {
    ConnectionId::from("demo-prod")
}

fn list(p: &impl Provider, zones: &[&str]) -> clouddeck_extension_api::ListResult {
    let host = TestHost;
    let ids = zones.iter().map(|z| ValueId::from(*z)).collect();
    let selection = Selection::new().with("zone", ids);
    p.list(&call(&host), &prod(), &KindId::instances(), &selection)
}

fn names(result: &clouddeck_extension_api::ListResult) -> Vec<&str> {
    result.resources.iter().map(|i| i.id.as_str()).collect()
}

#[test]
fn unfiltered_list_returns_every_instance_and_the_unreachable_zone() {
    let p = provider(Arc::new(FakeHttp::default()));
    let result = list(&p, &[]);
    assert_eq!(
        names(&result),
        [
            "europe-west1-b/batch-1",
            "us-central1-a/web-1",
            "us-central1-a/db-1"
        ]
    );
    assert_eq!(result.failures.len(), 1);
    let failure = &result.failures[0];
    assert_eq!(
        failure.cell.values("zone"),
        [ValueId::from("us-central1-b")]
    );
    assert!(failure.error.message.contains("UNREACHABLE"));
}

#[test]
fn region_selection_expands_to_its_zones() {
    let p = provider(Arc::new(FakeHttp::default()));
    let result = list(&p, &["europe-west1"]);
    assert_eq!(names(&result), ["europe-west1-b/batch-1"]);
    assert!(result.failures.is_empty());
    assert_eq!(result.resources[0].state.kind, StateKind::Starting);
}

#[test]
fn zone_selection_lists_only_that_zone() {
    let p = provider(Arc::new(FakeHttp::default()));
    let result = list(&p, &["us-central1-a"]);
    assert_eq!(
        names(&result),
        ["us-central1-a/web-1", "us-central1-a/db-1"]
    );
    assert_eq!(result.resources[1].state.kind, StateKind::Stopped);
}

#[test]
fn unknown_zone_is_a_cell_failure() {
    let p = provider(Arc::new(FakeHttp::default()));
    let result = list(&p, &["mars-1-a"]);
    assert!(result.resources.is_empty());
    assert_eq!(result.failures[0].error.kind, ErrorKind::NotFound);
}

#[test]
fn describe_round_trips_a_listed_id_and_rejects_others() {
    let p = provider(Arc::new(FakeHttp::default()));
    let host = TestHost;
    let kind = KindId::instances();
    let id = ResourceId::from("us-central1-a/web-1");
    let sections = p.describe(&call(&host), &prod(), &kind, &id).unwrap();
    assert_eq!(sections[0].title, "Machine");
    let bad = ResourceId::from("us-central1-a/../../x");
    let error = p.describe(&call(&host), &prod(), &kind, &bad).unwrap_err();
    assert_eq!(error.kind, ErrorKind::NotFound);
}

#[test]
fn token_is_cached_across_calls() {
    let http = Arc::new(FakeHttp::default());
    let p = provider(Arc::clone(&http));
    list(&p, &[]);
    list(&p, &["europe-west1"]);
    assert_eq!(http.count("oauth2.googleapis.com"), 1);
}

#[test]
fn missing_adc_asks_the_user_to_sign_in() {
    let missing = clouddeck_extension_api::ProviderError::new(ErrorKind::Auth, "no ADC");
    let p = provider_with(Arc::new(FakeHttp::default()), Err(missing));
    let host = TestHost;
    let method = p.auth_methods().remove(0);
    let input = AuthInput {
        connection: None,
        values: Default::default(),
    };
    let error = p.begin_auth(&call(&host), &method, input).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Auth);
    assert_eq!(p.auth_status(&call(&host), None), AuthStatus::SignedOut);
    assert_eq!(
        p.connections(&call(&host)).unwrap_err().kind,
        ErrorKind::Auth
    );
}

#[test]
fn usable_adc_completes_auth() {
    let p = provider(Arc::new(FakeHttp::default()));
    let host = TestHost;
    let method = p.auth_methods().remove(0);
    let input = AuthInput {
        connection: None,
        values: Default::default(),
    };
    assert_eq!(
        p.begin_auth(&call(&host), &method, input),
        Ok(AuthStep::Done)
    );
    assert!(fixture("token.json").contains("access_token"));
}

#[test]
fn rejected_refresh_token_is_an_auth_error_with_advice() {
    let http = Arc::new(FakeHttp {
        fail_token: true,
        ..Default::default()
    });
    let p = provider(http);
    let error = p.connections(&call(&TestHost)).unwrap_err();
    assert_eq!(error.kind, ErrorKind::Auth);
    assert!(error.advice.is_some());
}
