mod common;

use std::time::Duration;

use clouddeck_core::{DemoFleet, workspace_provider};
use clouddeck_provider_api::{ConnectionId, KindId, ListResult, Selection};

use common::{call, happy_provider};

#[test]
fn happy_demo_falls_back_to_aws_without_a_demo_provider() {
    let providers = clouddeck_aws::builtin_providers(false);
    assert_eq!(happy_provider(&providers, "us-east-1").info().id, "aws");
}

#[test]
fn demo_scenarios_stay_demo_data_without_a_demo_entry() {
    let providers = clouddeck_aws::builtin_providers(false);
    let list = |demo| {
        workspace_provider(
            &providers,
            "scenario-default",
            "us-east-1",
            Some(demo),
            &[],
            Duration::from_secs(5),
        )
        .list(
            &call(),
            &ConnectionId::from("scenario-default"),
            &KindId::instances(),
            &Selection::new(),
        )
    };
    assert_eq!(list(DemoFleet::Empty), ListResult::default());
    let failure = list(DemoFleet::Failure);
    assert!(failure.resources.is_empty());
    assert_eq!(failure.failures.len(), 1);
}
