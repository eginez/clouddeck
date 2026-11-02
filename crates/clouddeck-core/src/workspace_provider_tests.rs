use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use clouddeck_provider_api::{
    Call, ConnectionId, ErrorKind, Host, KindId, ListResult, LogLevel, Provider, ProviderError,
    Selection, ValueId,
};

use crate::console_url::console_url_for_instance;
use crate::{DemoFleet, bound_provider, builtin_providers, demo_provider, workspace_provider};

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

fn region_selection(regions: &[&str]) -> Selection {
    Selection::new().with(
        "region",
        regions.iter().map(|region| ValueId::from(*region)).collect(),
    )
}

fn happy_provider(providers: &[Arc<dyn Provider>], region: &str) -> Arc<dyn Provider> {
    workspace_provider(
        providers,
        "scenario-default",
        region,
        Some(DemoFleet::Happy),
        &[],
        Duration::from_secs(5),
    )
}

fn instance_ids(result: &ListResult) -> Vec<String> {
    result
        .resources
        .iter()
        .map(|instance| instance.id.clone())
        .collect()
}

#[test]
fn demo_instances_carry_their_region() {
    let result = demo_provider().list(
        &call(),
        &ConnectionId::from("scenario-default"),
        &KindId::instances(),
        &region_selection(&["us-east-1"]),
    );
    assert!(!result.resources.is_empty());
    assert!(
        result
            .resources
            .iter()
            .all(|instance| instance.region.as_deref() == Some("us-east-1"))
    );
}

#[test]
fn happy_demo_lists_through_the_demo_provider() {
    let providers = builtin_providers(true);
    let provider = happy_provider(&providers, "us-east-1");
    assert_eq!(provider.info().id, "demo");

    let result = provider.list(
        &call(),
        &ConnectionId::from("default"),
        &KindId::instances(),
        &Selection::new(),
    );
    assert_eq!(
        instance_ids(&result),
        [
            "us-east-1:i-scenario-001",
            "us-east-1:i-scenario-002",
            "us-east-1:i-scenario-003",
        ]
    );
    assert!(result.failures.is_empty());
    assert!(
        result
            .resources
            .iter()
            .all(|instance| instance.region.as_deref() == Some("us-east-1"))
    );
}

#[test]
fn happy_demo_console_url_matches_the_aws_console_helper() {
    let providers = builtin_providers(true);
    let provider = happy_provider(&providers, "us-east-1");
    let kind = KindId::instances();
    let result = provider.list(
        &call(),
        &ConnectionId::from("default"),
        &kind,
        &Selection::new(),
    );
    let first = result.resources.first().expect("demo instance");
    assert_eq!(
        provider.console_url(&ConnectionId::from("default"), &kind, first),
        console_url_for_instance("us-east-1", "i-scenario-001").ok(),
    );
}

#[test]
fn other_demo_paths_stay_on_the_aws_service() {
    let providers = builtin_providers(true);
    let fleet = workspace_provider(
        &providers,
        "scenario-default",
        "fleet",
        Some(DemoFleet::Happy),
        &["us-east-1".to_string()],
        Duration::from_secs(5),
    );
    assert_eq!(fleet.info().id, "aws");

    for scenario in [
        DemoFleet::Empty,
        DemoFleet::Failure,
        DemoFleet::Partial,
        DemoFleet::Large,
    ] {
        let provider = workspace_provider(
            &providers,
            "scenario-default",
            "us-east-1",
            Some(scenario),
            &[],
            Duration::from_secs(5),
        );
        assert_eq!(provider.info().id, "aws", "{scenario:?}");
    }
}

#[test]
fn happy_demo_falls_back_to_aws_without_a_demo_provider() {
    let providers = builtin_providers(false);
    assert_eq!(happy_provider(&providers, "us-east-1").info().id, "aws");
}

#[test]
fn bound_provider_uses_a_non_empty_incoming_selection() {
    let provider = bound_provider(
        Arc::new(demo_provider()),
        ConnectionId::from("scenario-default"),
        region_selection(&["us-east-1"]),
    );
    let result = provider.list(
        &call(),
        &ConnectionId::from("default"),
        &KindId::instances(),
        &region_selection(&["mars-1"]),
    );
    assert!(result.resources.is_empty());
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].error.kind, ErrorKind::NotFound);
}
