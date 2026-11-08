pub use clouddeck_provider_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStep, Call, CellFailure, Connection, ConnectionId,
    CoreField, DimValue, Dimension, DimensionId, ErrorKind, FieldDef, FieldKind, FieldSource,
    FlowId, Host, InspectorRowDef, InspectorSectionDef, InspectorValue, KindId, KindInfo,
    ListResult, LogLevel, MatchMode, Provider, ProviderError, ProviderInfo, ResourceId, Selection,
    SettingDef, SettingKind, SettingValue, ValueId,
};
pub use clouddeck_provider_kit::ComputeServiceProvider;
pub use clouddeck_provider_kit::service_provider;
pub use clouddeck_provider_kit::{
    Address, AddressKind, ExtrasSection, Instance, Label, State, StateKind, native_id,
};
pub(crate) use clouddeck_provider_kit::{
    aws_inspector, context, demo_scenarios, fault_injecting, fleet_compute, instance, mock, models,
    scenario, service,
};
mod query;
mod scenario_service;

pub use clouddeck_provider_kit::aws_fields::aws_fields;
pub use clouddeck_provider_kit::{
    AwsConfigFile, AwsConfigParseError, AwsCredentialKind, AwsProfile, AwsProfileAuth,
    AwsSsoConfiguration, BoxError, CloudDeckError, ComputeInstanceFilters,
    ComputeInstanceLoadResult, ComputeService, ComputeServicePartialFailure, ConsoleUrlError,
    DEFAULT_AWS_CONFIG_FILE, DefaultSleep, DirectoryScenarioCatalog, EC2Instance, EC2InstanceTag,
    EC2Scenario, EC2ScenarioCatalog, EC2ScenarioProfile, EC2ScenarioRegion, EC2SecurityGroup,
    FLEET_REGIONS, FaultInjectingComputeService, FaultInjectionBehavior, FaultInjectionOutcome,
    FleetComputeService, LoadHost, LoginGuidance, MockComputeService, Partition, REGION, REGIONS,
    ScopeSettings, Sleep, WorkspaceContext, aws_file_paths, aws_file_paths_for, cloud_deck_error,
    console_url_for, console_url_for_instance, context_path, custom_aws_config_file, join_failure,
    list_scenario, load_profile_auth_for, load_profiles_for, login_guidance, parse_profile_auth,
    parse_profiles, parse_scenario, partition_for_region, read_profile_auth, whole_load_failure,
};
pub use query::*;
pub use scenario_service::{
    DEFAULT_FAILURE_MESSAGE, ScenarioFailureError, ScenarioService, TestScenarioKind,
};

mod instance_detail_view_model;
mod instance_list_view_model;
pub use instance_detail_view_model::InstanceDetailViewModel;
pub use instance_list_view_model::{InstanceListViewModel, LoadRequest};
mod merged_list_view_model;
pub use merged_list_view_model::{MergedListViewModel, ProviderFailure, ProviderRow};
mod list_view;
pub use list_view::{ListView, all_providers_view};
mod workspace_provider;
pub use workspace_provider::{
    DemoFleet, demo_profiles, demo_region_names, workspace_provider, workspace_provider_for,
};
pub mod fixture_fleet;
#[cfg(feature = "test-support")]
pub use extension_host::load_extension;
pub use extension_host::{LoadError, LoadedExtension};
mod bound_provider;
pub use bound_provider::{BoundProvider, bound_provider};
mod demo_provider;
#[cfg(test)]
mod demo_provider_tests;
#[cfg(test)]
mod workspace_provider_tests;
pub use demo_provider::{DemoProvider, builtin_providers, demo_provider};
pub mod extension_host;
pub mod extension_manager;
mod inspector;
pub use inspector::*;
mod status_text;
pub use status_text::*;
mod context_model;
pub use context_model::*;
mod provider_sign_in;
pub use provider_sign_in::*;
mod failure_advice;
pub use failure_advice::*;
mod commands;
pub use commands::*;
mod table;
pub use table::*;
mod display_rows;
pub use display_rows::*;
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
pub use layout::*;
pub use stale::*;
pub mod about;
pub mod release;
