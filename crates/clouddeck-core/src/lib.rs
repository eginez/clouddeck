mod error;
mod fault_injecting;
mod fleet_compute;
mod mock;
mod models;
mod scenario;
mod scenario_service;
mod service;

pub use error::CloudDeckError;
pub use fault_injecting::{
    DefaultSleep, FaultInjectingComputeService, FaultInjectionBehavior, FaultInjectionOutcome,
    Sleep,
};
pub use fleet_compute::FleetComputeService;
pub use mock::MockComputeService;
pub use models::{
    ComputeInstanceFilters, ComputeInstanceLoadResult, ComputeServicePartialFailure, EC2Instance,
    EC2InstanceTag, EC2SecurityGroup,
};
pub use scenario::{
    DirectoryScenarioCatalog, EC2Scenario, EC2ScenarioCatalog, EC2ScenarioProfile,
    EC2ScenarioRegion, parse_scenario,
};
pub use scenario_service::{
    DEFAULT_FAILURE_MESSAGE, ScenarioFailureError, ScenarioService, TestScenarioKind,
};
pub use service::{BoxError, ComputeService};
