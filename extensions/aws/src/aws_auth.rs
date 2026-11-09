//! AWS sign-in behind the provider auth contract. The app drives sign-in only
//! through `Provider::begin_auth` / `continue_auth`; the IAM Identity Center
//! details stay in `aws_sso_login`.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::SystemTime;

use clouddeck_provider_api::{
    Auth, AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep, Browse, Call, Config,
    Connection, ConnectionId, ErrorKind, FieldDef, FlowId, InspectorSectionDef, Instance, KindId,
    KindInfo, ListResult, Provider, ProviderError, ProviderInfo, Selection, SettingDef,
    SettingKind, SettingOption, SettingValue,
};

use clouddeck_provider_kit::{
    AwsCredentialKind, AwsFiles, AwsSsoConfiguration, BoxError, ComputeServiceProvider,
    DEFAULT_AWS_CONFIG_FILE, Dimension, LoginGuidance, REGION, REGIONS, ScopeSettings,
    aws_files_for, list_scenario, load_profiles_for, login_guidance, read_profile_auth,
};

use crate::aws_identity::AwsIdentityService;
use crate::aws_sdk::AwsSdkComputeService;
use crate::aws_sso_login::{
    AwsSsoLoginService, PendingSsoSignIn, SsoDeviceAuthorization, home_dir, sso_cache_identifier,
    sso_cache_path,
};
use crate::routing::{routed_list, verify_instance_access};

const PROFILE_METHOD_ID: &str = "profile";
pub(crate) const CONFIG_DIR_SETTING: &str = "config_dir";
pub(crate) const CONFIG_FILE_SETTING: &str = "config_file";
pub(crate) const REQUEST_TIMEOUT_SETTING: &str = "request_timeout";

struct Flow {
    authorization: SsoDeviceAuthorization,
    pending: PendingSsoSignIn,
}

struct AwsAuth {
    home: PathBuf,
    sso: AwsSsoLoginService,
    flows: Mutex<HashMap<FlowId, Arc<Flow>>>,
    next_flow: AtomicU64,
}

impl AwsAuth {
    fn new(home: PathBuf, sso: AwsSsoLoginService) -> Self {
        Self {
            home,
            sso,
            flows: Mutex::new(HashMap::new()),
            next_flow: AtomicU64::new(0),
        }
    }

    fn flows(&self) -> Result<MutexGuard<'_, HashMap<FlowId, Arc<Flow>>>, ProviderError> {
        self.flows
            .lock()
            .map_err(|_| ProviderError::new(ErrorKind::Other, "The sign-in state is unavailable."))
    }

    fn methods(&self) -> Vec<AuthMethod> {
        let mut method = AuthMethod::new(
            PROFILE_METHOD_ID.to_string(),
            "Use an AWS profile".to_string(),
        );
        method.description = Some("Choose a profile from a config folder.".to_string());
        method.settings = vec![CONFIG_DIR_SETTING.to_string()];
        vec![method]
    }

    fn credential_kind(
        &self,
        files: &AwsFiles,
        profile: &str,
    ) -> Result<AwsCredentialKind, ProviderError> {
        let profiles = read_profile_auth(&files.config, &files.credentials)
            .map_err(|error| ProviderError::new(ErrorKind::Other, error.to_string()))?;
        Ok(profiles
            .into_iter()
            .find(|candidate| candidate.name == profile)
            .map_or(AwsCredentialKind::Unconfigured, |found| {
                found.credential_kind
            }))
    }

    fn begin(
        &self,
        files: &AwsFiles,
        method: &AuthMethod,
        input: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        if method.id != PROFILE_METHOD_ID {
            return Err(ProviderError::unsupported());
        }
        let Some(connection) = input.connection else {
            return Err(ProviderError::new(
                ErrorKind::Unsupported,
                "Choose a profile to sign in to.",
            ));
        };
        let kind = self.credential_kind(files, connection.as_str())?;
        match login_guidance(&kind) {
            LoginGuidance::BrowserSignIn(_) => {}
            LoginGuidance::AlreadyConfigured => {
                return Err(ProviderError::new(
                    ErrorKind::Unsupported,
                    "This profile already has usable credentials; browser sign-in is not needed.",
                ));
            }
            LoginGuidance::Unsupported { reason } => {
                return Err(ProviderError::new(ErrorKind::Unsupported, reason));
            }
        }
        let (authorization, pending) = block_on(self.sso.begin(&kind)).map_err(sso_error)?;
        let flow = FlowId::from(format!(
            "sso-{}",
            self.next_flow.fetch_add(1, Ordering::Relaxed)
        ));
        let step = device_code_step(flow.clone(), &authorization);
        self.flows()?.insert(
            flow,
            Arc::new(Flow {
                authorization,
                pending,
            }),
        );
        Ok(step)
    }

    fn continue_flow(&self, flow: &FlowId, answer: AuthAnswer) -> Result<AuthStep, ProviderError> {
        match answer {
            AuthAnswer::Poll => self.poll(flow),
            AuthAnswer::Cancel => self.cancel(flow),
            AuthAnswer::Values(_) => Err(ProviderError::unsupported()),
        }
    }

    fn poll(&self, flow_id: &FlowId) -> Result<AuthStep, ProviderError> {
        let flow = self.find_flow(flow_id)?;
        let completed = block_on(flow.pending.try_complete_into(&self.home)).map_err(sso_error)?;
        if completed {
            self.flows()?.remove(flow_id);
            return Ok(AuthStep::Done);
        }
        Ok(device_code_step(flow_id.clone(), &flow.authorization))
    }

    fn cancel(&self, flow_id: &FlowId) -> Result<AuthStep, ProviderError> {
        self.find_flow(flow_id)?;
        self.flows()?.remove(flow_id);
        Err(ProviderError::new(
            ErrorKind::Other,
            "Browser sign-in was cancelled.",
        ))
    }

    fn find_flow(&self, flow_id: &FlowId) -> Result<Arc<Flow>, ProviderError> {
        self.flows()?.get(flow_id).cloned().ok_or_else(|| {
            ProviderError::new(
                ErrorKind::Auth,
                "This sign-in is no longer active. Choose Sign in again to get a new code.",
            )
        })
    }

    fn status(&self, files: &AwsFiles, connection: Option<&ConnectionId>) -> AuthStatus {
        let Some(connection) = connection else {
            return AuthStatus::SignedOut;
        };
        match self.credential_kind(files, connection.as_str()) {
            Ok(AwsCredentialKind::Sso(config)) => self.sso_token_status(&config),
            Ok(AwsCredentialKind::SharedCredentials) => AuthStatus::SignedIn { expires: None },
            Ok(_) => AuthStatus::SignedOut,
            Err(_) => AuthStatus::Unknown,
        }
    }

    fn sso_token_status(&self, config: &AwsSsoConfiguration) -> AuthStatus {
        let path = sso_cache_path(sso_cache_identifier(config), &self.home);
        let Some(expires) = read_cached_expiry(&path) else {
            return AuthStatus::SignedOut;
        };
        let expires = SystemTime::from(expires);
        if expires <= SystemTime::now() {
            AuthStatus::Expired
        } else {
            AuthStatus::SignedIn {
                expires: Some(expires),
            }
        }
    }

    fn sign_out(
        &self,
        files: &AwsFiles,
        connection: Option<&ConnectionId>,
    ) -> Result<(), ProviderError> {
        let Some(connection) = connection else {
            return Ok(());
        };
        let AwsCredentialKind::Sso(sso) = self.credential_kind(files, connection.as_str())? else {
            return Ok(());
        };
        let path = sso_cache_path(sso_cache_identifier(&sso), &self.home);
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ProviderError::new(
                ErrorKind::Other,
                format!("Could not remove the cached AWS SSO token: {error}"),
            )),
        }
    }
}

fn read_cached_expiry(path: &std::path::Path) -> Option<chrono::DateTime<chrono::Utc>> {
    let contents = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    let expires_at = value["expiresAt"].as_str()?;
    chrono::DateTime::parse_from_rfc3339(expires_at)
        .ok()
        .map(|expires| expires.with_timezone(&chrono::Utc))
}

fn device_code_step(flow: FlowId, authorization: &SsoDeviceAuthorization) -> AuthStep {
    AuthStep::DeviceCode {
        flow,
        code: authorization.user_code.clone(),
        url: authorization.verification_uri.clone(),
        complete_url: authorization.verification_uri_complete.clone(),
        expires_in: authorization.expires_in,
    }
}

fn sso_error(error: clouddeck_provider_kit::BoxError) -> ProviderError {
    ProviderError::new(ErrorKind::Other, error.to_string())
}

fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("building the AWS sign-in runtime")
    })
}

/// The provider trait is synchronous; the AWS SDK needs a tokio reactor.
/// Callers must not already be inside a tokio runtime.
fn block_on<T>(future: impl Future<Output = T>) -> T {
    runtime().block_on(future)
}

/// Runs an SDK call bounded by the caller's deadline, if it has one.
fn within_deadline<T>(
    cx: &Call<'_>,
    work: impl Future<Output = Result<T, BoxError>>,
) -> Result<T, ProviderError> {
    block_on(async {
        let outcome = match cx.remaining() {
            Some(remaining) => tokio::time::timeout(remaining, work)
                .await
                .map_err(|_| ProviderError::of(ErrorKind::Timeout))?,
            None => work.await,
        };
        outcome.map_err(|error| ProviderError::new(ErrorKind::Other, error.to_string()))
    })
}

fn first_region(selection: &Selection) -> String {
    selection
        .values(REGION)
        .first()
        .map(|value| value.as_str().to_string())
        .unwrap_or_default()
}

struct AwsProvider {
    inner: ComputeServiceProvider,
    auth: AwsAuth,
}

pub(crate) fn aws_files(cx: &Call<'_>) -> AwsFiles {
    aws_files_for(
        cx.host.setting(CONFIG_DIR_SETTING).as_deref(),
        cx.host.setting(CONFIG_FILE_SETTING).as_deref(),
    )
}

fn setting_defs() -> Vec<SettingDef> {
    let option = |seconds: &str| SettingOption {
        value: seconds.into(),
        label: format!("{seconds} seconds"),
    };
    vec![
        SettingDef {
            key: CONFIG_DIR_SETTING.into(),
            label: "Config folder".into(),
            description: Some("An .aws-style folder with config and credentials. Empty uses ~/.aws or AWS_CONFIG_FILE.".into()),
            group: None,
            kind: SettingKind::Folder,
            default: None,
            secret: false,
        },
        SettingDef {
            key: CONFIG_FILE_SETTING.into(),
            label: "AWS config file".into(),
            description: Some("Overrides the config file. Wins over the config folder.".into()),
            group: None,
            kind: SettingKind::Path,
            default: Some(SettingValue::Text(DEFAULT_AWS_CONFIG_FILE.into())),
            secret: false,
        },
        SettingDef {
            key: REQUEST_TIMEOUT_SETTING.into(),
            label: "Request timeout".into(),
            description: Some("Per region, per request.".into()),
            group: None,
            kind: SettingKind::Choice(vec![option("15"), option("30"), option("60")]),
            default: Some(SettingValue::Text("30".into())),
            secret: false,
        },
    ]
}

impl Provider for AwsProvider {
    fn info(&self) -> ProviderInfo {
        self.inner.info()
    }

    fn auth(&self) -> Option<&dyn Auth> {
        Some(self)
    }

    fn config(&self) -> Option<&dyn Config> {
        Some(self)
    }
}

impl Browse for AwsProvider {
    fn connections(&self, cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        let profiles = load_profiles_for(&aws_files(cx)).unwrap_or_default();
        Ok(profiles
            .into_iter()
            .map(|profile| Connection {
                id: ConnectionId::from(profile.name.as_str()),
                display_name: profile.name,
            })
            .collect())
    }

    fn dimensions(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError> {
        self.inner.dimensions(cx, connection)
    }

    fn kinds(&self) -> Vec<KindInfo> {
        self.inner.kinds()
    }

    fn list(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> ListResult {
        match ScopeSettings::from_host(cx.host) {
            Some(ScopeSettings {
                demo: Some(demo),
                fleet,
                ..
            }) => list_scenario(demo, connection, kind, selection, fleet),
            Some(settings) => routed_list(cx, connection, kind, selection, &settings),
            None => self.inner.list(cx, connection, kind, selection),
        }
    }

    fn console_url(
        &self,
        connection: &ConnectionId,
        kind: &KindId,
        resource: &Instance,
    ) -> Option<String> {
        self.inner.console_url(connection, kind, resource)
    }

    fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
        self.inner.fields(kind)
    }

    fn inspector(&self, kind: &KindId) -> Vec<InspectorSectionDef> {
        self.inner.inspector(kind)
    }
}

impl Auth for AwsProvider {
    fn auth_methods(&self) -> Vec<AuthMethod> {
        self.auth.methods()
    }

    fn begin_auth(
        &self,
        cx: &Call<'_>,
        method: &AuthMethod,
        input: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        self.auth.begin(&aws_files(cx), method, input)
    }

    fn continue_auth(
        &self,
        _cx: &Call<'_>,
        flow: &FlowId,
        answer: AuthAnswer,
    ) -> Result<AuthStep, ProviderError> {
        self.auth.continue_flow(flow, answer)
    }

    fn auth_status(&self, cx: &Call<'_>, connection: Option<&ConnectionId>) -> AuthStatus {
        self.auth.status(&aws_files(cx), connection)
    }

    fn sign_out(
        &self,
        cx: &Call<'_>,
        connection: Option<&ConnectionId>,
    ) -> Result<(), ProviderError> {
        self.auth.sign_out(&aws_files(cx), connection)
    }

    fn identity(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        selection: &Selection,
    ) -> Result<String, ProviderError> {
        let service = AwsIdentityService::new(connection.as_str(), first_region(selection))
            .with_files(aws_files(cx));
        within_deadline(cx, service.resolve()).map(|identity| identity.account_id)
    }

    fn check_access(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        _kind: &KindId,
        selection: &Selection,
    ) -> Result<(), ProviderError> {
        within_deadline(
            cx,
            verify_instance_access(
                connection.as_str().to_string(),
                first_region(selection),
                aws_files(cx),
            ),
        )
    }
}

impl Config for AwsProvider {
    fn settings(&self) -> Vec<SettingDef> {
        setting_defs()
    }
}

pub fn aws_provider() -> Arc<dyn Provider> {
    Arc::from(boxed_aws_provider())
}

pub(crate) fn boxed_aws_provider() -> Box<dyn Provider> {
    let regions = REGIONS.iter().map(|region| (*region).to_string()).collect();
    let inner = ComputeServiceProvider::new("aws", Vec::new(), regions, |profile, region| {
        Box::new(AwsSdkComputeService::new(profile, region))
    });
    let auth = AwsAuth::new(home_dir(), AwsSsoLoginService::new());
    Box::new(AwsProvider { inner, auth })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aws_sso_login::TokenPoll;
    use crate::aws_sso_login::tests::{FakeProvider, fake_provider};
    use std::time::Duration;

    const SSO_PROFILE: &str = "\
[profile sso]
sso_start_url = https://example.awsapps.com/start
sso_region = us-east-1
sso_account_id = 123456789012
sso_role_name = Admin

[profile external]
credential_process = /usr/local/bin/creds
";

    struct Fixture {
        _dir: tempfile::TempDir,
        auth: AwsAuth,
        home: PathBuf,
        files: AwsFiles,
    }

    fn fixture(fake: Arc<FakeProvider>) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().to_path_buf();
        let files = AwsFiles {
            config: home.join("config"),
            credentials: home.join("credentials"),
        };
        std::fs::write(&files.config, SSO_PROFILE).unwrap();
        let auth = AwsAuth::new(home.clone(), AwsSsoLoginService::with_provider(fake));
        Fixture {
            _dir: dir,
            auth,
            home,
            files,
        }
    }

    fn approved_after_one_pending_poll() -> Arc<FakeProvider> {
        fake_provider(
            Duration::from_secs(1),
            vec![
                Ok(TokenPoll::Pending),
                Ok(TokenPoll::Ready(crate::aws_sso_login::tests::issued_token())),
            ],
        )
    }

    fn sso_method(fixture: &Fixture) -> AuthMethod {
        fixture.auth.methods().remove(0)
    }

    fn input(profile: &str) -> AuthInput {
        AuthInput {
            connection: Some(ConnectionId::from(profile)),
            values: Default::default(),
        }
    }

    fn begin_sso(fixture: &Fixture) -> AuthStep {
        fixture
            .auth
            .begin(&fixture.files, &sso_method(fixture), input("sso"))
            .unwrap()
    }

    fn flow_of(step: &AuthStep) -> FlowId {
        match step {
            AuthStep::DeviceCode { flow, .. } => flow.clone(),
            other => panic!("expected a device code step, got {other:?}"),
        }
    }

    #[test]
    fn begin_on_an_sso_profile_returns_a_device_code_with_a_flow() {
        let fixture = fixture(approved_after_one_pending_poll());

        let step = begin_sso(&fixture);

        assert!(matches!(
            &step,
            AuthStep::DeviceCode { code, url, .. }
                if code == "ABCD-EFGH" && url.starts_with("https://device.sso")
        ));
        flow_of(&step);
    }

    #[test]
    fn poll_while_pending_keeps_the_same_flow_and_poll_after_approval_is_done() {
        let fixture = fixture(approved_after_one_pending_poll());
        let flow = flow_of(&begin_sso(&fixture));

        let pending = fixture.auth.continue_flow(&flow, AuthAnswer::Poll).unwrap();
        assert_eq!(flow_of(&pending), flow);

        assert_eq!(
            fixture.auth.continue_flow(&flow, AuthAnswer::Poll),
            Ok(AuthStep::Done)
        );
        let cache = sso_cache_path(sso_cache_identifier(&sso_config()), &fixture.home);
        assert!(cache.is_file());
    }

    #[test]
    fn continue_with_an_unknown_flow_is_an_auth_error() {
        let fixture = fixture(approved_after_one_pending_poll());

        let error = fixture
            .auth
            .continue_flow(&FlowId::from("sso-missing"), AuthAnswer::Poll)
            .unwrap_err();

        assert_eq!(error.kind, ErrorKind::Auth);
    }

    #[test]
    fn begin_on_an_external_provider_profile_is_unsupported() {
        let fixture = fixture(approved_after_one_pending_poll());

        let error = fixture
            .auth
            .begin(&fixture.files, &sso_method(&fixture), input("external"))
            .unwrap_err();

        assert_eq!(error.kind, ErrorKind::Unsupported);
    }

    #[test]
    fn status_follows_the_cache_file_through_sign_in_expiry_and_sign_out() {
        let fixture = fixture(approved_after_one_pending_poll());
        let connection = ConnectionId::from("sso");
        assert_eq!(
            fixture.auth.status(&fixture.files, Some(&connection)),
            AuthStatus::SignedOut
        );

        let flow = flow_of(&begin_sso(&fixture));
        fixture.auth.continue_flow(&flow, AuthAnswer::Poll).unwrap();
        assert_eq!(
            fixture.auth.continue_flow(&flow, AuthAnswer::Poll),
            Ok(AuthStep::Done)
        );
        assert!(matches!(
            fixture.auth.status(&fixture.files, Some(&connection)),
            AuthStatus::SignedIn { expires: Some(_) }
        ));

        let cache = sso_cache_path(sso_cache_identifier(&sso_config()), &fixture.home);
        std::fs::write(
            &cache,
            r#"{"accessToken":"expired","expiresAt":"2001-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        assert_eq!(
            fixture.auth.status(&fixture.files, Some(&connection)),
            AuthStatus::Expired
        );

        fixture
            .auth
            .sign_out(&fixture.files, Some(&connection))
            .unwrap();
        assert!(!cache.exists());
        assert_eq!(
            fixture.auth.status(&fixture.files, Some(&connection)),
            AuthStatus::SignedOut
        );
    }

    #[test]
    fn unreadable_config_leaves_sign_in_unknown() {
        let fixture = fixture(approved_after_one_pending_poll());
        let config = fixture.home.join("config");
        std::fs::remove_file(&config).unwrap();
        std::fs::create_dir(&config).unwrap();

        assert_eq!(
            fixture
                .auth
                .status(&fixture.files, Some(&ConnectionId::from("sso"))),
            AuthStatus::Unknown
        );
    }

    #[test]
    fn sign_in_is_one_profile_method_that_takes_the_config_folder() {
        let fixture = fixture(approved_after_one_pending_poll());

        let methods = fixture.auth.methods();

        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].id, "profile");
        assert_eq!(methods[0].settings, ["config_dir"]);
    }

    fn sso_config() -> AwsSsoConfiguration {
        AwsSsoConfiguration {
            start_url: "https://example.awsapps.com/start".into(),
            region: "us-east-1".into(),
            account_id: Some("123456789012".into()),
            role_name: Some("Admin".into()),
            session_name: None,
        }
    }
}

#[cfg(test)]
mod conformance {
    use std::path::Path;
    use std::time::Duration;

    use clouddeck_extension_api::conformance::{check_manifest_settings, check_provider};
    use clouddeck_provider_api::{
        ExtrasSection, Host, LogLevel, NoAuth, NoConfig, ProviderError, ResourceId,
    };
    use clouddeck_provider_kit::{
        BoxError, CloudDeckError, ComputeInstanceFilters, ComputeInstanceLoadResult,
        ComputeService, EC2Instance,
    };

    use super::*;
    use crate::aws_sso_login::TokenPoll;
    use crate::aws_sso_login::tests::fake_provider;

    const PROFILE: &str = "\
[profile conformance]
sso_start_url = https://example.awsapps.com/start
sso_region = us-east-1
sso_account_id = 123456789012
sso_role_name = Admin
";

    struct FakeSdk {
        region: String,
    }

    #[async_trait::async_trait]
    impl ComputeService for FakeSdk {
        async fn list_instances(
            &self,
            _filters: Option<ComputeInstanceFilters>,
        ) -> Result<ComputeInstanceLoadResult, BoxError> {
            if self.region != "us-east-1" {
                return Err(Box::new(CloudDeckError::NotFound));
            }
            Ok(ComputeInstanceLoadResult {
                instances: vec![EC2Instance {
                    instance_id: "i-0123456789abcdef0".to_string(),
                    region: Some(self.region.clone()),
                    instance_type: None,
                    state: None,
                    launch_time: None,
                    public_ip_address: None,
                    private_ip_address: None,
                    vpc_id: None,
                    subnet_id: None,
                    security_groups: Vec::new(),
                    tags: Vec::new(),
                    availability_zone: None,
                    image_id: None,
                    key_name: None,
                    architecture: None,
                    state_transition_reason: None,
                    iam_instance_profile_arn: None,
                    private_dns_name: None,
                    public_dns_name: None,
                }],
                partial_failures: Vec::new(),
            })
        }
    }

    fn provider(dir: &Path) -> Box<dyn Provider> {
        let connections = vec![Connection {
            id: ConnectionId::from("conformance"),
            display_name: "conformance".to_string(),
        }];
        let inner = ComputeServiceProvider::new(
            "aws",
            connections,
            vec!["us-east-1".to_string()],
            |_, region| {
                Box::new(FakeSdk {
                    region: region.to_string(),
                })
            },
        );
        let sso = AwsSsoLoginService::with_provider(fake_provider(
            Duration::ZERO,
            (0..8).map(|_| Ok(TokenPoll::Pending)).collect(),
        ));
        let auth = AwsAuth::new(dir.to_path_buf(), sso);
        Box::new(AwsProvider { inner, auth })
    }

    struct NoHost;

    impl Host for NoHost {
        fn log(&self, _level: LogLevel, _message: &str) {}

        fn data_dir(&self) -> PathBuf {
            PathBuf::new()
        }

        fn setting(&self, _key: &str) -> Option<String> {
            None
        }

        fn secret(&self, _key: &str) -> Option<String> {
            None
        }

        fn set_secret(&self, _key: &str, _value: Option<&str>) -> Result<(), ProviderError> {
            Ok(())
        }

        fn open_url(&self, _url: &str) -> Result<(), ProviderError> {
            Ok(())
        }

        fn cancelled(&self) -> bool {
            false
        }
    }

    struct PinnedHost<'a> {
        base: &'a dyn Host,
        folder: &'a Path,
    }

    impl Host for PinnedHost<'_> {
        fn log(&self, level: LogLevel, message: &str) {
            self.base.log(level, message)
        }

        fn data_dir(&self) -> PathBuf {
            self.base.data_dir()
        }

        fn setting(&self, key: &str) -> Option<String> {
            if key == CONFIG_DIR_SETTING {
                return Some(self.folder.display().to_string());
            }
            self.base.setting(key)
        }

        fn secret(&self, key: &str) -> Option<String> {
            self.base.secret(key)
        }

        fn set_secret(&self, key: &str, value: Option<&str>) -> Result<(), ProviderError> {
            self.base.set_secret(key, value)
        }

        fn open_url(&self, url: &str) -> Result<(), ProviderError> {
            self.base.open_url(url)
        }

        fn cancelled(&self) -> bool {
            self.base.cancelled()
        }
    }

    /// Points the wrapped provider at `folder` for calls the conformance kit makes with its own host.
    struct Pinned {
        inner: Arc<dyn Provider>,
        folder: PathBuf,
    }

    impl Pinned {
        fn with_host<R>(&self, cx: &Call<'_>, f: impl FnOnce(&Call<'_>) -> R) -> R {
            let host = PinnedHost {
                base: cx.host,
                folder: &self.folder,
            };
            f(&Call {
                deadline: cx.deadline,
                host: &host,
            })
        }
    }

    impl Provider for Pinned {
        fn info(&self) -> ProviderInfo {
            self.inner.info()
        }

        fn auth(&self) -> Option<&dyn Auth> {
            Some(self)
        }

        fn config(&self) -> Option<&dyn Config> {
            Some(self)
        }
    }

    impl Browse for Pinned {
        fn connections(&self, cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
            self.with_host(cx, |cx| self.inner.connections(cx))
        }

        fn dimensions(
            &self,
            cx: &Call<'_>,
            connection: &ConnectionId,
        ) -> Result<Vec<Dimension>, ProviderError> {
            self.with_host(cx, |cx| self.inner.dimensions(cx, connection))
        }

        fn kinds(&self) -> Vec<KindInfo> {
            self.inner.kinds()
        }

        fn list(
            &self,
            cx: &Call<'_>,
            connection: &ConnectionId,
            kind: &KindId,
            selection: &Selection,
        ) -> ListResult {
            self.with_host(cx, |cx| self.inner.list(cx, connection, kind, selection))
        }

        fn describe(
            &self,
            cx: &Call<'_>,
            connection: &ConnectionId,
            kind: &KindId,
            id: &ResourceId,
        ) -> Result<Vec<ExtrasSection>, ProviderError> {
            self.with_host(cx, |cx| self.inner.describe(cx, connection, kind, id))
        }

        fn console_url(
            &self,
            connection: &ConnectionId,
            kind: &KindId,
            resource: &Instance,
        ) -> Option<String> {
            self.inner.console_url(connection, kind, resource)
        }

        fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
            self.inner.fields(kind)
        }

        fn inspector(&self, kind: &KindId) -> Vec<InspectorSectionDef> {
            self.inner.inspector(kind)
        }
    }

    impl Auth for Pinned {
        fn auth_methods(&self) -> Vec<AuthMethod> {
            self.inner.auth().unwrap_or(&NoAuth).auth_methods()
        }

        fn begin_auth(
            &self,
            cx: &Call<'_>,
            method: &AuthMethod,
            input: AuthInput,
        ) -> Result<AuthStep, ProviderError> {
            self.with_host(cx, |cx| {
                self.inner
                    .auth()
                    .unwrap_or(&NoAuth)
                    .begin_auth(cx, method, input)
            })
        }

        fn continue_auth(
            &self,
            cx: &Call<'_>,
            flow: &FlowId,
            answer: AuthAnswer,
        ) -> Result<AuthStep, ProviderError> {
            self.with_host(cx, |cx| {
                self.inner
                    .auth()
                    .unwrap_or(&NoAuth)
                    .continue_auth(cx, flow, answer)
            })
        }

        fn auth_status(&self, cx: &Call<'_>, connection: Option<&ConnectionId>) -> AuthStatus {
            self.with_host(cx, |cx| {
                self.inner
                    .auth()
                    .unwrap_or(&NoAuth)
                    .auth_status(cx, connection)
            })
        }

        fn sign_out(
            &self,
            cx: &Call<'_>,
            connection: Option<&ConnectionId>,
        ) -> Result<(), ProviderError> {
            self.with_host(cx, |cx| {
                self.inner
                    .auth()
                    .unwrap_or(&NoAuth)
                    .sign_out(cx, connection)
            })
        }

        fn identity(
            &self,
            cx: &Call<'_>,
            connection: &ConnectionId,
            selection: &Selection,
        ) -> Result<String, ProviderError> {
            self.with_host(cx, |cx| {
                self.inner
                    .auth()
                    .unwrap_or(&NoAuth)
                    .identity(cx, connection, selection)
            })
        }

        fn check_access(
            &self,
            cx: &Call<'_>,
            connection: &ConnectionId,
            kind: &KindId,
            selection: &Selection,
        ) -> Result<(), ProviderError> {
            self.with_host(cx, |cx| {
                self.inner
                    .auth()
                    .unwrap_or(&NoAuth)
                    .check_access(cx, connection, kind, selection)
            })
        }
    }

    impl Config for Pinned {
        fn settings(&self) -> Vec<SettingDef> {
            self.inner.config().unwrap_or(&NoConfig).settings()
        }

        fn settings_changed(&self, cx: &Call<'_>, changed: &[String]) {
            self.with_host(cx, |cx| {
                self.inner
                    .config()
                    .unwrap_or(&NoConfig)
                    .settings_changed(cx, changed)
            })
        }
    }

    #[test]
    fn provider_passes_the_conformance_kit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config"), PROFILE).unwrap();
        let pinned = Pinned {
            inner: Arc::from(provider(dir.path())),
            folder: dir.path().to_path_buf(),
        };
        let violations = check_provider(Arc::new(pinned));
        assert!(violations.is_empty(), "{violations:#?}");
    }

    #[test]
    fn manifest_settings_match_the_code() {
        let manifest =
            clouddeck_core::extension_host::Manifest::parse(include_str!("../extension.toml"))
                .unwrap();
        let provider = aws_provider();
        let violations = check_manifest_settings(&*provider, &manifest.settings);
        assert!(violations.is_empty(), "{}", violations.join("\n"));
    }

    #[test]
    fn connections_are_the_profile_scan_of_the_configured_folder() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        std::fs::write(first.path().join("config"), PROFILE).unwrap();
        std::fs::write(second.path().join("config"), "[profile other]\n").unwrap();
        let cx = Call {
            deadline: None,
            host: &NoHost,
        };
        let connections = |folder: &Path| -> Vec<String> {
            let pinned = Pinned {
                inner: Arc::from(provider(folder)),
                folder: folder.to_path_buf(),
            };
            pinned
                .connections(&cx)
                .unwrap()
                .into_iter()
                .map(|connection| connection.id.as_str().to_string())
                .collect()
        };

        assert_eq!(connections(first.path()), ["conformance"]);
        assert_eq!(connections(second.path()), ["other"]);
        let scan: Vec<String> = load_profiles_for(&aws_files_for(first.path().to_str(), None))
            .unwrap()
            .into_iter()
            .map(|profile| profile.name)
            .collect();
        assert_eq!(scan, ["conformance"]);
    }
}
