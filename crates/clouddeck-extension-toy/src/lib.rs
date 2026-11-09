//! Minimal extension used to exercise the boundary and the conformance kit.

use std::collections::BTreeMap;

use clouddeck_extension_api::{
    AuthAnswer, AuthInput, AuthMethod, AuthStep, Call, Connection, ConnectionId, Dimension,
    ErrorKind, Extension, FieldDef, FlowId, INSTANCES, Instance, KindId, KindInfo, ListResult,
    PromptField, PromptKind, Provider, ProviderError, ProviderInfo, ResourceId, Selection,
    SettingDef, SettingKind, SettingOption, SettingValue, register_extension,
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

const PROFILE_METHOD: &str = "profile";
const PROFILE_FLOW: &str = "toy-profile";
const CONFIG_FIELD: &str = "config_dir";
const PROFILES: [&str; 2] = ["alpha", "beta"];

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

fn profile_method() -> AuthMethod {
    AuthMethod {
        description: Some("Pick a config folder and a profile in it".into()),
        initial_fields: vec![config_field(""), profile_field()],
        ..AuthMethod::new(PROFILE_METHOD, "Profile in a folder")
    }
}

fn config_field(folder: &str) -> PromptField {
    PromptField {
        default: (!folder.is_empty()).then(|| folder.to_string()),
        help: Some("The folder that holds the profile's config".into()),
        required: true,
        refresh_on_change: true,
        persist_as: Some(CONFIG_FIELD.into()),
        ..PromptField::new(CONFIG_FIELD, "Config folder", PromptKind::Folder)
    }
}

fn profile_field() -> PromptField {
    PromptField {
        default: Some(PROFILES[0].into()),
        options: PROFILES
            .iter()
            .map(|name| SettingOption {
                value: (*name).into(),
                label: (*name).into(),
            })
            .collect(),
        required: true,
        ..PromptField::new("profile", "Profile", PromptKind::Choice)
    }
}

fn form_error(values: &BTreeMap<String, String>) -> Option<String> {
    if values.get(CONFIG_FIELD).is_none_or(String::is_empty) {
        return Some("Choose a config folder".into());
    }
    let profile = values
        .get("profile")
        .map(String::as_str)
        .unwrap_or(PROFILES[0]);
    (!PROFILES.contains(&profile)).then(|| "Choose one of the listed profiles".into())
}

fn profile_prompt(values: &BTreeMap<String, String>, error: Option<String>) -> AuthStep {
    let folder = values.get(CONFIG_FIELD).cloned().unwrap_or_default();
    let mut fields = vec![config_field(&folder), profile_field()];
    if folder.is_empty() {
        fields[0].error = error.clone();
    }
    AuthStep::Prompt {
        flow: FlowId::from(PROFILE_FLOW),
        fields,
        message: (!folder.is_empty())
            .then(|| format!("Found {} profiles in {folder}", PROFILES.len())),
        error: error.filter(|_| !folder.is_empty()),
    }
}

impl Provider for ToyProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "toy".into(),
            name: "Toy".into(),
            version: env!("CARGO_PKG_VERSION").into(),
            connection_label: "Profile".into(),
        }
    }

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
                key: CONFIG_FIELD.into(),
                label: "Config folder".into(),
                description: None,
                group: None,
                kind: SettingKind::Folder,
                default: None,
                secret: false,
            },
        ]
    }

    fn connections(&self, cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        let display_name = cx
            .host
            .setting("display_name")
            .unwrap_or_else(|| "Default".into());
        Ok(vec![Connection {
            id: "default".into(),
            display_name,
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

    fn auth_methods(&self) -> Vec<AuthMethod> {
        vec![profile_method()]
    }

    fn begin_auth(
        &self,
        _: &Call<'_>,
        method: &AuthMethod,
        input: AuthInput,
    ) -> Result<AuthStep, ProviderError> {
        if method.id != PROFILE_METHOD {
            return Err(ProviderError::unsupported());
        }
        let error = form_error(&input.values);
        Ok(profile_prompt(&input.values, error))
    }

    fn continue_auth(
        &self,
        _: &Call<'_>,
        flow: &FlowId,
        answer: AuthAnswer,
    ) -> Result<AuthStep, ProviderError> {
        if flow.as_str() != PROFILE_FLOW {
            return Err(ProviderError::new(ErrorKind::Other, "unknown sign-in flow"));
        }
        match answer {
            AuthAnswer::Change { values, .. } => Ok(profile_prompt(&values, None)),
            AuthAnswer::Values(values) => match form_error(&values) {
                Some(error) => Ok(profile_prompt(&values, Some(error))),
                None => Ok(AuthStep::Done),
            },
            AuthAnswer::Cancel => Ok(AuthStep::Done),
            AuthAnswer::Poll => Err(ProviderError::unsupported()),
        }
    }
}
