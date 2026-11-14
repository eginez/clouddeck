use serde::{Deserialize, Serialize};

use crate::auth::{AuthAnswer, AuthInput, AuthMethod, AuthStatus, AuthStep};
use crate::error::ProviderError;
use crate::field::FieldDef;
use crate::host::Call;
use crate::ids::{ConnectionId, FlowId, KindId, ResourceId};
use crate::inspector::InspectorSectionDef;
use crate::instance::{ExtrasSection, Instance};
use crate::selection::{Dimension, Selection};
use crate::settings::SettingDef;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub id: ConnectionId,
    pub display_name: String,
    /// Second line under the name, such as an account number and region.
    #[serde(default)]
    pub detail: Option<String>,
    /// Short pills shown on the row, such as "SSO".
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Onboarding text for a provider. Absent fields keep the core's wording.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Onboarding {
    /// Pane heading; the provider name when absent.
    #[serde(default)]
    pub title: Option<String>,
    /// Line under the heading.
    #[serde(default)]
    pub subtitle: Option<String>,
    /// Footer note on the left of the pane.
    #[serde(default)]
    pub footer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub connection_label: String,
    #[serde(default)]
    pub onboarding: Onboarding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KindInfo {
    pub id: KindId,
    pub label: String,
}

/// `cell` names the exact dimension values that failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CellFailure {
    pub cell: Selection,
    pub error: ProviderError,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ListResult {
    pub resources: Vec<Instance>,
    pub failures: Vec<CellFailure>,
}

/// What a provider exposes to browse: connections, dimensions, kinds, resources and their fields.
pub trait Browse: Send + Sync {
    /// Err with kind Auth means the user must sign in first.
    fn connections(&self, cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError>;
    fn dimensions(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError>;
    fn kinds(&self) -> Vec<KindInfo>;
    fn list(
        &self,
        cx: &Call<'_>,
        connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> ListResult;

    fn describe(
        &self,
        _cx: &Call<'_>,
        _connection: &ConnectionId,
        _kind: &KindId,
        _id: &ResourceId,
    ) -> Result<Vec<ExtrasSection>, ProviderError> {
        Err(ProviderError::unsupported())
    }

    fn console_url(
        &self,
        connection: &ConnectionId,
        kind: &KindId,
        resource: &Instance,
    ) -> Option<String>;
    fn fields(&self, kind: &KindId) -> Vec<FieldDef>;

    fn inspector(&self, _kind: &KindId) -> Vec<InspectorSectionDef> {
        Vec::new()
    }
}

/// How a connection signs in and whether it is signed in.
pub trait Auth: Send + Sync {
    fn auth_methods(&self) -> Vec<AuthMethod> {
        Vec::new()
    }

    fn begin_auth(
        &self,
        _cx: &Call<'_>,
        _method: &AuthMethod,
        _input: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        Err(ProviderError::unsupported())
    }

    fn continue_auth(
        &self,
        _cx: &Call<'_>,
        _flow: &FlowId,
        _answer: AuthAnswer,
    ) -> Result<AuthStep, ProviderError> {
        Err(ProviderError::unsupported())
    }

    /// `None` asks about the provider as a whole rather than one connection.
    fn auth_status(&self, _cx: &Call<'_>, _connection: Option<&ConnectionId>) -> AuthStatus {
        AuthStatus::SignedIn { expires: None }
    }

    fn sign_out(
        &self,
        _cx: &Call<'_>,
        _connection: Option<&ConnectionId>,
    ) -> Result<(), ProviderError> {
        Ok(())
    }

    /// The account the connection signs in as, checked with the selection's scope.
    fn identity(
        &self,
        _cx: &Call<'_>,
        _connection: &ConnectionId,
        _selection: &Selection,
    ) -> Result<String, ProviderError> {
        Err(ProviderError::unsupported())
    }

    /// Confirms the connection can list `kind` within the selection's scope.
    fn check_access(
        &self,
        _cx: &Call<'_>,
        _connection: &ConnectionId,
        _kind: &KindId,
        _selection: &Selection,
    ) -> Result<(), ProviderError> {
        Err(ProviderError::unsupported())
    }
}

/// Settings this provider reads through `Host::setting`. Also declared in the manifest.
pub trait Config: Send + Sync {
    fn settings(&self) -> Vec<SettingDef> {
        Vec::new()
    }

    /// Called after the host stored a new value for one of `changed`, so caches can be dropped.
    fn settings_changed(&self, _cx: &Call<'_>, _changed: &[String]) {}
}

/// A provider without `auth()` needs no sign-in; without `config()` it has no settings.
pub trait Provider: Browse {
    fn info(&self) -> ProviderInfo;
    fn auth(&self) -> Option<&dyn Auth> {
        None
    }
    fn config(&self) -> Option<&dyn Config> {
        None
    }
}

/// Callers use this in place of a missing `auth()`: every method is the trait default.
pub struct NoAuth;

impl Auth for NoAuth {}

/// Callers use this in place of a missing `config()`: no settings.
pub struct NoConfig;

impl Config for NoConfig {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::LogLevel;
    use crate::ids::ValueId;
    use crate::selection::DimValue;
    use std::path::PathBuf;
    use std::sync::Arc;

    struct NullHost;

    impl crate::host::Host for NullHost {
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

    struct EmptyProvider;

    impl Provider for EmptyProvider {
        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: "empty".to_string(),
                name: "Empty".to_string(),
                version: "0".to_string(),
                connection_label: "Profile".to_string(),
                onboarding: Default::default(),
            }
        }
    }

    impl Auth for EmptyProvider {}

    impl Browse for EmptyProvider {
        fn connections(&self, _cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
            Ok(vec![])
        }

        fn dimensions(
            &self,
            _cx: &Call<'_>,
            _connection: &ConnectionId,
        ) -> Result<Vec<Dimension>, ProviderError> {
            Ok(vec![])
        }

        fn kinds(&self) -> Vec<KindInfo> {
            vec![]
        }

        fn list(
            &self,
            _cx: &Call<'_>,
            _connection: &ConnectionId,
            _kind: &KindId,
            _selection: &Selection,
        ) -> ListResult {
            ListResult::default()
        }

        fn console_url(
            &self,
            _connection: &ConnectionId,
            _kind: &KindId,
            _resource: &Instance,
        ) -> Option<String> {
            None
        }

        fn fields(&self, _kind: &KindId) -> Vec<FieldDef> {
            vec![]
        }
    }

    struct TreeProvider;

    impl Provider for TreeProvider {
        fn info(&self) -> ProviderInfo {
            EmptyProvider.info()
        }
    }

    impl Browse for TreeProvider {
        fn connections(&self, _cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
            Ok(vec![])
        }

        fn dimensions(
            &self,
            _cx: &Call<'_>,
            _connection: &ConnectionId,
        ) -> Result<Vec<Dimension>, ProviderError> {
            Ok(vec![Dimension {
                id: "place".into(),
                label: "Place".to_string(),
                plural_label: None,
                multi: true,
                optional: true,
                values: vec![
                    DimValue {
                        id: "us".into(),
                        label: "US".to_string(),
                        parent: None,
                        default: false,
                    },
                    DimValue {
                        id: "us-east".into(),
                        label: "US East".to_string(),
                        parent: Some("us".into()),
                        default: true,
                    },
                ],
            }])
        }

        fn kinds(&self) -> Vec<KindInfo> {
            vec![]
        }

        fn list(
            &self,
            _cx: &Call<'_>,
            _connection: &ConnectionId,
            _kind: &KindId,
            _selection: &Selection,
        ) -> ListResult {
            ListResult::default()
        }

        fn console_url(
            &self,
            _connection: &ConnectionId,
            _kind: &KindId,
            _resource: &Instance,
        ) -> Option<String> {
            None
        }

        fn fields(&self, _kind: &KindId) -> Vec<FieldDef> {
            vec![]
        }
    }

    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn provider_is_object_safe_behind_box_and_arc() {
        let host = NullHost;
        let cx = Call {
            deadline: None,
            host: &host,
        };
        let default = ConnectionId::from("default");

        let boxed: Box<dyn Provider> = Box::new(EmptyProvider);
        assert_eq!(boxed.info().id, "empty");
        assert_eq!(
            boxed.list(&cx, &default, &KindId::instances(), &Selection::new()),
            ListResult::default()
        );

        let shared: Arc<dyn Provider> = Arc::new(EmptyProvider);
        assert_eq!(shared.connections(&cx), Ok(vec![]));
        assert_eq!(shared.dimensions(&cx, &default), Ok(vec![]));
    }

    #[test]
    fn dyn_provider_is_send_and_sync() {
        assert_send_sync::<dyn Provider>();
    }

    #[test]
    fn default_describe_is_unsupported() {
        let host = NullHost;
        let cx = Call {
            deadline: None,
            host: &host,
        };
        assert_eq!(
            EmptyProvider.describe(
                &cx,
                &ConnectionId::from("default"),
                &KindId::instances(),
                &ResourceId::from("i-1"),
            ),
            Err(ProviderError::unsupported())
        );
    }

    #[test]
    fn default_identity_and_access_are_unsupported() {
        let host = NullHost;
        let cx = Call {
            deadline: None,
            host: &host,
        };
        let connection = ConnectionId::from("default");
        assert_eq!(
            EmptyProvider.identity(&cx, &connection, &Selection::new()),
            Err(ProviderError::unsupported())
        );
        assert_eq!(
            EmptyProvider.check_access(&cx, &connection, &KindId::instances(), &Selection::new()),
            Err(ProviderError::unsupported())
        );
    }

    #[test]
    fn default_inspector_is_empty_for_any_kind() {
        assert!(EmptyProvider.inspector(&KindId::instances()).is_empty());
        assert!(
            EmptyProvider
                .inspector(&KindId::from("services"))
                .is_empty()
        );
    }

    #[test]
    fn default_auth_is_empty_unsupported_and_signed_in() {
        let host = NullHost;
        let cx = Call {
            deadline: None,
            host: &host,
        };
        let method = AuthMethod::new("method".to_string(), "Method".to_string());
        let input = AuthInput {
            connection: Some(ConnectionId::from("default")),
            values: Default::default(),
        };
        assert!(EmptyProvider.auth_methods().is_empty());
        assert_eq!(
            EmptyProvider.begin_auth(&cx, &method, input),
            Err(ProviderError::unsupported())
        );
        assert_eq!(
            EmptyProvider.continue_auth(&cx, &FlowId::from("flow"), AuthAnswer::Poll),
            Err(ProviderError::unsupported())
        );
        assert_eq!(
            EmptyProvider.auth_status(&cx, None),
            AuthStatus::SignedIn { expires: None }
        );
        assert_eq!(EmptyProvider.sign_out(&cx, None), Ok(()));
    }

    #[test]
    fn dimension_value_tree_round_trips() {
        let host = NullHost;
        let cx = Call {
            deadline: None,
            host: &host,
        };
        let dimensions = TreeProvider
            .dimensions(&cx, &ConnectionId::from("default"))
            .expect("dimensions");
        let values = &dimensions[0].values;
        assert_eq!(values[0].parent, None);
        assert_eq!(values[1].parent, Some(ValueId::from("us")));
    }

    #[test]
    fn selection_absent_dimension_is_empty() {
        let selection = Selection::new().with("region", vec![ValueId::from("eu-west-1")]);
        assert!(selection.values("zone").is_empty());
        assert_eq!(selection.values("region"), &[ValueId::from("eu-west-1")]);
        assert!(Selection::new().is_empty());
        assert!(!selection.is_empty());
    }

    #[test]
    fn payloads_without_onboarding_fields_decode() {
        let connection: Connection =
            serde_json::from_str(r#"{"id":"default","display_name":"Default"}"#)
                .expect("connection");
        assert_eq!(connection.detail, None);
        assert!(connection.tags.is_empty());

        let info: ProviderInfo = serde_json::from_str(
            r#"{"id":"empty","name":"Empty","version":"0","connection_label":"Profile"}"#,
        )
        .expect("provider info");
        assert_eq!(info.onboarding, Onboarding::default());

        let method: AuthMethod =
            serde_json::from_str(r#"{"id":"sso","label":"SSO"}"#).expect("auth method");
        assert_eq!(method.header_button, None);

        let error: ProviderError = serde_json::from_str(
            r#"{"kind":"Other","message":"boom","advice":null,"retry_after":null}"#,
        )
        .expect("provider error");
        assert_eq!(error.hint, None);
    }
}
