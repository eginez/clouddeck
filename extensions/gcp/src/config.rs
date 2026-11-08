//! Settings that select the endpoints and auth mode, read through `Host::setting` on every call.

use clouddeck_extension_api::{
    ErrorKind, Host, ProviderError, SettingDef, SettingKind, SettingOption, SettingValue,
};

pub const COMPUTE_SETTING: &str = "compute_endpoint";
pub const RESOURCE_MANAGER_SETTING: &str = "resource_manager_endpoint";
pub const TOKEN_SETTING: &str = "token_endpoint";
pub const AUTH_MODE_SETTING: &str = "auth_mode";

pub const EMULATOR_TOKEN: &str = "floci-emulator";

const GROUP: &str = "Advanced: endpoints";
const DEFAULT_COMPUTE: &str = "https://compute.googleapis.com";
const DEFAULT_RESOURCE_MANAGER: &str = "https://cloudresourcemanager.googleapis.com";
const DEFAULT_TOKEN: &str = "https://oauth2.googleapis.com/token";
const ENDPOINT_HELP: &str = "Must be https://, or a local emulator address.";
const LOCAL_EMULATORS: [&str; 3] = [
    "http://localhost",
    "http://127.0.0.1",
    "http://host.docker.internal",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    Adc,
    Emulator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub compute: String,
    pub resource_manager: String,
    pub token: String,
    pub auth: AuthMode,
}

impl Config {
    pub fn from_host(host: &dyn Host) -> Result<Self, ProviderError> {
        let auth = match setting(host, AUTH_MODE_SETTING, "adc").as_str() {
            "adc" => AuthMode::Adc,
            "emulator" => AuthMode::Emulator,
            other => {
                return Err(ProviderError::new(
                    ErrorKind::Other,
                    format!("{AUTH_MODE_SETTING} {other:?} is not adc or emulator"),
                ));
            }
        };
        Ok(Self {
            compute: endpoint(host, COMPUTE_SETTING, DEFAULT_COMPUTE)?,
            resource_manager: endpoint(host, RESOURCE_MANAGER_SETTING, DEFAULT_RESOURCE_MANAGER)?,
            token: endpoint(host, TOKEN_SETTING, DEFAULT_TOKEN)?,
            auth,
        })
    }
}

pub fn setting_defs() -> Vec<SettingDef> {
    let endpoint = |key: &str, label: &str, default: &str| SettingDef {
        key: key.into(),
        label: label.into(),
        description: Some(ENDPOINT_HELP.into()),
        group: Some(GROUP.into()),
        kind: SettingKind::Text,
        default: Some(SettingValue::Text(default.into())),
        secret: false,
    };
    let option = |value: &str, label: &str| SettingOption {
        value: value.into(),
        label: label.into(),
    };
    vec![
        endpoint(COMPUTE_SETTING, "Compute Engine endpoint", DEFAULT_COMPUTE),
        endpoint(
            RESOURCE_MANAGER_SETTING,
            "Resource Manager endpoint",
            DEFAULT_RESOURCE_MANAGER,
        ),
        endpoint(TOKEN_SETTING, "Token endpoint", DEFAULT_TOKEN),
        SettingDef {
            key: AUTH_MODE_SETTING.into(),
            label: "Authentication".into(),
            description: Some("Emulator mode skips ADC and sends a fixed token.".into()),
            group: Some(GROUP.into()),
            kind: SettingKind::Choice(vec![
                option("adc", "Application Default Credentials"),
                option("emulator", "Local emulator (fixed token)"),
            ]),
            default: Some(SettingValue::Text("adc".into())),
            secret: false,
        },
    ]
}

fn setting(host: &dyn Host, key: &str, default: &str) -> String {
    host.setting(key)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn endpoint(host: &dyn Host, key: &str, default: &str) -> Result<String, ProviderError> {
    let value = setting(host, key, default);
    let url = value.trim().trim_end_matches('/');
    if allowed(url) {
        Ok(url.to_string())
    } else {
        Err(ProviderError::new(
            ErrorKind::Other,
            format!(
                "{key} {url:?} must start with https://, or with http://localhost, http://127.0.0.1 or http://host.docker.internal"
            ),
        ))
    }
}

/// Pure: plain http is allowed only for a local emulator, so bearer tokens never cross the network unencrypted.
fn allowed(url: &str) -> bool {
    if let Some(rest) = url.strip_prefix("https://") {
        return !rest.is_empty() && !rest.starts_with('/');
    }
    LOCAL_EMULATORS.iter().any(|base| {
        let Some(rest) = url.strip_prefix(base) else {
            return false;
        };
        let after_port = match rest.strip_prefix(':') {
            Some(port) => port.trim_start_matches(|c: char| c.is_ascii_digit()),
            None => rest,
        };
        after_port.is_empty() || after_port.starts_with('/')
    })
}
