use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Call, ConnectionId, ErrorKind, FlowId,
    Host, KindId, ListResult, LogLevel, Provider, ProviderError, ResourceId, Selection, ValueId,
};

use crate::demo_provider::{DemoProvider, demo_provider};

struct NullHost;

impl Host for NullHost {
    fn log(&self, _level: LogLevel, _message: &str) {}

    fn data_dir(&self) -> PathBuf {
        PathBuf::new()
    }

    fn setting(&self, _key: &str) -> Option<String> {
        None
    }

    fn secret(&self, _key: &str) -> Option<String> {
        None
    }

    fn set_secret(&self, _key: &str, _value: Option<&str>) -> Result<(), ProviderError> {
        Ok(())
    }

    fn open_url(&self, _url: &str) -> Result<(), ProviderError> {
        Ok(())
    }

    fn cancelled(&self) -> bool {
        false
    }
}

static HOST: NullHost = NullHost;

fn call() -> Call<'static> {
    Call {
        deadline: None,
        host: &HOST,
    }
}

fn profile() -> ConnectionId {
    ConnectionId::from("scenario-default")
}

fn region_selection(regions: &[&str]) -> Selection {
    Selection::new().with(
        "region",
        regions
            .iter()
            .map(|region| ValueId::from(*region))
            .collect(),
    )
}

fn list_instances(provider: &DemoProvider, selection: &Selection) -> ListResult {
    provider.list(&call(), &profile(), &KindId::instances(), selection)
}

fn resource_ids(result: &ListResult) -> Vec<&str> {
    result.resources.iter().map(|r| r.id.as_str()).collect()
}

const INSTANCE_IDS: [&str; 3] = [
    "us-east-1:i-scenario-001",
    "us-east-1:i-scenario-002",
    "us-east-1:i-scenario-003",
];

#[test]
fn info_identifies_the_demo_provider() {
    let info = demo_provider().info();
    assert_eq!(info.id, "demo");
    assert_eq!(info.name, "Demo");
    assert_eq!(info.connection_label, "Profile");
}

#[test]
fn auth_is_a_no_op_sign_in() {
    let provider = demo_provider();
    let methods = provider.auth_methods();
    assert_eq!(methods.len(), 1);
    assert_eq!(methods[0].id, "demo");

    let input = || AuthInput {
        connection: Some(profile()),
        values: BTreeMap::new(),
    };
    assert_eq!(
        provider.begin_auth(&call(), &methods[0], input()),
        Ok(AuthStep::Done)
    );

    let sso = AuthMethod {
        id: "sso".to_string(),
        label: "SSO".to_string(),
    };
    assert_eq!(
        provider
            .begin_auth(&call(), &sso, input())
            .unwrap_err()
            .kind,
        ErrorKind::Unsupported
    );

    assert_eq!(
        provider
            .continue_auth(&call(), &FlowId::from("x"), AuthAnswer::Poll)
            .unwrap_err()
            .kind,
        ErrorKind::Auth
    );

    let signed_in = AuthStatus::SignedIn { expires: None };
    assert_eq!(provider.auth_status(&call(), None), signed_in);
    assert_eq!(provider.auth_status(&call(), Some(&profile())), signed_in);
    assert_eq!(provider.sign_out(&call(), Some(&profile())), Ok(()));
}

#[test]
fn connections_are_the_fixture_profiles() {
    let connections = demo_provider().connections(&call()).unwrap();
    let ids: Vec<&str> = connections.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, ["scenario-default"]);
}

#[test]
fn dimensions_expose_the_fixture_regions() {
    let provider = demo_provider();
    let dimensions = provider.dimensions(&call(), &profile()).unwrap();
    assert_eq!(dimensions.len(), 1);

    let region = &dimensions[0];
    assert_eq!(region.id.as_str(), "region");
    assert!(region.multi);
    assert!(!region.optional);
    let values: Vec<(&str, bool)> = region
        .values
        .iter()
        .map(|v| (v.id.as_str(), v.default))
        .collect();
    assert_eq!(values, [("us-east-1", true)]);

    assert_eq!(
        provider
            .dimensions(&call(), &ConnectionId::from("nope"))
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
}

#[test]
fn kinds_are_instances_only() {
    let ids: Vec<String> = demo_provider()
        .kinds()
        .into_iter()
        .map(|k| k.id.as_str().to_string())
        .collect();
    assert_eq!(ids, ["instances"]);
}

#[test]
fn list_returns_instances_for_a_known_region() {
    let provider = demo_provider();
    let result = list_instances(&provider, &region_selection(&["us-east-1"]));
    assert_eq!(resource_ids(&result), INSTANCE_IDS);
    assert!(result.failures.is_empty());
}

#[test]
fn list_reports_an_unknown_region_as_a_cell_failure() {
    let provider = demo_provider();
    let result = list_instances(&provider, &region_selection(&["us-east-1", "mars-1"]));
    assert_eq!(resource_ids(&result), INSTANCE_IDS);
    assert_eq!(result.failures.len(), 1);

    let failure = &result.failures[0];
    let cell: Vec<&str> = failure
        .cell
        .values("region")
        .iter()
        .map(|v| v.as_str())
        .collect();
    assert_eq!(cell, ["mars-1"]);
    assert_eq!(failure.error.kind, ErrorKind::NotFound);
}

#[test]
fn list_with_an_empty_selection_is_empty() {
    let provider = demo_provider();
    assert_eq!(
        list_instances(&provider, &Selection::new()),
        ListResult::default()
    );
}

#[test]
fn list_of_an_unknown_kind_is_unsupported() {
    let provider = demo_provider();
    let result = provider.list(
        &call(),
        &profile(),
        &KindId::from("buckets"),
        &region_selection(&["us-east-1"]),
    );
    assert!(result.resources.is_empty());
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].error.kind, ErrorKind::Unsupported);
}

#[test]
fn describe_returns_the_listed_instance_extras() {
    let provider = demo_provider();
    let listed = list_instances(&provider, &region_selection(&["us-east-1"]));
    let expected = listed.resources[0].extras.clone();

    let extras = provider
        .describe(
            &call(),
            &profile(),
            &KindId::instances(),
            &ResourceId::from("us-east-1:i-scenario-001"),
        )
        .unwrap();
    assert_eq!(extras, expected);

    assert_eq!(
        provider
            .describe(
                &call(),
                &profile(),
                &KindId::instances(),
                &ResourceId::from("i-missing"),
            )
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
}

#[test]
fn console_url_is_absent_for_every_instance() {
    let provider = demo_provider();
    let listed = list_instances(&provider, &region_selection(&["us-east-1"]));
    for instance in &listed.resources {
        assert_eq!(
            provider.console_url(&profile(), &KindId::instances(), instance),
            None
        );
    }
}

#[test]
fn fields_and_inspector_reuse_the_aws_definitions() {
    let provider = demo_provider();
    let instances = KindId::instances();
    assert_eq!(provider.fields(&instances), crate::aws_fields());
    assert_eq!(
        provider.inspector(&instances).len(),
        crate::aws_inspector::aws_inspector().len()
    );

    let buckets = KindId::from("buckets");
    assert!(provider.fields(&buckets).is_empty());
    assert!(provider.inspector(&buckets).is_empty());
}

#[test]
fn demo_provider_is_usable_as_a_shared_provider() {
    let provider: Arc<dyn Provider> = Arc::new(demo_provider());
    assert_eq!(provider.info().id, "demo");
}
