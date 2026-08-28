use crate::mock::MockComputeService;
use crate::scenario::{EC2Scenario, EC2ScenarioCatalog};
use crate::service::BoxError;

/// A named test scenario with a built-in compute service.
///
/// The scope wired here is exactly the three cases the
/// InstanceListViewModel/InstanceDetailViewModel tests need:
/// `small-happy-path` (load success), `empty-fleet` (empty state), and one
/// generic failure case (fixed error via [`MockComputeService`], no fault
/// injection or extra fixture needed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestScenarioKind {
    SmallHappyPath,
    EmptyFleet,
    Failure,
}

impl TestScenarioKind {
    /// Scenario name used to look the fixture up in a catalog.
    ///
    /// The failure case is synthesized (a [`MockComputeService`] with a fixed
    /// error), so it has no fixture and no catalog name.
    pub fn fixture_name(self) -> Option<&'static str> {
        match self {
            Self::SmallHappyPath => Some("small-happy-path"),
            Self::EmptyFleet => Some("empty-fleet"),
            Self::Failure => None,
        }
    }

    fn display_name(self) -> &'static str {
        match self {
            Self::SmallHappyPath => "small-happy-path",
            Self::EmptyFleet => "empty-fleet",
            Self::Failure => "failure",
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
    service: MockComputeService,
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
                (Some(scenario), service)
            }
            TestScenarioKind::Failure => (
                None,
                MockComputeService::new(
                    vec![],
                    Some(Box::new(ScenarioFailureError::new(DEFAULT_FAILURE_MESSAGE))),
                ),
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

    /// The compute service to hand to a view model under test.
    pub fn service(&self) -> &MockComputeService {
        &self.service
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
        assert!(service.list_instances_error().is_some());
    }

    #[tokio::test]
    async fn failure_case_has_no_scenario_data() {
        let resolved = ScenarioService::for_kind(TestScenarioKind::Failure, &catalog()).unwrap();
        assert!(resolved.scenario().is_none());
        assert!(TestScenarioKind::Failure.fixture_name().is_none());
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
