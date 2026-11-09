//! Settings that select the config folder, endpoints and auth mode, read through `Host::setting` on every call.

use std::io;
use std::path::{Path, PathBuf};

use clouddeck_extension_api::{
    ErrorKind, Host, ProviderError, SettingDef, SettingKind, SettingOption, SettingValue,
};
use serde::Deserialize;

pub const CONFIG_DIR_SETTING: &str = "config_dir";
pub const COMPUTE_SETTING: &str = "compute_endpoint";
pub const RESOURCE_MANAGER_SETTING: &str = "resource_manager_endpoint";
pub const TOKEN_SETTING: &str = "token_endpoint";
pub const AUTH_MODE_SETTING: &str = "auth_mode";

pub const EMULATOR_TOKEN: &str = "floci-emulator";
const EMULATOR_FILE: &str = "emulator.json";

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
    pub folder: Option<PathBuf>,
}

#[derive(Deserialize)]
struct EmulatorFile {
    endpoint: String,
}

impl Config {
    pub fn from_host(host: &dyn Host) -> Result<Self, ProviderError> {
        let dir = setting(host, CONFIG_DIR_SETTING, "");
        let folder = (!dir.is_empty()).then(|| PathBuf::from(dir));
        let emulator = folder
            .as_deref()
            .map(emulator_endpoint)
            .transpose()?
            .flatten();
        let auth = match setting(host, AUTH_MODE_SETTING, "adc").as_str() {
            "adc" if emulator.is_some() => AuthMode::Emulator,
            "adc" => AuthMode::Adc,
            "emulator" => AuthMode::Emulator,
            other => {
                return Err(ProviderError::new(
                    ErrorKind::Other,
                    format!("{AUTH_MODE_SETTING} {other:?} is not adc or emulator"),
                ));
            }
        };
        let emulated = emulator.as_deref();
        Ok(Self {
            compute: endpoint(host, COMPUTE_SETTING, DEFAULT_COMPUTE, emulated)?,
            resource_manager: endpoint(
                host,
                RESOURCE_MANAGER_SETTING,
                DEFAULT_RESOURCE_MANAGER,
                emulated,
            )?,
            token: endpoint(host, TOKEN_SETTING, DEFAULT_TOKEN, None)?,
            auth,
            folder,
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
        SettingDef {
            key: CONFIG_DIR_SETTING.into(),
            label: "Config folder".into(),
            description: Some("A gcloud-style folder with application_default_credentials.json and optionally emulator.json. Empty uses GOOGLE_APPLICATION_CREDENTIALS or ~/.config/gcloud.".into()),
            group: None,
            kind: SettingKind::Folder,
            default: None,
            secret: false,
        },
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

/// `Host::setting` returns the default when a value is unset, so a value equal to the default
/// counts as not chosen and an emulator endpoint from the config folder takes its place.
fn endpoint(
    host: &dyn Host,
    key: &str,
    default: &str,
    emulator: Option<&str>,
) -> Result<String, ProviderError> {
    let value = setting(host, key, default);
    let chosen = match emulator {
        Some(url) if value == default => url,
        _ => value.as_str(),
    };
    checked(key, chosen)
}

fn checked(name: &str, raw: &str) -> Result<String, ProviderError> {
    let url = raw.trim().trim_end_matches('/');
    if allowed(url) {
        Ok(url.to_string())
    } else {
        Err(ProviderError::new(
            ErrorKind::Other,
            format!(
                "{name} {url:?} must start with https://, or with http://localhost, http://127.0.0.1 or http://host.docker.internal"
            ),
        ))
    }
}

/// Reads `<folder>/emulator.json`; a missing file means the folder is not an emulator.
fn emulator_endpoint(folder: &Path) -> Result<Option<String>, ProviderError> {
    let path = folder.join(EMULATOR_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(ProviderError::new(
                ErrorKind::Other,
                format!("cannot read {}: {e}", path.display()),
            ));
        }
    };
    let file: EmulatorFile = serde_json::from_str(&text).map_err(|e| {
        ProviderError::new(
            ErrorKind::Other,
            format!("{} is not {{\"endpoint\": \"...\"}}: {e}", path.display()),
        )
    })?;
    checked("emulator.json endpoint", &file.endpoint).map(Some)
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
