use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::fault_injecting::{
    FaultError, FaultInjectingComputeService, FaultInjectionBehavior, FaultInjectionOutcome, Sleep,
};
use crate::fleet_compute::FleetComputeService;
use crate::mock::MockComputeService;
use crate::models::{ComputeInstanceFilters, ComputeInstanceLoadResult};
use crate::scenario::{EC2Scenario, EC2ScenarioCatalog};
use crate::service::{BoxError, ComputeService};

/// A named test scenario with a built-in compute service.
///
/// Fixture-backed kinds (`small-happy-path`, `empty-fleet`, `multi-region`)
/// load JSON data; the failure kinds synthesize their faults in Rust through
/// [`FaultInjectingComputeService`] so the fixture schema stays pure data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestScenarioKind {
    SmallHappyPath,
    EmptyFleet,
    Failure,
    /// Every load fails with an error classified as `Throttled`.
    ThrottledFailure,
    /// Every load fails with an error classified as `Permission`.
    PermissionFailure,
    /// `multi-region` fixture served through a [`FleetComputeService`] where
    /// [`MULTI_REGION_FAILING_REGION`] fails and the other regions succeed.
    MultiRegionPartialFailure,
}

/// The region that fails in [`TestScenarioKind::MultiRegionPartialFailure`].
pub const MULTI_REGION_FAILING_REGION: &str = "eu-west-1";

/// Error text that `CloudDeckError::from_service_error` classifies as Throttled.
pub const THROTTLED_FAILURE_MESSAGE: &str = "RequestLimitExceeded: Rate exceeded";

/// Error text that `CloudDeckError::from_service_error` classifies as Permission.
pub const PERMISSION_FAILURE_MESSAGE: &str =
    "UnauthorizedOperation: You are not authorized to perform this operation.";

impl TestScenarioKind {
    /// Scenario name used to look the fixture up in a catalog.
    ///
    /// The failure case is synthesized (a [`MockComputeService`] with a fixed
    /// error), so it has no fixture and no catalog name.
    pub fn fixture_name(self) -> Option<&'static str> {
        match self {
            Self::SmallHappyPath => Some("small-happy-path"),
            Self::EmptyFleet => Some("empty-fleet"),
            Self::MultiRegionPartialFailure => Some("multi-region"),
            Self::Failure | Self::ThrottledFailure | Self::PermissionFailure => None,
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::SmallHappyPath => "small-happy-path",
            Self::EmptyFleet => "empty-fleet",
            Self::Failure => "failure",
            Self::ThrottledFailure => "throttled-failure",
            Self::PermissionFailure => "permission-failure",
            Self::MultiRegionPartialFailure => "multi-region-partial-failure",
        }
    }
}

impl std::fmt::Display for TestScenarioKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

/// Default error message for the generic failure scenario.
pub const DEFAULT_FAILURE_MESSAGE: &str =
    "Simulated compute failure: the configured scenario fault triggered.";

/// The fixed error type used by the generic failure scenario.
///
/// Kept separate from [`crate::CloudDeckError`] so a view-model under test
/// cannot special-case "this is just the test double" — it is an ordinary
/// error like any injected failure (matching how the Swift oracle injects
/// arbitrary `Error` values through `MockComputeService`).
#[derive(Debug)]
pub struct ScenarioFailureError {
    message: String,
}

impl ScenarioFailureError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ScenarioFailureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ScenarioFailureError {}

/// A resolved test scenario: its kind, the underlying scenario data (fixture
/// cases), and a compute service configured to serve it.
#[derive(Debug)]
pub struct ScenarioService {
    kind: TestScenarioKind,
    scenario: Option<EC2Scenario>,
    service: SharedService,
}

/// Cheaply clonable handle to the scenario's compute service.
#[derive(Clone)]
pub struct SharedService(Arc<dyn ComputeService + Send + Sync>);

impl std::fmt::Debug for SharedService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SharedService")
    }
}

#[async_trait::async_trait]
impl ComputeService for SharedService {
    async fn list_instances(
        &self,
        filters: Option<ComputeInstanceFilters>,
    ) -> Result<ComputeInstanceLoadResult, BoxError> {
        self.0.list_instances(filters).await
    }
}

/// Sleep that returns immediately: fault scenarios never really wait.
struct NoopSleep;

impl Sleep for NoopSleep {
    fn sleep(&self, _duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
}

/// A service that always fails with `message`, via the fault-injecting wrapper.
fn failing_service(message: &str) -> FaultInjectingComputeService {
    FaultInjectingComputeService::with_sleep(
        MockComputeService::new(vec![], None),
        FaultInjectionBehavior::default().with_outcome(FaultInjectionOutcome::Failure(
            FaultError::new(ScenarioFailureError::new(message)),
        )),
        NoopSleep,
    )
}

impl ScenarioService {
    /// Builds the compute service for a scenario kind.
    ///
    /// `small-happy-path` and `empty-fleet` are loaded from the catalog by
    /// fixture name and served region-by-region through a
    /// [`MockComputeService`]; `failure` serves the fixed error directly.
    pub fn for_kind(
        kind: TestScenarioKind,
        catalog: &impl EC2ScenarioCatalog,
    ) -> Result<Self, BoxError> {
        let (scenario, service) = match kind {
            TestScenarioKind::SmallHappyPath | TestScenarioKind::EmptyFleet => {
                let name = kind
                    .fixture_name()
                    .expect("fixture scenarios carry a fixture name");
                let scenario = catalog.scenario(name)?;
                let service = scenario_service_for(&scenario)?;
                (Some(scenario), SharedService(Arc::new(service)))
            }
            TestScenarioKind::MultiRegionPartialFailure => {
                let name = kind
                    .fixture_name()
                    .expect("fixture scenarios carry a fixture name");
                let scenario = catalog.scenario(name)?;
                let service = multi_region_service_for(&scenario)?;
                (Some(scenario), SharedService(Arc::new(service)))
            }
            TestScenarioKind::ThrottledFailure => (
                None,
                SharedService(Arc::new(failing_service(THROTTLED_FAILURE_MESSAGE))),
            ),
            TestScenarioKind::PermissionFailure => (
                None,
                SharedService(Arc::new(failing_service(PERMISSION_FAILURE_MESSAGE))),
            ),
            TestScenarioKind::Failure => (
                None,
                SharedService(Arc::new(MockComputeService::new(
                    vec![],
                    Some(Box::new(ScenarioFailureError::new(DEFAULT_FAILURE_MESSAGE))),
                ))),
            ),
        };

        Ok(Self {
            kind,
            scenario,
            service,
        })
    }

    pub fn kind(&self) -> TestScenarioKind {
        self.kind
    }

    /// The underlying scenario data for fixture-backed scenarios.
    pub fn scenario(&self) -> Option<&EC2Scenario> {
        self.scenario.as_ref()
    }

    /// The compute service to hand to a view model under test (borrowed).
    pub fn service(&self) -> &SharedService {
        &self.service
    }

    /// An owned compute service to hand to a view model that holds its
    /// service by value (e.g. `Box<dyn ComputeService>` or
    /// `Arc<dyn ComputeService>`).
    pub fn into_service(self) -> SharedService {
        self.service
    }

    /// A boxed compute service, for view models that take
    /// `Box<dyn ComputeService>`.
    pub fn boxed_service(&self) -> Box<dyn ComputeService> {
        Box::new(self.service.clone())
    }
}

/// Serves the scenario's default-region instances through a plain
/// [`MockComputeService`], matching the Swift oracle's wiring for
/// fixture-backed scenario data (the `FaultInjectingComputeService` /
/// `FleetComputeService` layers are deliberately not pulled in here).
fn scenario_service_for(scenario: &EC2Scenario) -> Result<MockComputeService, BoxError> {
    let Some((profile, region)) = scenario.default_selection() else {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Scenario '{}' has no default profile/region selection.",
                scenario.name
            ),
        )));
    };

    let instances = scenario.instances(Some(&profile), &region)?;
    Ok(MockComputeService::new(instances, None))
}

/// Serves every region of the scenario's first profile through a
/// [`FleetComputeService`]; [`MULTI_REGION_FAILING_REGION`] always fails.
fn multi_region_service_for(scenario: &EC2Scenario) -> Result<FleetComputeService, BoxError> {
    let Some((profile, _)) = scenario.default_selection() else {
        return Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "Scenario '{}' has no default profile/region selection.",
                scenario.name
            ),
        )));
    };
    let regions: Vec<String> = scenario
        .profiles
        .iter()
        .find(|p| p.name == profile)
        .map(|p| p.regions.iter().map(|r| r.name.clone()).collect())
        .unwrap_or_default();

    let scenario = scenario.clone();
    Ok(FleetComputeService::new(regions, move |region| {
        if region == MULTI_REGION_FAILING_REGION {
            return Box::new(failing_service(PERMISSION_FAILURE_MESSAGE));
        }
        let instances = scenario
            .instances(Some(&profile), region)
            .unwrap_or_default();
        Box::new(MockComputeService::new(instances, None))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ComputeInstanceFilters;
    use crate::scenario::DirectoryScenarioCatalog;
    use crate::service::ComputeService;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/ec2-scenarios");

    fn catalog() -> DirectoryScenarioCatalog {
        DirectoryScenarioCatalog::new(FIXTURES)
    }

    #[tokio::test]
    async fn small_happy_path_serves_parsed_fixture_instances() {
        let resolved =
            ScenarioService::for_kind(TestScenarioKind::SmallHappyPath, &catalog()).unwrap();
        let service = resolved.service();

        let result = service.list_instances(None).await.unwrap();
        let ids: Vec<&str> = result
            .instances
            .iter()
            .map(|i| i.instance_id.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["i-scenario-001", "i-scenario-002", "i-scenario-003"]
        );
        assert!(result.partial_failures.is_empty());
    }

    #[tokio::test]
    async fn small_happy_path_scenario_data_exposes_default_selection() {
        let resolved =
            ScenarioService::for_kind(TestScenarioKind::SmallHappyPath, &catalog()).unwrap();

        let scenario = resolved.scenario().unwrap();
        assert_eq!(
            scenario.default_selection(),
            Some(("scenario-default".to_string(), "us-east-1".to_string()))
        );
        assert_eq!(resolved.kind(), TestScenarioKind::SmallHappyPath);
    }

    #[tokio::test]
    async fn small_happy_path_supports_filters() {
        let resolved =
            ScenarioService::for_kind(TestScenarioKind::SmallHappyPath, &catalog()).unwrap();
        let service = resolved.service();

        let filters = ComputeInstanceFilters {
            name_contains: Some("bastion".to_string()),
            states: vec![],
            instance_types: vec![],
        };
        let result = service.list_instances(Some(filters)).await.unwrap();
        assert_eq!(result.instances.len(), 1);
        assert_eq!(result.instances[0].name(), Some("bastion-east-1"));
    }

    #[tokio::test]
    async fn empty_fleet_serves_empty_instance_list() {
        let resolved = ScenarioService::for_kind(TestScenarioKind::EmptyFleet, &catalog()).unwrap();
        let service = resolved.service();

        let result = service.list_instances(None).await.unwrap();
        assert!(result.instances.is_empty());
        assert!(result.partial_failures.is_empty());
    }

    #[tokio::test]
    async fn empty_fleet_scenario_data_is_empty_region_not_lookup_miss() {
        let resolved = ScenarioService::for_kind(TestScenarioKind::EmptyFleet, &catalog()).unwrap();

        let scenario = resolved.scenario().unwrap();
        let instances = scenario
            .instances(Some("scenario-default"), "us-east-1")
            .unwrap();
        assert!(instances.is_empty());
    }

    #[tokio::test]
    async fn failure_serves_fixed_error() {
        let resolved = ScenarioService::for_kind(TestScenarioKind::Failure, &catalog()).unwrap();
        let service = resolved.service();

        // Note: the error is re-boxed on every call, so downcasting the
        // returned `BoxError` to `ScenarioFailureError` does not work — the
        // same limitation applies to any injected error in
        // `MockComputeService`. The observable contract is the message.
        let err = service.list_instances(None).await.unwrap_err();
        assert_eq!(err.to_string(), DEFAULT_FAILURE_MESSAGE);
    }

    #[tokio::test]
    async fn failure_case_has_no_scenario_data() {
        let resolved = ScenarioService::for_kind(TestScenarioKind::Failure, &catalog()).unwrap();
        assert!(resolved.scenario().is_none());
        assert!(TestScenarioKind::Failure.fixture_name().is_none());
    }

    #[test]
    fn new_kinds_have_fixture_and_display_names() {
        assert!(TestScenarioKind::ThrottledFailure.fixture_name().is_none());
        assert!(TestScenarioKind::PermissionFailure.fixture_name().is_none());
        assert_eq!(
            TestScenarioKind::MultiRegionPartialFailure.fixture_name(),
            Some("multi-region")
        );
        assert_eq!(
            TestScenarioKind::ThrottledFailure.to_string(),
            "throttled-failure"
        );
        assert_eq!(
            TestScenarioKind::PermissionFailure.to_string(),
            "permission-failure"
        );
        assert_eq!(
            TestScenarioKind::MultiRegionPartialFailure.to_string(),
            "multi-region-partial-failure"
        );
    }

    #[test]
    fn multi_region_fixture_parses_with_three_distinct_regions() {
        let scenario = catalog().scenario("multi-region").unwrap();
        assert_eq!(scenario.profiles.len(), 1);
        let regions: Vec<&str> = scenario.profiles[0]
            .regions
            .iter()
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(
            regions,
            vec!["us-east-1", MULTI_REGION_FAILING_REGION, "ap-southeast-2"]
        );
        let mut ids: Vec<&str> = scenario.profiles[0]
            .regions
            .iter()
            .flat_map(|r| r.instances.iter().map(|i| i.instance_id.as_str()))
            .collect();
        let total = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), total);
    }

    #[tokio::test]
    async fn throttled_failure_classifies_as_throttled() {
        let resolved =
            ScenarioService::for_kind(TestScenarioKind::ThrottledFailure, &catalog()).unwrap();
        let err = resolved.service().list_instances(None).await.unwrap_err();
        assert_eq!(
            crate::CloudDeckError::from_service_error(err.as_ref()),
            crate::CloudDeckError::Throttled
        );
    }

    #[tokio::test]
    async fn permission_failure_classifies_as_permission() {
        let resolved =
            ScenarioService::for_kind(TestScenarioKind::PermissionFailure, &catalog()).unwrap();
        let err = resolved.service().list_instances(None).await.unwrap_err();
        assert_eq!(
            crate::CloudDeckError::from_service_error(err.as_ref()),
            crate::CloudDeckError::Permission
        );
    }

    #[tokio::test]
    async fn multi_region_partial_failure_returns_healthy_instances_and_one_failure() {
        let resolved =
            ScenarioService::for_kind(TestScenarioKind::MultiRegionPartialFailure, &catalog())
                .unwrap();
        let result = resolved.service().list_instances(None).await.unwrap();
        let ids: Vec<&str> = result
            .instances
            .iter()
            .map(|i| i.instance_id.as_str())
            .collect();
        assert_eq!(ids, vec!["i-mr-apse2-a", "i-mr-use1-a", "i-mr-use1-b"]);
        assert_eq!(
            result.partial_failures[0].message,
            crate::CloudDeckError::from_service_error(&ScenarioFailureError::new(
                PERMISSION_FAILURE_MESSAGE
            ))
            .user_message()
        );
        assert_eq!(result.partial_failures.len(), 1);
        assert_eq!(
            result.partial_failures[0].region,
            MULTI_REGION_FAILING_REGION
        );
    }

    #[tokio::test]
    async fn multi_region_with_every_region_failing_errors() {
        let scenario = catalog().scenario("multi-region").unwrap();
        let regions = scenario.profiles[0]
            .regions
            .iter()
            .map(|r| r.name.clone())
            .collect();
        let fleet = FleetComputeService::new(regions, |_| {
            Box::new(failing_service(THROTTLED_FAILURE_MESSAGE))
        });
        let err = fleet.list_instances(None).await.unwrap_err();
        assert_eq!(
            err.to_string(),
            crate::CloudDeckError::Throttled.user_message()
        );
    }

    #[tokio::test]
    async fn missing_fixture_is_reported() {
        let empty_catalog = DirectoryScenarioCatalog::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/testdata/does-not-exist"
        ));
        let err = ScenarioService::for_kind(TestScenarioKind::SmallHappyPath, &empty_catalog)
            .unwrap_err();
        assert!(err.to_string().contains("small-happy-path"));
    }

    #[tokio::test]
    async fn scenario_without_default_selection_is_rejected() {
        let catalog = StubCatalog::json(r#"{"name":"no-profile","profiles":[]}"#);
        let err =
            ScenarioService::for_kind(TestScenarioKind::SmallHappyPath, &catalog).unwrap_err();
        assert!(err.to_string().contains("no default profile/region"));
    }

    /// A single-scenario in-memory catalog for rejection tests.
    struct StubCatalog(String);

    impl StubCatalog {
        fn json(contents: &str) -> Self {
            Self(contents.to_string())
        }
    }

    impl EC2ScenarioCatalog for StubCatalog {
        fn scenario(&self, _name: &str) -> Result<EC2Scenario, BoxError> {
            crate::scenario::parse_scenario(&self.0)
        }

        fn scenario_names(&self) -> Vec<String> {
            vec![]
        }
    }
}
