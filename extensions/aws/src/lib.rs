//! AWS provider: EC2 listing over the AWS SDK, IAM Identity Center sign-in and
//! caller identity checks. Statically linked into the app for now.

mod aws_auth;
mod aws_identity;
mod aws_sdk;
mod aws_sso_login;
mod routing;

use std::sync::Arc;

use clouddeck_provider_api::Provider;

pub use aws_auth::aws_provider;
pub use aws_identity::{AwsIdentity, AwsIdentityProvider, AwsIdentityService};
pub use aws_sdk::{AwsSdkComputeService, Ec2Page, Ec2PageSource};
pub use aws_sso_login::{LoginGuidance, login_guidance};
pub use routing::verify_instance_access;

pub fn builtin_providers(demo: bool) -> Vec<Arc<dyn Provider>> {
    let mut providers = vec![aws_provider()];
    if demo {
        providers.push(Arc::new(clouddeck_core::demo_provider()));
    }
    providers
}

#[cfg(test)]
mod tests {
    use super::builtin_providers;

    #[test]
    fn builtin_providers_register_demo_only_when_requested() {
        let ids = |demo| {
            builtin_providers(demo)
                .iter()
                .map(|provider| provider.info().id)
                .collect::<Vec<String>>()
        };
        assert_eq!(ids(false), ["aws"]);
        assert_eq!(ids(true), ["aws", "demo"]);
    }
}
