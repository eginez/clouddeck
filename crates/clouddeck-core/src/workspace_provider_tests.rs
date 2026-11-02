use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use clouddeck_provider_api::{
    Call, Connection, ConnectionId, Dimension, ErrorKind, FieldDef, Host, Instance, KindId,
    KindInfo, ListResult, LogLevel, Provider, ProviderError, ProviderInfo, Selection, ValueId,
};

use crate::console_url::console_url_for_instance;
use crate::scoped_provider::{ScopeHost, ScopeSettings};
use crate::workspace_provider::service;
use crate::{
    DemoFleet, bound_provider, builtin_providers, demo_provider, service_provider,
    workspace_provider,
};

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

fn call_with_deadline() -> Call<'static> {
    Call {
        deadline: Some(Instant::now() + Duration::from_secs(60)),
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
fn other_demo_paths_list_through_the_demo_entry() {
    let providers = builtin_providers(true);
    let fleet = workspace_provider(
        &providers,
        "scenario-default",
        "fleet",
        Some(DemoFleet::Happy),
        &["us-east-1".to_string()],
        Duration::from_secs(5),
    );
    assert_eq!(fleet.info().id, "demo");

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
        assert_eq!(provider.info().id, "demo", "{scenario:?}");
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

#[test]
fn scope_settings_round_trip_through_the_host() {
    let settings = ScopeSettings {
        request_timeout: Duration::from_millis(7000),
        fleet: true,
        demo: Some(DemoFleet::Partial),
    };
    let host = ScopeHost::new(&HOST, settings);
    assert_eq!(ScopeSettings::from_host(&host), Some(settings));
    assert_eq!(ScopeSettings::from_host(&HOST), None);
}

fn previous_listing(
    profile: &str,
    region: &str,
    demo: Option<DemoFleet>,
    fleet_regions: &[String],
    request_timeout: Duration,
) -> ListResult {
    let connection = ConnectionId::from(profile);
    let provider: Arc<dyn Provider> =
        if demo == Some(DemoFleet::Happy) && region != "fleet" && !profile.is_empty() {
            bound_provider(
                Arc::new(demo_provider()),
                connection.clone(),
                region_selection(&[region]),
            )
        } else {
            service_provider(service(
                profile,
                region,
                demo,
                fleet_regions,
                request_timeout,
            ))
        };
    provider.list(
        &call(),
        &connection,
        &KindId::instances(),
        &Selection::new(),
    )
}

#[test]
fn scoped_listing_matches_the_old_paths() {
    let providers = builtin_providers(true);
    let timeout = Duration::from_secs(5);
    let fleet_regions = ["us-east-1".to_string(), "eu-west-1".to_string()];
    let scenarios = [
        DemoFleet::Happy,
        DemoFleet::Empty,
        DemoFleet::Failure,
        DemoFleet::Partial,
        DemoFleet::Large,
    ];
    for demo in scenarios {
        for (region, regions) in [("us-east-1", &[][..]), ("fleet", &fleet_regions[..])] {
            let expected =
                previous_listing("scenario-default", region, Some(demo), regions, timeout);
            let actual = workspace_provider(
                &providers,
                "scenario-default",
                region,
                Some(demo),
                regions,
                timeout,
            )
            .list(
                &call(),
                &ConnectionId::from("scenario-default"),
                &KindId::instances(),
                &Selection::new(),
            );
            assert_eq!(actual, expected, "{demo:?} {region}");
        }
    }
    for region in ["us-east-1", "fleet"] {
        let expected = previous_listing("", region, None, &fleet_regions, timeout);
        let actual = workspace_provider(&providers, "", region, None, &fleet_regions, timeout)
            .list(
                &call(),
                &ConnectionId::from(""),
                &KindId::instances(),
                &Selection::new(),
            );
        assert_eq!(actual, expected, "no profile {region}");
    }
}

#[derive(Debug, PartialEq)]
struct Seen {
    connection: String,
    regions: Vec<String>,
    request_timeout: Option<String>,
    fleet: Option<String>,
    demo: Option<String>,
    deadline: bool,
}

struct Spy {
    id: &'static str,
    answer: Vec<Instance>,
    seen: Mutex<Vec<Seen>>,
}

impl Spy {
    fn new(id: &'static str) -> Arc<Self> {
        Self::returning(id, Vec::new())
    }

    fn returning(id: &'static str, answer: Vec<Instance>) -> Arc<Self> {
        Arc::new(Self {
            id,
            answer,
            seen: Mutex::new(Vec::new()),
        })
    }

    fn seen(&self) -> Vec<Seen> {
        std::mem::take(&mut *self.seen.lock().unwrap())
    }
}

impl Provider for Spy {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.id.to_string(),
            name: self.id.to_string(),
            version: "0".to_string(),
            connection_label: "Profile".to_string(),
        }
    }

    fn connections(&self, _cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        Ok(Vec::new())
    }

    fn dimensions(
        &self,
        _cx: &Call<'_>,
        _connection: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError> {
        Ok(Vec::new())
    }

    fn kinds(&self) -> Vec<KindInfo> {
        Vec::new()
    }

    fn list(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        _kind: &KindId,
        selection: &Selection,
    ) -> ListResult {
        self.seen.lock().unwrap().push(Seen {
            connection: connection.as_str().to_string(),
            regions: selection
                .values("region")
                .iter()
                .map(|value| value.as_str().to_string())
                .collect(),
            request_timeout: cx.host.setting("clouddeck.request_timeout_ms"),
            fleet: cx.host.setting("clouddeck.fleet"),
            demo: cx.host.setting("clouddeck.demo_scenario"),
            deadline: cx.deadline.is_some(),
        });
        ListResult {
            resources: self.answer.clone(),
            failures: Vec::new(),
        }
    }

    fn console_url(
        &self,
        _connection: &ConnectionId,
        _kind: &KindId,
        _resource: &Instance,
    ) -> Option<String> {
        None
    }

    fn fields(&self, _kind: &KindId) -> Vec<FieldDef> {
        Vec::new()
    }
}

#[test]
fn listing_goes_through_the_registry_entry() {
    let aws = Spy::new("aws");
    let providers: Vec<Arc<dyn Provider>> = vec![aws.clone()];
    let fleet_regions = ["us-east-1".to_string(), "eu-west-1".to_string()];
    workspace_provider(
        &providers,
        "prof",
        "fleet",
        None,
        &fleet_regions,
        Duration::from_secs(7),
    )
    .list(
        &call_with_deadline(),
        &ConnectionId::from("default"),
        &KindId::instances(),
        &Selection::new(),
    );
    workspace_provider(
        &providers,
        "prof",
        "us-west-2",
        None,
        &[],
        Duration::from_secs(7),
    )
    .list(
        &call_with_deadline(),
        &ConnectionId::from("default"),
        &KindId::instances(),
        &Selection::new(),
    );
    assert_eq!(
        aws.seen(),
        [
            Seen {
                connection: "prof".to_string(),
                regions: fleet_regions.to_vec(),
                request_timeout: Some("7000".to_string()),
                fleet: Some("true".to_string()),
                demo: None,
                deadline: true,
            },
            Seen {
                connection: "prof".to_string(),
                regions: vec!["us-west-2".to_string()],
                request_timeout: Some("7000".to_string()),
                fleet: Some("false".to_string()),
                demo: None,
                deadline: true,
            },
        ]
    );
}

#[test]
fn extension_shaped_demo_entry_lists_from_connection_and_selection() {
    let instance = demo_provider()
        .list(
            &call(),
            &ConnectionId::from("scenario-default"),
            &KindId::instances(),
            &region_selection(&["us-east-1"]),
        )
        .resources
        .remove(0);
    let demo = Spy::returning("demo", vec![instance.clone()]);
    let providers: Vec<Arc<dyn Provider>> = vec![demo.clone()];
    let result = happy_provider(&providers, "us-east-1").list(
        &call(),
        &ConnectionId::from("default"),
        &KindId::instances(),
        &Selection::new(),
    );
    assert_eq!(result.resources, vec![instance]);
    assert_eq!(
        demo.seen(),
        [Seen {
            connection: "scenario-default".to_string(),
            regions: vec!["us-east-1".to_string()],
            request_timeout: Some("5000".to_string()),
            fleet: Some("false".to_string()),
            demo: Some("happy".to_string()),
            deadline: false,
        }]
    );
}
