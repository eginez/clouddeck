mod error;
mod fault_injecting;
mod fleet_compute;
mod instance;
pub use instance::{
    Address, AddressKind, ExtrasSection, Instance, Label, State, StateKind, native_id,
};
mod aws_inspector;
mod provider;
pub use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthStep, Call, CellFailure, Connection, ConnectionId, CoreField,
    DimValue, Dimension, DimensionId, ErrorKind, FieldDef, FieldKind, FieldSource, FlowId, Host,
    InspectorRowDef, InspectorSectionDef, InspectorValue, KindId, KindInfo, ListResult, LogLevel,
    MatchMode, Provider, ProviderError, ProviderInfo, ResourceId, Selection, ValueId,
};
pub use provider::ComputeServiceProvider;
mod mock;
mod models;
mod query;
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
pub use query::*;
pub use scenario::{
    DirectoryScenarioCatalog, EC2Scenario, EC2ScenarioCatalog, EC2ScenarioProfile,
    EC2ScenarioRegion, parse_scenario,
};
pub use scenario_service::{
    DEFAULT_FAILURE_MESSAGE, ScenarioFailureError, ScenarioService, TestScenarioKind,
};
pub use service::{BoxError, ComputeService};

mod instance_detail_view_model;
mod instance_list_view_model;
pub use instance_detail_view_model::InstanceDetailViewModel;
pub use instance_list_view_model::{InstanceListViewModel, LoadRequest};
mod service_provider;
pub use service_provider::{
    LoadHost, REGION, cloud_deck_error, join_failure, service_provider, whole_load_failure,
};
mod workspace_provider;
pub use workspace_provider::{DemoFleet, demo_profiles, demo_region_names, workspace_provider};
mod aws_fields;
pub mod fixture_fleet;
pub use aws_fields::aws_fields;
#[cfg(feature = "test-support")]
pub use extension_host::load_extension;
pub use extension_host::{LoadError, LoadedExtension};
mod bound_provider;
mod scoped_provider;
pub use bound_provider::{BoundProvider, bound_provider};
pub use scoped_provider::ScopeSettings;
mod demo_provider;
#[cfg(test)]
mod demo_provider_tests;
mod demo_scenarios;
pub use demo_scenarios::list_scenario;
#[cfg(test)]
mod workspace_provider_tests;
pub use demo_provider::{DemoProvider, builtin_providers, demo_provider};
mod aws_login;
mod console_url;
pub use aws_login::{LoginGuidance, login_guidance};
mod context;
pub mod extension_host;
pub mod extension_manager;
mod inspector;
pub use inspector::*;
mod status_text;
pub use status_text::*;
mod failure_advice;
pub use failure_advice::*;
mod commands;
pub use commands::*;
mod table;
pub use table::*;
mod region_scope;
pub use region_scope::*;
mod profile_picker;
pub use profile_picker::*;
mod recents;
pub use recents::*;
mod ui_state;
pub use ui_state::*;
mod settings;
pub use settings::*;
mod center_state;
pub use center_state::*;
mod refresh;
pub use refresh::*;
mod layout;
mod selection;
pub use selection::*;
mod stale;
pub use console_url::{
    ConsoleUrlError, Partition, console_url_for, console_url_for_instance, partition_for_region,
};
pub use context::{
    AwsConfigFile, AwsConfigParseError, AwsCredentialKind, AwsProfile, AwsProfileAuth,
    AwsSsoConfiguration, FLEET_REGIONS, REGIONS, WorkspaceContext, aws_file_paths, context_path,
    load_profile_auth, load_profiles, parse_profile_auth, parse_profiles, read_profile_auth,
};
pub use layout::*;
pub use stale::*;
pub mod release;
