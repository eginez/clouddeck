mod error;
mod fault_injecting;
mod fleet_compute;
mod mock;
mod models;
mod scenario;
mod scenario_service;
mod query;
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
pub use query::*;
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
mod commands;
pub use commands::*;
mod table;
pub use table::*;
mod region_scope;
pub use region_scope::*;
pub use aws_cli::{AwsCliComputeService, CliVersionError, DEFAULT_REQUEST_TIMEOUT, test_aws_cli};
pub use aws_identity::{AwsIdentity, AwsIdentityProvider, AwsIdentityService};
pub use aws_sdk::{AwsSdkComputeService, Ec2Page, Ec2PageSource};
pub use aws_sso_login::{
    AwsSsoLoginService, LoginGuidance, PendingSsoSignIn, SsoDeviceAuthorization, login_guidance,
};
pub use console_url::{
    ConsoleUrlError, Partition, console_url_for, console_url_for_instance, partition_for_region,
};
pub use context::{
    AwsCredentialKind, AwsProfile, AwsProfileAuth, AwsSsoConfiguration, FLEET_REGIONS, REGIONS,
    WorkspaceContext, context_path, load_profile_auth, load_profiles, parse_profile_auth,
    parse_profiles,
};
