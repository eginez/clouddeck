pub use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthStep, Call, CellFailure, Connection, ConnectionId, CoreField,
    DimValue, Dimension, DimensionId, ErrorKind, FieldDef, FieldKind, FieldSource, FlowId, Host,
    InspectorRowDef, InspectorSectionDef, InspectorValue, KindId, KindInfo, ListResult, LogLevel,
    MatchMode, Provider, ProviderError, ProviderInfo, ResourceId, Selection, ValueId,
};

pub mod aws_fields;
pub use aws_fields::aws_fields;
pub mod aws_inspector;
pub mod aws_login;
pub use aws_login::{LoginGuidance, login_guidance};
pub mod console_url;
pub use console_url::{
    ConsoleUrlError, Partition, console_url_for, console_url_for_instance, partition_for_region,
};
pub mod context;
pub use context::{
    AwsConfigFile, AwsConfigParseError, AwsCredentialKind, AwsFiles, AwsProfile, AwsProfileAuth,
    AwsSsoConfiguration, DEFAULT_AWS_CONFIG_FILE, FLEET_REGIONS, REGIONS, WorkspaceContext,
    aws_files_for, context_path, custom_aws_config_file, load_profile_auth_for, load_profiles_for,
    parse_profile_auth, parse_profiles, read_profile_auth,
};
pub mod demo_fleet;
pub use demo_fleet::{DemoFleet, demo_profiles, demo_region_names};
pub mod demo_scenarios;
pub use demo_scenarios::{list_scenario, requested_regions};
pub mod error;
pub use error::CloudDeckError;
pub mod fault_injecting;
pub use fault_injecting::{
    DefaultSleep, FaultInjectingComputeService, FaultInjectionBehavior, FaultInjectionOutcome,
    Sleep,
};
#[cfg(any(test, feature = "test-support"))]
pub mod fixtures;
pub mod fleet_compute;
pub use fleet_compute::{FleetComputeService, fleet_region_timeout};
pub mod instance;
pub use instance::{
    Address, AddressKind, ExtrasSection, Instance, Label, State, StateKind, native_id,
};
pub mod mock;
pub use mock::MockComputeService;
pub mod models;
pub use models::{
    ComputeInstanceFilters, ComputeInstanceLoadResult, ComputeServicePartialFailure, EC2Instance,
    EC2InstanceTag, EC2SecurityGroup,
};
pub mod provider;
pub use provider::ComputeServiceProvider;
pub mod scenario;
pub use scenario::{
    DirectoryScenarioCatalog, EC2Scenario, EC2ScenarioCatalog, EC2ScenarioProfile,
    EC2ScenarioRegion, parse_scenario,
};
pub mod scoped_provider;
pub use scoped_provider::ScopeSettings;
pub mod service;
pub use service::{BoxError, ComputeService};
pub mod service_provider;
pub use service_provider::{
    LoadHost, REGION, cloud_deck_error, join_failure, service_provider, whole_load_failure,
};
