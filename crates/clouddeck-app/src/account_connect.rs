//! Drives the AWS connection check behind `AccountWindowView`'s single
//! Connect-to-AWS screen.
//!
//! `AccountWindowView` (account_window.rs) is pure UI state: clicking
//! "Connect to AWS" / "Sign in with browser" / "Rescan" only flips one-shot
//! request flags (`connect_requested`, `sign_in_requested`,
//! `rescan_requested`, `open_browser_requested`) and moves the view into a
//! busy connection state. Until this module, nothing ever consumed those
//! flags, so the button just spun on "Connecting..." forever -- a dead end
//! the clouddeck-rs-w6s.18 acceptance criteria explicitly rule out.
//!
//! The SDK-backed backend this module calls
//! (`AwsIdentityService`/`AwsSdkComputeService`/`AwsSsoLoginService`/
//! `login_guidance`) already existed from clouddeck-rs-wz2.2; this module is
//! the missing wiring, not a new auth implementation. Kept out of
//! `account_window.rs` (pure UI state, easy to unit test without gpui) and
//! `shell.rs` (window lifecycle, not auth orchestration) so each stays
//! focused.

use gpui::{App, AsyncApp, Entity};

use crate::account_window::{AccountBrowserLoginPrompt, AccountConnectionError, AccountWindowView};
use clouddeck_core::{
    AwsCredentialKind, AwsIdentityService, AwsSdkComputeService, AwsSsoLoginService,
    ComputeService, LoginGuidance, login_guidance,
};

/// Consumes whichever one-shot request flags the wizard has set since the
/// last notify and dispatches the matching work. Called from the shell's
/// `cx.observe` on the wizard entity, so it runs after every render-worthy
/// change; flags are cleared up front (without renotifying) so a later
/// re-notify -- e.g. from the async work's own state updates -- never
/// re-dispatches the same request.
pub fn drive(wizard: &Entity<AccountWindowView>, cx: &mut App) {
    let (connect, sign_in, rescan_requested, open_browser) = wizard.update(cx, |view, _cx| {
        let flags = (
            view.connect_requested,
            view.sign_in_requested,
            view.rescan_requested,
            view.open_browser_requested,
        );
        view.clear_requests();
        flags
    });

    if open_browser && let Some(prompt) = wizard.read(cx).browser_login_prompt.clone() {
        let url = prompt
            .verification_uri_complete
            .clone()
            .unwrap_or_else(|| prompt.verification_uri.clone());
        cx.open_url(&url);
    }

    if rescan_requested {
        rescan(wizard.clone(), cx);
    }

    if sign_in {
        begin_sign_in(wizard.clone(), cx);
    } else if connect {
        begin_connect(wizard.clone(), cx);
    }
}

/// Looks up the selected profile's credential mechanism from `~/.aws`
/// (secret-free metadata only -- see `clouddeck_core::AwsProfileAuth`).
/// Falls back to `Unconfigured` if the profile can't be found, which
/// correctly routes into an actionable "no usable credentials" failure
/// instead of a panic or a silent no-op.
fn credential_kind_for(profile: &str) -> AwsCredentialKind {
    clouddeck_core::load_profile_auth()
        .ok()
        .and_then(|profiles| profiles.into_iter().find(|p| p.name == profile))
        .map(|p| p.credential_kind)
        .unwrap_or(AwsCredentialKind::Unconfigured)
}

fn rescan(wizard: Entity<AccountWindowView>, cx: &mut App) {
    let profiles = clouddeck_core::load_profiles().unwrap_or_default();
    let regions = clouddeck_core::REGIONS
        .iter()
        .map(|region| (*region).to_string())
        .collect::<Vec<_>>();
    wizard.update(cx, |view, cx| {
        view.profiles = Some(profiles);
        view.available_regions = Some(regions);
        cx.notify();
    });
}

/// Handles "Connect to AWS". Unsupported credential mechanisms (no CLI
/// dependency in this app -- see aws_sso_login.rs -- so this is the
/// equivalent recoverable state for a profile CloudDeck genuinely cannot
/// drive) fail immediately with the actionable `login_guidance` reason,
/// rather than spinning on a network call guaranteed to fail. Otherwise this
/// always attempts a direct identity check first: an already-authenticated
/// profile (static credentials, or a still-valid cached SSO token) connects
/// in one click without a browser round trip.
fn begin_connect(wizard: Entity<AccountWindowView>, cx: &mut App) {
    let profile = wizard.read(cx).selected_profile.clone().unwrap_or_default();
    let region = wizard.read(cx).selected_region.clone();
    let attempt = wizard.read(cx).connection_attempt;
    let credential_kind = credential_kind_for(&profile);

    if let LoginGuidance::Unsupported { reason } = login_guidance(&credential_kind) {
        let error = unsupported_connection_error(&credential_kind, reason);
        wizard.update(cx, |view, cx| {
            if view.fail_connection_for_attempt(attempt, error) {
                cx.notify();
            }
        });
        return;
    }

    cx.spawn(async move |cx| {
        run_identity_and_access_check(wizard, profile, region, attempt, cx).await;
    })
    .detach();
}

/// Handles "Sign in with browser" / the "Sign in again" retry action:
/// starts a device-authorization browser sign-in
/// (`AwsSsoLoginService::begin`), publishes the code/URL for the UI to
/// show, waits for the user to approve it, then falls through to the same
/// identity + EC2-access check `begin_connect` uses once a token is cached.
fn begin_sign_in(wizard: Entity<AccountWindowView>, cx: &mut App) {
    let profile = wizard.read(cx).selected_profile.clone().unwrap_or_default();
    let region = wizard.read(cx).selected_region.clone();
    let attempt = wizard.read(cx).connection_attempt;
    let credential_kind = credential_kind_for(&profile);

    cx.spawn(async move |cx| {
        let login_service = AwsSsoLoginService::new();
        let begin_result = crate::runtime::runtime()
            .spawn(async move { login_service.begin(&credential_kind).await })
            .await
            .unwrap_or_else(|error| Err(std::io::Error::other(error.to_string()).into()));

        let (authorization, pending) = match begin_result {
            Ok(pair) => pair,
            Err(error) => {
                fail(&wizard, attempt, &error.to_string(), cx);
                return;
            }
        };

        let applied = wizard.update(cx, |view, cx| {
            if view.connection_attempt != attempt || view.dismissed {
                return false;
            }
            view.await_browser_login_with_prompt(AccountBrowserLoginPrompt {
                verification_uri: authorization.verification_uri.clone(),
                verification_uri_complete: authorization.verification_uri_complete.clone(),
                user_code: authorization.user_code.clone(),
                expires_in: authorization.expires_in.as_secs(),
                // The SDK's own polling interval lives inside
                // `PendingSsoSignIn` (not exposed -- `wait_for_completion`
                // handles pacing internally); the UI never renders this
                // field, so any positive placeholder is fine here.
                interval: 5,
            });
            cx.notify();
            true
        });
        if !matches!(applied, Ok(true)) {
            return;
        }

        let wait_result = crate::runtime::runtime()
            .spawn(async move { pending.wait_for_completion().await })
            .await
            .unwrap_or_else(|error| Err(std::io::Error::other(error.to_string()).into()));

        if let Err(error) = wait_result {
            fail(&wizard, attempt, &error.to_string(), cx);
            return;
        }

        run_identity_and_access_check(wizard, profile, region, attempt, cx).await;
    })
    .detach();
}

/// Resolves STS identity, then verifies the selected region's EC2 access
/// with a real `DescribeInstances` call -- the same seam `AppShell` uses to
/// load the fleet, so "connected" here means the fleet will actually open,
/// not just that credentials parsed. Reports through the wizard's
/// attempt-guarded methods so a stale/cancelled/rescoped attempt can never
/// clobber a newer one (see `AccountWindowView::connection_attempt`).
async fn run_identity_and_access_check(
    wizard: Entity<AccountWindowView>,
    profile: String,
    region: String,
    attempt: u64,
    cx: &mut AsyncApp,
) {
    let identity_service = AwsIdentityService::new(profile.clone(), region.clone());
    let identity_result = crate::runtime::runtime()
        .spawn(async move { identity_service.resolve().await })
        .await
        .unwrap_or_else(|error| Err(std::io::Error::other(error.to_string()).into()));

    let account_id = match identity_result {
        Ok(identity) => identity.account_id,
        Err(error) => {
            fail(&wizard, attempt, &error.to_string(), cx);
            return;
        }
    };

    let applied = wizard.update(cx, |view, cx| {
        let ok = view.identity_resolved_for_attempt(attempt, account_id.clone());
        if ok {
            cx.notify();
        }
        ok
    });
    if !matches!(applied, Ok(true)) {
        return;
    }

    let service = AwsSdkComputeService::new(profile, region);
    let access_result = crate::runtime::runtime()
        .spawn(async move { service.list_instances(None).await })
        .await
        .unwrap_or_else(|error| Err(std::io::Error::other(error.to_string()).into()));

    match access_result {
        Ok(_) => {
            let _ = wizard.update(cx, |view, cx| {
                if view.complete_connection_for_attempt(attempt, account_id.clone()) {
                    cx.notify();
                }
            });
        }
        Err(error) => fail(&wizard, attempt, &error.to_string(), cx),
    }
}

/// `ExternalProvider` already has a dedicated, precise message
/// (`AccountConnectionError::UnsupportedProfile`, "uses an external
/// credential provider..."); everything else unsupported (no credentials
/// configured yet, or an incomplete SSO section -- this app's equivalent of
/// the old wizard's "install the CLI" dead end, now replaced with an
/// in-place, actionable reason) carries a case-specific reason from
/// `login_guidance` itself, so that's surfaced verbatim instead of a
/// generic message.
fn unsupported_connection_error(
    credential_kind: &AwsCredentialKind,
    reason: String,
) -> AccountConnectionError {
    match credential_kind {
        AwsCredentialKind::ExternalProvider => AccountConnectionError::UnsupportedProfile,
        _ => AccountConnectionError::Unknown(reason),
    }
}

fn fail(wizard: &Entity<AccountWindowView>, attempt: u64, message: &str, cx: &mut AsyncApp) {
    let error = classify(message);
    let _ = wizard.update(cx, |view, cx| {
        if view.fail_connection_for_attempt(attempt, error) {
            cx.notify();
        }
    });
}

/// Maps a `clouddeck_core` service error's user-facing message (already
/// classified once via `CloudDeckError` inside `aws_identity.rs`/
/// `aws_sdk.rs`/`aws_sso_login.rs`) onto the wizard's small recovery
/// vocabulary. Matches the exact wording those modules produce -- see their
/// `identity_error`/`aws_request_error`/`oidc_error` helpers -- rather than
/// re-deriving classification generically, since round-tripping through
/// `CloudDeckError::from_service_error`'s keyword list a second time
/// produces mismatches (e.g. "connectivity" vs. its "connection" keyword).
fn classify(message: &str) -> AccountConnectionError {
    let lower = message.to_lowercase();
    if lower.contains("expired")
        || lower.contains("sign in again")
        || lower.contains("not approved in time")
        || lower.contains("could not authenticate")
    {
        AccountConnectionError::ExpiredLogin
    } else if lower.contains("denied")
        || lower.contains("permission")
        || lower.contains("not authorized")
    {
        AccountConnectionError::AccessDenied
    } else if lower.contains("reach aws") || lower.contains("timed out") {
        AccountConnectionError::Network
    } else {
        AccountConnectionError::Unknown(message.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_auth_failures_as_expired_login_not_generic_unknown() {
        assert_eq!(
            classify(
                "AWS credentials are invalid or expired. Sign in to the selected profile, then retry."
            ),
            AccountConnectionError::ExpiredLogin
        );
        assert_eq!(
            classify(
                "EC2 DescribeInstances could not authenticate. Sign in to the selected AWS profile, then retry."
            ),
            AccountConnectionError::ExpiredLogin
        );
        assert_eq!(
            classify(
                "Browser sign-in was not approved in time. Choose Sign in again to get a new code."
            ),
            AccountConnectionError::ExpiredLogin
        );
        assert_eq!(
            classify("Sign-in was denied in the browser. Choose Sign in again to retry."),
            AccountConnectionError::ExpiredLogin,
            "a browser-flow denial should route through sign-in-again, not the IAM access-denied bucket"
        );
    }

    #[test]
    fn classifies_permission_failures_as_access_denied() {
        assert_eq!(
            classify("AWS identity access was denied. Verify sts:GetCallerIdentity permission."),
            AccountConnectionError::AccessDenied
        );
        assert_eq!(
            classify(
                "EC2 DescribeInstances was denied. Verify the selected profile has ec2:DescribeInstances permission."
            ),
            AccountConnectionError::AccessDenied
        );
    }

    #[test]
    fn classifies_connectivity_failures_as_network() {
        assert_eq!(
            classify("AWS identity could not reach AWS. Check connectivity."),
            AccountConnectionError::Network
        );
        assert_eq!(
            classify("AWS identity request timed out. Retry the connection."),
            AccountConnectionError::Network
        );
    }

    #[test]
    fn falls_back_to_unknown_with_the_original_message_preserved() {
        assert_eq!(
            classify("AWS identity was throttled. Retry after a short delay."),
            AccountConnectionError::Unknown(
                "AWS identity was throttled. Retry after a short delay.".into()
            )
        );
    }

    #[test]
    fn unsupported_profiles_never_suggest_installing_a_cli_or_restarting() {
        // No CLI dependency in this app (clouddeck-rs-wz2.2) -- a profile
        // CloudDeck cannot drive must still get an actionable, in-place
        // reason, never "install the CLI and come back" or "restart the
        // app".
        let external = unsupported_connection_error(
            &AwsCredentialKind::ExternalProvider,
            "should be ignored for this variant".into(),
        );
        assert_eq!(external, AccountConnectionError::UnsupportedProfile);
        assert!(!external.message().to_lowercase().contains("restart"));
        assert!(!external.message().to_lowercase().contains("aws "));

        let unconfigured_reason =
            "This profile has no usable credentials configured. Add IAM Identity Center (SSO) settings or access keys to this profile in ~/.aws/config, then retry."
                .to_string();
        let unconfigured = unsupported_connection_error(
            &AwsCredentialKind::Unconfigured,
            unconfigured_reason.clone(),
        );
        assert_eq!(
            unconfigured,
            AccountConnectionError::Unknown(unconfigured_reason)
        );
        assert!(!unconfigured.message().to_lowercase().contains("restart"));
        assert!(!unconfigured.message().to_lowercase().contains("install"));
    }
}
