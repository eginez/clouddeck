//! Browser-based IAM Identity Center (SSO) sign-in.
//!
//! CloudDeck never invokes the AWS CLI. This module implements the OAuth 2.0
//! device-authorization grant against IAM Identity Center OIDC
//! (`aws-sdk-ssooidc`) -- the same mechanism `aws sso login` uses -- so a
//! profile backed by `sso_start_url`/`sso_session` can be signed into without
//! an installed CLI.
//!
//! On success the resulting token is written to the standard
//! `~/.aws/sso/cache/<sha1(identifier)>.json` cache file that the AWS SDK's
//! own credential provider chain already reads (the `sso` feature already
//! enabled on `aws-config`, and already exercised by `AwsSdkComputeService`
//! and `AwsIdentityService`). Because the cache also carries the client id,
//! client secret and refresh token, the SDK's built-in `SsoTokenProvider`
//! transparently refreshes the token again later -- renewal does not need to
//! be reimplemented here, only the initial browser sign-in.

use crate::{AwsCredentialKind, AwsSsoConfiguration, BoxError, CloudDeckError};
use aws_config::BehaviorVersion;
use aws_types::region::Region;
use sha1::{Digest, Sha1};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};

const CLIENT_NAME: &str = "clouddeck-rs";
const CLIENT_TYPE: &str = "public";
const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// Hard ceiling on how long a single sign-in attempt keeps polling, even if
/// the service reports a longer device-code lifetime, so a forgotten browser
/// tab cannot pin a background task open indefinitely.
const MAX_POLL_DURATION: Duration = Duration::from_secs(15 * 60);

/// Non-secret fields the UI needs to prompt the user to open a browser and
/// enter (or confirm) a code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsoDeviceAuthorization {
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub user_code: String,
    pub expires_in: Duration,
}

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

/// The identifier the AWS SDK hashes to find a profile's cached SSO token:
/// the `sso_session` name for session-based profiles, otherwise the profile's
/// own `sso_start_url`.
fn sso_cache_identifier(config: &AwsSsoConfiguration) -> &str {
    config
        .session_name
        .as_deref()
        .unwrap_or(config.start_url.as_str())
}

struct RegisteredClient {
    client_id: String,
    client_secret: String,
    client_secret_expires_at: SystemTime,
}

struct DeviceAuthorizationDetails {
    device_code: String,
    interval: Duration,
    expires_in: Duration,
}

struct IssuedToken {
    access_token: String,
    expires_in: Duration,
    refresh_token: Option<String>,
}

impl std::fmt::Debug for IssuedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedToken")
            .field("access_token", &"** redacted **")
            .field("expires_in", &self.expires_in)
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "** redacted **"),
            )
            .finish()
    }
}

enum TokenPoll {
    Ready(IssuedToken),
    Pending,
    SlowDown,
}

#[async_trait::async_trait]
trait SsoOidcProvider: Send + Sync {
    async fn register_client(&self) -> Result<RegisteredClient, BoxError>;
    async fn start_device_authorization(
        &self,
        client: &RegisteredClient,
        start_url: &str,
    ) -> Result<(SsoDeviceAuthorization, DeviceAuthorizationDetails), BoxError>;
    async fn create_token(
        &self,
        client: &RegisteredClient,
        device_code: &str,
    ) -> Result<TokenPoll, BoxError>;
}

/// Starts browser SSO sign-ins. Stateless: each `begin` call registers a
/// fresh public client against the profile's SSO region and starts a new
/// device-authorization request.
pub struct AwsSsoLoginService {
    provider: Option<Arc<dyn SsoOidcProvider>>,
}

impl Default for AwsSsoLoginService {
    fn default() -> Self {
        Self::new()
    }
}

impl AwsSsoLoginService {
    pub fn new() -> Self {
        Self { provider: None }
    }

    #[cfg(test)]
    fn with_provider(provider: Arc<dyn SsoOidcProvider>) -> Self {
        Self {
            provider: Some(provider),
        }
    }

    fn provider_for(&self, region: &str) -> Arc<dyn SsoOidcProvider> {
        match &self.provider {
            Some(provider) => Arc::clone(provider),
            None => Arc::new(SdkSsoOidcProvider {
                region: region.to_owned(),
            }),
        }
    }

    /// Begins a browser sign-in for `credential_kind`. Returns the code/URL
    /// to show the user and a handle to await completion, or an actionable
    /// error if this profile's credential mechanism does not support (or
    /// does not need) a browser sign-in.
    pub async fn begin(
        &self,
        credential_kind: &AwsCredentialKind,
    ) -> Result<(SsoDeviceAuthorization, PendingSsoSignIn), BoxError> {
        let sso = match login_guidance(credential_kind) {
            LoginGuidance::BrowserSignIn(config) => config,
            LoginGuidance::AlreadyConfigured => {
                return Err(unsupported(
                    "This profile already has usable credentials; browser sign-in is not needed.",
                ));
            }
            LoginGuidance::Unsupported { reason } => return Err(unsupported(&reason)),
        };
        let provider = self.provider_for(&sso.region);
        let client = provider.register_client().await?;
        let (authorization, details) = provider
            .start_device_authorization(&client, &sso.start_url)
            .await?;
        let identifier = sso_cache_identifier(&sso).to_owned();
        Ok((
            authorization,
            PendingSsoSignIn {
                provider,
                client,
                details,
                identifier,
                start_url: sso.start_url.clone(),
                region: sso.region.clone(),
            },
        ))
    }
}

fn unsupported(message: &str) -> BoxError {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.to_string()).into()
}

/// A device-authorization request that has been started and is waiting on
/// the user to approve it in a browser.
pub struct PendingSsoSignIn {
    provider: Arc<dyn SsoOidcProvider>,
    client: RegisteredClient,
    details: DeviceAuthorizationDetails,
    identifier: String,
    start_url: String,
    region: String,
}

impl std::fmt::Debug for PendingSsoSignIn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingSsoSignIn")
            .field("identifier", &self.identifier)
            .field("start_url", &self.start_url)
            .field("region", &self.region)
            .field("client_id", &self.client.client_id)
            .field("client_secret", &"** redacted **")
            .field("device_code", &"** redacted **")
            .finish()
    }
}

impl PendingSsoSignIn {
    /// Polls IAM Identity Center until the user completes the browser step,
    /// the device code expires, or the poll ceiling is hit, then caches the
    /// resulting token where the AWS SDK's credential chain expects it.
    pub async fn wait_for_completion(&self) -> Result<(), BoxError> {
        let token = self.poll_until_ready().await?;
        cache_sso_token(
            &self.identifier,
            &self.start_url,
            &self.region,
            &self.client,
            &token,
        )?;
        Ok(())
    }

    async fn poll_until_ready(&self) -> Result<IssuedToken, BoxError> {
        let budget = self.details.expires_in.min(MAX_POLL_DURATION);
        let deadline = tokio::time::Instant::now() + budget;
        let mut interval = self.details.interval.max(Duration::from_secs(1));
        loop {
            match self
                .provider
                .create_token(&self.client, &self.details.device_code)
                .await?
            {
                TokenPoll::Ready(token) => return Ok(token),
                TokenPoll::Pending => {}
                TokenPoll::SlowDown => interval += Duration::from_secs(5),
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(std::io::Error::other(
                    "Browser sign-in was not approved in time. Choose Sign in again to get a new code.",
                )
                .into());
            }
            tokio::time::sleep(interval).await;
        }
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_default()
}

fn sso_cache_path(identifier: &str, home: &Path) -> PathBuf {
    home.join(".aws/sso/cache").join(format!(
        "{}.json",
        hex::encode(Sha1::digest(identifier.as_bytes()))
    ))
}

fn format_rfc3339(time: SystemTime) -> String {
    let datetime: chrono::DateTime<chrono::Utc> = time.into();
    datetime.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn cache_sso_token(
    identifier: &str,
    start_url: &str,
    region: &str,
    client: &RegisteredClient,
    token: &IssuedToken,
) -> std::io::Result<PathBuf> {
    let path = sso_cache_path(identifier, &home_dir());
    write_cache_token_at(&path, start_url, region, client, token)?;
    Ok(path)
}

/// Writes the cached-token JSON at an explicit path in the same shape the
/// AWS SDK writes and reads (see `aws-config`'s `sso::token` module): a
/// required `accessToken`/`expiresAt` pair, plus the refresh material needed
/// for the SDK to renew the token on its own later.
fn write_cache_token_at(
    path: &Path,
    start_url: &str,
    region: &str,
    client: &RegisteredClient,
    token: &IssuedToken,
) -> std::io::Result<()> {
    let expires_at = SystemTime::now() + token.expires_in;
    let mut object = serde_json::Map::new();
    object.insert("accessToken".into(), token.access_token.clone().into());
    object.insert("expiresAt".into(), format_rfc3339(expires_at).into());
    if let Some(refresh_token) = &token.refresh_token {
        object.insert("refreshToken".into(), refresh_token.clone().into());
    }
    object.insert("clientId".into(), client.client_id.clone().into());
    object.insert("clientSecret".into(), client.client_secret.clone().into());
    object.insert(
        "registrationExpiresAt".into(),
        format_rfc3339(client.client_secret_expires_at).into(),
    );
    object.insert("region".into(), region.into());
    object.insert("startUrl".into(), start_url.into());

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload =
        serde_json::to_vec(&serde_json::Value::Object(object)).map_err(std::io::Error::other)?;

    static NEXT_TEMP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("token.json");
    let temp = path.with_file_name(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        NEXT_TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(&payload)?;
        file.sync_all()?;
    }
    std::fs::rename(temp, path)
}

struct SdkSsoOidcProvider {
    region: String,
}

impl SdkSsoOidcProvider {
    async fn client(&self) -> aws_sdk_ssooidc::Client {
        // Device-authorization operations are unauthenticated; the OIDC
        // client id/secret carry the request, not SigV4 credentials. Using
        // `no_credentials` keeps sign-in usable even when no profile exists
        // yet, matching the CLI-free onboarding goal.
        let config = aws_config::defaults(BehaviorVersion::latest())
            .region(Region::new(self.region.clone()))
            .no_credentials()
            .load()
            .await;
        aws_sdk_ssooidc::Client::new(&config)
    }
}

#[async_trait::async_trait]
impl SsoOidcProvider for SdkSsoOidcProvider {
    async fn register_client(&self) -> Result<RegisteredClient, BoxError> {
        let client = self.client().await;
        let output = client
            .register_client()
            .client_name(CLIENT_NAME)
            .client_type(CLIENT_TYPE)
            .send()
            .await
            .map_err(|error| {
                let code = error.as_service_error().and_then(|e| e.meta().code());
                oidc_error(
                    "register this application with AWS SSO",
                    code,
                    &error.to_string(),
                )
            })?;
        let client_id = output
            .client_id()
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| missing_field("clientId"))?
            .to_owned();
        let client_secret = output
            .client_secret()
            .filter(|secret| !secret.trim().is_empty())
            .ok_or_else(|| missing_field("clientSecret"))?
            .to_owned();
        let client_secret_expires_at = SystemTime::UNIX_EPOCH
            + Duration::from_secs(output.client_secret_expires_at().max(0) as u64);
        Ok(RegisteredClient {
            client_id,
            client_secret,
            client_secret_expires_at,
        })
    }

    async fn start_device_authorization(
        &self,
        registered: &RegisteredClient,
        start_url: &str,
    ) -> Result<(SsoDeviceAuthorization, DeviceAuthorizationDetails), BoxError> {
        let client = self.client().await;
        let output = client
            .start_device_authorization()
            .client_id(&registered.client_id)
            .client_secret(&registered.client_secret)
            .start_url(start_url)
            .send()
            .await
            .map_err(|error| {
                let code = error.as_service_error().and_then(|e| e.meta().code());
                oidc_error(
                    "start the AWS SSO browser sign-in",
                    code,
                    &error.to_string(),
                )
            })?;
        let device_code = output
            .device_code()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| missing_field("deviceCode"))?
            .to_owned();
        let user_code = output
            .user_code()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| missing_field("userCode"))?
            .to_owned();
        let verification_uri = output
            .verification_uri()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| missing_field("verificationUri"))?
            .to_owned();
        let verification_uri_complete = output
            .verification_uri_complete()
            .map(str::to_owned)
            .filter(|value| !value.trim().is_empty());
        let expires_in = Duration::from_secs(output.expires_in().max(0) as u64);
        let interval = Duration::from_secs(output.interval().max(1) as u64);
        Ok((
            SsoDeviceAuthorization {
                verification_uri,
                verification_uri_complete,
                user_code,
                expires_in,
            },
            DeviceAuthorizationDetails {
                device_code,
                interval,
                expires_in,
            },
        ))
    }

    async fn create_token(
        &self,
        registered: &RegisteredClient,
        device_code: &str,
    ) -> Result<TokenPoll, BoxError> {
        use aws_sdk_ssooidc::operation::create_token::CreateTokenError;

        let client = self.client().await;
        let result = client
            .create_token()
            .client_id(&registered.client_id)
            .client_secret(&registered.client_secret)
            .grant_type(DEVICE_GRANT_TYPE)
            .device_code(device_code)
            .send()
            .await;
        let output = match result {
            Ok(output) => output,
            Err(error) => {
                return match error.as_service_error() {
                    Some(CreateTokenError::AuthorizationPendingException(_)) => {
                        Ok(TokenPoll::Pending)
                    }
                    Some(CreateTokenError::SlowDownException(_)) => Ok(TokenPoll::SlowDown),
                    Some(CreateTokenError::ExpiredTokenException(_)) => {
                        Err(std::io::Error::other(
                            "The browser sign-in code expired before it was approved. Choose Sign in again to get a new code.",
                        )
                        .into())
                    }
                    Some(CreateTokenError::AccessDeniedException(_)) => Err(std::io::Error::other(
                        "Sign-in was denied in the browser. Choose Sign in again to retry.",
                    )
                    .into()),
                    other => {
                        let code = other.and_then(|e| e.meta().code());
                        Err(oidc_error(
                            "complete the AWS SSO browser sign-in",
                            code,
                            &error.to_string(),
                        ))
                    }
                };
            }
        };
        let access_token = output
            .access_token()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| missing_field("accessToken"))?
            .to_owned();
        let expires_in = Duration::from_secs(output.expires_in().max(0) as u64);
        let refresh_token = output.refresh_token().map(str::to_owned);
        Ok(TokenPoll::Ready(IssuedToken {
            access_token,
            expires_in,
            refresh_token,
        }))
    }
}

fn missing_field(field: &str) -> BoxError {
    std::io::Error::other(format!(
        "AWS SSO response did not include the expected `{field}` field."
    ))
    .into()
}

fn oidc_error(operation: &str, code: Option<&str>, detail: &str) -> BoxError {
    let classification = CloudDeckError::from_aws_error_code(code, detail);
    let message = match classification {
        CloudDeckError::Permission => format!(
            "AWS denied the request to {operation}. Verify the IAM Identity Center start URL and that your account has access."
        ),
        CloudDeckError::Auth => format!(
            "Could not {operation}: the SSO client registration or session is invalid or expired. Choose Sign in again to retry."
        ),
        CloudDeckError::Throttled => {
            format!("AWS throttled the request to {operation}. Retry after a short delay.")
        }
        CloudDeckError::Timeout => {
            format!("Timed out trying to {operation}. Retry the connection.")
        }
        CloudDeckError::Network => {
            format!("Could not reach AWS to {operation}. Check connectivity.")
        }
        CloudDeckError::NotFound => {
            format!("AWS could not find the requested resource while trying to {operation}.")
        }
        CloudDeckError::Unknown(_) => format!(
            "Could not {operation}{}.",
            code.map_or(String::new(), |code| format!(" ({code})"))
        ),
    };
    std::io::Error::other(message).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

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

    #[test]
    fn sso_cache_identifier_prefers_session_name_over_start_url() {
        assert_eq!(sso_cache_identifier(&sso_config(Some("corp"))), "corp");
        assert_eq!(
            sso_cache_identifier(&sso_config(None)),
            "https://example.awsapps.com/start"
        );
    }

    #[test]
    fn cache_path_matches_the_aws_sdks_own_known_hash() {
        // Cross-checked against aws-config's own `determine_correct_cache_filenames`
        // test (aws-config-1.11.0 src/sso/token.rs) so a token this module
        // writes is actually found by the SDK's credential provider chain.
        assert_eq!(
            sso_cache_path(
                "https://d-0000000000.awsapps.com/start",
                Path::new("/home/me")
            ),
            PathBuf::from("/home/me/.aws/sso/cache/b51e627ec5d75dab0c56e37911d5b31610f612e0.json")
        );
        assert_eq!(
            sso_cache_path("admin", Path::new("/home/someuser")),
            PathBuf::from(
                "/home/someuser/.aws/sso/cache/d033e22ae348aeb5660fc2140aec35850c4da997.json"
            )
        );
    }

    #[test]
    fn cached_token_file_round_trips_expected_json_fields_without_missing_the_refresh_material() {
        let dir = std::env::temp_dir().join(format!(
            "clouddeck-sso-cache-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("token.json");
        let client = RegisteredClient {
            client_id: "client-id".into(),
            client_secret: "client-secret".into(),
            client_secret_expires_at: SystemTime::UNIX_EPOCH + Duration::from_secs(100_000_000),
        };
        let token = IssuedToken {
            access_token: "access-token".into(),
            expires_in: Duration::from_secs(3600),
            refresh_token: Some("refresh-token".into()),
        };
        write_cache_token_at(
            &path,
            "https://example.awsapps.com/start",
            "us-east-1",
            &client,
            &token,
        )
        .unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(value["accessToken"], "access-token");
        assert_eq!(value["refreshToken"], "refresh-token");
        assert_eq!(value["clientId"], "client-id");
        assert_eq!(value["clientSecret"], "client-secret");
        assert_eq!(value["region"], "us-east-1");
        assert_eq!(value["startUrl"], "https://example.awsapps.com/start");
        assert!(value["expiresAt"].as_str().unwrap().ends_with('Z'));
        assert!(
            value["registrationExpiresAt"]
                .as_str()
                .unwrap()
                .ends_with('Z')
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o600,
                "cached SSO token must not be group/world readable"
            );
        }

        let _ = std::fs::remove_dir_all(&dir);
    }

    struct FakeProvider {
        register_calls: Mutex<u32>,
        create_token_outcomes: Mutex<Vec<Result<TokenPoll, String>>>,
    }

    #[async_trait::async_trait]
    impl SsoOidcProvider for FakeProvider {
        async fn register_client(&self) -> Result<RegisteredClient, BoxError> {
            *self.register_calls.lock().unwrap() += 1;
            Ok(RegisteredClient {
                client_id: "fake-client-id".into(),
                client_secret: "fake-client-secret".into(),
                client_secret_expires_at: SystemTime::now() + Duration::from_secs(86_400),
            })
        }

        async fn start_device_authorization(
            &self,
            _client: &RegisteredClient,
            _start_url: &str,
        ) -> Result<(SsoDeviceAuthorization, DeviceAuthorizationDetails), BoxError> {
            Ok((
                SsoDeviceAuthorization {
                    verification_uri: "https://device.sso.us-east-1.amazonaws.com".into(),
                    verification_uri_complete: Some(
                        "https://device.sso.us-east-1.amazonaws.com?user_code=ABCD-EFGH".into(),
                    ),
                    user_code: "ABCD-EFGH".into(),
                    expires_in: Duration::from_secs(600),
                },
                DeviceAuthorizationDetails {
                    device_code: "fake-device-code".into(),
                    interval: Duration::from_millis(1),
                    expires_in: Duration::from_secs(600),
                },
            ))
        }

        async fn create_token(
            &self,
            _client: &RegisteredClient,
            _device_code: &str,
        ) -> Result<TokenPoll, BoxError> {
            let mut outcomes = self.create_token_outcomes.lock().unwrap();
            if outcomes.is_empty() {
                return Err(std::io::Error::other("no more outcomes queued").into());
            }
            outcomes
                .remove(0)
                .map_err(|message| std::io::Error::other(message).into())
        }
    }

    #[tokio::test]
    async fn sign_in_polls_through_pending_and_slow_down_before_succeeding() {
        let provider = Arc::new(FakeProvider {
            register_calls: Mutex::new(0),
            create_token_outcomes: Mutex::new(vec![
                Ok(TokenPoll::Pending),
                Ok(TokenPoll::SlowDown),
                Ok(TokenPoll::Ready(IssuedToken {
                    access_token: "access-token".into(),
                    expires_in: Duration::from_secs(3600),
                    refresh_token: Some("refresh-token".into()),
                })),
            ]),
        });
        let service = AwsSsoLoginService::with_provider(provider.clone());
        let (authorization, pending) = service
            .begin(&AwsCredentialKind::Sso(sso_config(Some("corp"))))
            .await
            .unwrap();
        assert_eq!(authorization.user_code, "ABCD-EFGH");
        assert_eq!(*provider.register_calls.lock().unwrap(), 1);

        let token = pending.poll_until_ready().await.unwrap();
        assert_eq!(token.access_token, "access-token");
        assert_eq!(token.refresh_token.as_deref(), Some("refresh-token"));
    }

    #[tokio::test]
    async fn sign_in_reports_an_actionable_error_when_the_user_denies_access() {
        let provider = Arc::new(FakeProvider {
            register_calls: Mutex::new(0),
            create_token_outcomes: Mutex::new(vec![Err("access denied by user".into())]),
        });
        let service = AwsSsoLoginService::with_provider(provider);
        let (_, pending) = service
            .begin(&AwsCredentialKind::Sso(sso_config(None)))
            .await
            .unwrap();
        let error = pending.poll_until_ready().await.unwrap_err();
        assert!(!error.to_string().is_empty());
    }

    #[tokio::test]
    async fn sign_in_expires_if_the_device_code_lifetime_elapses_without_approval() {
        struct NeverReadyProvider;
        #[async_trait::async_trait]
        impl SsoOidcProvider for NeverReadyProvider {
            async fn register_client(&self) -> Result<RegisteredClient, BoxError> {
                Ok(RegisteredClient {
                    client_id: "id".into(),
                    client_secret: "secret".into(),
                    client_secret_expires_at: SystemTime::now() + Duration::from_secs(86_400),
                })
            }
            async fn start_device_authorization(
                &self,
                _client: &RegisteredClient,
                _start_url: &str,
            ) -> Result<(SsoDeviceAuthorization, DeviceAuthorizationDetails), BoxError>
            {
                Ok((
                    SsoDeviceAuthorization {
                        verification_uri: "https://device.example".into(),
                        verification_uri_complete: None,
                        user_code: "CODE".into(),
                        expires_in: Duration::from_millis(5),
                    },
                    DeviceAuthorizationDetails {
                        device_code: "device-code".into(),
                        interval: Duration::from_millis(1),
                        expires_in: Duration::from_millis(5),
                    },
                ))
            }
            async fn create_token(
                &self,
                _client: &RegisteredClient,
                _device_code: &str,
            ) -> Result<TokenPoll, BoxError> {
                Ok(TokenPoll::Pending)
            }
        }
        let service = AwsSsoLoginService::with_provider(Arc::new(NeverReadyProvider));
        let (_, pending) = service
            .begin(&AwsCredentialKind::Sso(sso_config(None)))
            .await
            .unwrap();
        let error = pending.poll_until_ready().await.unwrap_err();
        assert!(error.to_string().contains("not approved in time"));
    }

    #[tokio::test]
    async fn begin_rejects_profiles_that_do_not_support_browser_sign_in() {
        let service = AwsSsoLoginService::new();
        let error = service
            .begin(&AwsCredentialKind::ExternalProvider)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("credential_process"));
    }
}
