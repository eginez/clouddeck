use crate::{AwsCredentialKind, AwsSsoConfiguration};

/// What CloudDeck can do, and how to explain it, for a profile's configured
/// credential mechanism. Checked before starting a browser sign-in so
/// unsupported profiles get an actionable message instead of a confusing SDK
/// error partway through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginGuidance {
    /// The profile already has usable credentials; no browser step is needed.
    AlreadyConfigured,
    /// The profile is IAM Identity Center backed; a browser sign-in can proceed.
    BrowserSignIn(AwsSsoConfiguration),
    /// CloudDeck cannot drive sign-in for this profile. `reason` explains why
    /// and what to change, without assuming a shell or the AWS CLI.
    Unsupported { reason: String },
}

pub fn login_guidance(credential_kind: &AwsCredentialKind) -> LoginGuidance {
    match credential_kind {
        AwsCredentialKind::Sso(config) if config.is_configured() => {
            LoginGuidance::BrowserSignIn(config.clone())
        }
        AwsCredentialKind::Sso(_) => LoginGuidance::Unsupported {
            reason: "This profile references IAM Identity Center but is missing a start URL or region. Fix sso_start_url/sso_region for this profile in ~/.aws/config, then retry.".into(),
        },
        AwsCredentialKind::SharedCredentials => LoginGuidance::AlreadyConfigured,
        AwsCredentialKind::ExternalProvider => LoginGuidance::Unsupported {
            reason: "This profile uses a custom credential_process. CloudDeck does not run external credential processes; choose a profile with IAM Identity Center or static credentials instead.".into(),
        },
        AwsCredentialKind::Unconfigured => LoginGuidance::Unsupported {
            reason: "This profile has no usable credentials configured. Add IAM Identity Center (SSO) settings or access keys to this profile in ~/.aws/config, then retry.".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sso_config(session_name: Option<&str>) -> AwsSsoConfiguration {
        AwsSsoConfiguration {
            start_url: "https://example.awsapps.com/start".into(),
            region: "us-east-1".into(),
            account_id: Some("123456789012".into()),
            role_name: Some("Admin".into()),
            session_name: session_name.map(str::to_owned),
        }
    }

    #[test]
    fn login_guidance_routes_sso_profiles_to_browser_sign_in() {
        let guidance = login_guidance(&AwsCredentialKind::Sso(sso_config(Some("corp"))));
        assert_eq!(
            guidance,
            LoginGuidance::BrowserSignIn(sso_config(Some("corp")))
        );
    }

    #[test]
    fn login_guidance_flags_incomplete_sso_configuration() {
        let incomplete = AwsSsoConfiguration {
            start_url: String::new(),
            ..sso_config(None)
        };
        assert!(matches!(
            login_guidance(&AwsCredentialKind::Sso(incomplete)),
            LoginGuidance::Unsupported { .. }
        ));
    }

    #[test]
    fn login_guidance_does_not_need_sign_in_for_shared_credentials() {
        assert_eq!(
            login_guidance(&AwsCredentialKind::SharedCredentials),
            LoginGuidance::AlreadyConfigured
        );
    }

    #[test]
    fn login_guidance_explains_external_provider_and_unconfigured_profiles() {
        let external = login_guidance(&AwsCredentialKind::ExternalProvider);
        let unconfigured = login_guidance(&AwsCredentialKind::Unconfigured);
        assert!(
            matches!(&external, LoginGuidance::Unsupported { reason } if reason.contains("credential_process"))
        );
        assert!(
            matches!(&unconfigured, LoginGuidance::Unsupported { reason } if reason.contains("no usable credentials"))
        );
        // Actionable guidance must never suggest a shell command CloudDeck
        // did not run itself.
        if let LoginGuidance::Unsupported { reason } = external {
            assert!(!reason.contains("aws "));
        }
    }
}
