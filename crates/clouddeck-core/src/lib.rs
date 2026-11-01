mod error;
mod fault_injecting;
mod fleet_compute;
mod instance;
pub use instance::{Address, AddressKind, ExtrasSection, Instance, Label};
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
pub use instance_list_view_model::InstanceListViewModel;
mod aws_cli;
mod aws_identity;
mod aws_sdk;
mod aws_sso_login;
mod console_url;
mod context;
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
pub use aws_cli::{
    AwsCliComputeService, CliVersionError, DEFAULT_REQUEST_TIMEOUT, resolve_aws_cli_path,
    test_aws_cli,
};
pub use aws_identity::{AwsIdentity, AwsIdentityProvider, AwsIdentityService};
pub use aws_sdk::{AwsSdkComputeService, Ec2Page, Ec2PageSource};
pub use aws_sso_login::{
    AwsSsoLoginService, LoginGuidance, PendingSsoSignIn, SsoDeviceAuthorization, login_guidance,
};
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
    AwsSsoConfiguration, FLEET_REGIONS, REGIONS, WorkspaceContext, context_path, load_profile_auth,
    load_profiles, parse_profile_auth, parse_profiles, read_profile_auth,
};
pub use layout::*;
pub use stale::*;
pub mod release;
