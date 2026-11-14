//! Minimal extension used to exercise the boundary and the conformance kit.

use clouddeck_extension_api::{
    Auth, AuthAnswer, AuthInput, AuthMethod, AuthStep, Browse, Call, Config, Connection,
    ConnectionId, Dimension, ErrorKind, Extension, FieldDef, FlowId, INSTANCES, Instance, KindId,
    KindInfo, ListResult, PromptField, Provider, ProviderError, ProviderInfo, ResourceId,
    Selection, SettingDef, SettingKind, SettingValue, register_extension,
};

pub struct Toy;

impl Extension for Toy {
    fn new() -> Self {
        Toy
    }

    fn provider(&self, id: &str) -> Option<Box<dyn Provider>> {
        (id == "toy").then(|| Box::new(ToyProvider) as Box<dyn Provider>)
    }
}

register_extension!(Toy);

pub struct ToyProvider;

fn toy_instances() -> Vec<Instance> {
    ["alpha", "beta"]
        .into_iter()
        .map(|name| Instance {
            id: name.to_string(),
            name: Some(name.to_string()),
            ..Default::default()
        })
        .collect()
}

impl Provider for ToyProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "toy".into(),
            name: "Toy".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            connection_label: "Profile".into(),
            onboarding: Default::default(),
        }
    }

    fn auth(&self) -> Option<&dyn Auth> {
        Some(self)
    }

    fn config(&self) -> Option<&dyn Config> {
        Some(self)
    }
}

impl Browse for ToyProvider {
    fn connections(&self, cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        let display_name = cx
            .host
            .setting("display_name")
            .unwrap_or_else(|| "Default".into());
        Ok(vec![Connection {
            id: "default".into(),
            display_name,
            detail: None,
            tags: Vec::new(),
        }])
    }

    fn dimensions(&self, _: &Call<'_>, _: &ConnectionId) -> Result<Vec<Dimension>, ProviderError> {
        Ok(Vec::new())
    }

    fn kinds(&self) -> Vec<KindInfo> {
        vec![KindInfo {
            id: KindId::instances(),
            label: "Instances".into(),
        }]
    }

    fn list(&self, _: &Call<'_>, _: &ConnectionId, kind: &KindId, _: &Selection) -> ListResult {
        if kind.as_str() != INSTANCES {
            return ListResult::default();
        }
        ListResult {
            resources: toy_instances(),
            failures: Vec::new(),
        }
    }

    fn describe(
        &self,
        _: &Call<'_>,
        _: &ConnectionId,
        _: &KindId,
        id: &ResourceId,
    ) -> Result<Vec<clouddeck_extension_api::ExtrasSection>, ProviderError> {
        toy_instances()
            .into_iter()
            .find(|i| i.id == id.as_str())
            .map(|i| i.extras)
            .ok_or_else(|| ProviderError::of(ErrorKind::NotFound))
    }

    fn console_url(&self, _: &ConnectionId, _: &KindId, _: &Instance) -> Option<String> {
        None
    }

    fn fields(&self, _: &KindId) -> Vec<FieldDef> {
        Vec::new()
    }
}

const TOKEN_METHOD: &str = "api-token";
const TOKEN_FLOW: &str = "toy-token";
const BROWSER_METHOD: &str = "browser";
const BROWSER_FLOW: &str = "toy-browser";

impl Auth for ToyProvider {
    fn auth_methods(&self) -> Vec<AuthMethod> {
        vec![
            AuthMethod {
                id: TOKEN_METHOD.into(),
                label: "API token".into(),
                header_button: Some("Connect…".into()),
                description: Some("Paste a token from the toy's dashboard.".into()),
                settings: Vec::new(),
            },
            AuthMethod {
                id: BROWSER_METHOD.into(),
                label: "Sign in with browser".into(),
                header_button: None,
                description: Some("Approve the sign-in in your browser.".into()),
                settings: Vec::new(),
            },
        ]
    }

    fn begin_auth(
        &self,
        _: &Call<'_>,
        method: &AuthMethod,
        _: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        match method.id.as_str() {
            TOKEN_METHOD => Ok(AuthStep::Prompt {
                flow: FlowId::from(TOKEN_FLOW),
                fields: vec![
                    PromptField {
                        id: "tenant".into(),
                        label: "Tenant".into(),
                        secret: false,
                    },
                    PromptField {
                        id: "api_token".into(),
                        label: "API token".into(),
                        secret: true,
                    },
                    PromptField {
                        id: "endpoint".into(),
                        label: "Endpoint".into(),
                        secret: false,
                    },
                ],
            }),
            BROWSER_METHOD => Ok(AuthStep::OpenUrl {
                flow: FlowId::from(BROWSER_FLOW),
                url: "https://example.test/toy/sign-in".into(),
                then_poll: true,
            }),
            _ => Err(ProviderError::of(ErrorKind::Unsupported)),
        }
    }

    fn continue_auth(
        &self,
        _: &Call<'_>,
        flow: &FlowId,
        answer: AuthAnswer,
    ) -> Result<AuthStep, ProviderError> {
        match (flow.as_str(), answer) {
            (TOKEN_FLOW, AuthAnswer::Values(_)) => Ok(AuthStep::Done),
            (BROWSER_FLOW, AuthAnswer::Poll | AuthAnswer::Cancel) => Ok(AuthStep::Done),
            _ => Err(ProviderError::of(ErrorKind::Unsupported)),
        }
    }
}

impl Config for ToyProvider {
    fn settings(&self) -> Vec<SettingDef> {
        vec![
            SettingDef {
                key: "display_name".into(),
                label: "Display name".into(),
                description: None,
                group: None,
                kind: SettingKind::Text,
                default: Some(SettingValue::Text("Default".into())),
                secret: false,
            },
            SettingDef {
                key: "config_dir".into(),
                label: "Config folder".into(),
                description: None,
                group: None,
                kind: SettingKind::Folder,
                default: None,
                secret: false,
            },
        ]
    }
}
