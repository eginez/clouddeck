use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use clouddeck_provider_api::{SettingDef, SettingKind, SettingOption, SettingValue};
use serde::{Deserialize, Serialize};

pub const MANIFEST_FILE: &str = "extension.toml";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub schema_version: u32,
    pub api_version: u32,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    pub repository: Option<String>,
    /// File inside the extension directory; defaults to `lib<id>` with the platform's library suffix.
    pub library: Option<String>,
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderDecl>,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default, rename = "settings")]
    setting_tables: Vec<SettingTable>,
    /// Settings the extension's providers declare; filled from `[[settings]]` when parsed.
    #[serde(skip)]
    pub settings: Vec<SettingDef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ProviderDecl {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingTable {
    key: String,
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    group: Option<String>,
    kind: String,
    #[serde(default)]
    default: Option<toml::Value>,
    #[serde(default)]
    options: Vec<SettingOptionTable>,
    #[serde(default)]
    min: Option<toml::Value>,
    #[serde(default)]
    max: Option<toml::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingOptionTable {
    value: String,
    label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum Capability {
    #[serde(rename = "network:http")]
    NetworkHttp { hosts: Vec<String> },
    #[serde(rename = "process:exec")]
    ProcessExec {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    #[serde(rename = "download_file")]
    DownloadFile { url: Vec<String> },
    #[serde(rename = "credentials")]
    Credentials,
    #[serde(rename = "secrets")]
    Secrets,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut manifest: Self =
            toml::from_str(text).map_err(|error| format!("invalid {MANIFEST_FILE}: {error}"))?;
        manifest.validate()?;
        manifest.settings = settings_from_tables(&manifest.setting_tables)?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<(), String> {
        if !is_valid_id(&self.id) {
            return Err(format!(
                "extension id {:?} may only contain lowercase letters, digits, '-' and '_'",
                self.id
            ));
        }
        if self.schema_version != SCHEMA_VERSION {
            return Err(format!(
                "{} uses manifest schema {}; this CloudDeck reads schema {SCHEMA_VERSION}",
                self.id, self.schema_version
            ));
        }
        Ok(())
    }

    pub fn check_api(&self, host_api: u32, host_version: &str) -> Result<(), String> {
        if self.api_version == host_api {
            return Ok(());
        }
        Err(format!(
            "{} was built for extension API {} but this CloudDeck {host_version} uses API {host_api}. Rebuild {} for CloudDeck {host_version}.",
            self.id, self.api_version, self.id
        ))
    }

    pub fn library_path(&self, dir: &Path) -> Result<PathBuf, String> {
        let file = self
            .library
            .clone()
            .unwrap_or_else(|| default_library_file(&self.id));
        let relative = Path::new(&file);
        let stays_inside = !file.is_empty()
            && relative
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !stays_inside {
            return Err(format!(
                "library {file:?} must be a file name inside the extension directory"
            ));
        }
        let dir = dir
            .canonicalize()
            .map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
        if fs::symlink_metadata(dir.join(relative)).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(format!(
                "library {file:?} is a symlink; copy the library into the extension directory instead"
            ));
        }
        let path = dir
            .join(relative)
            .canonicalize()
            .map_err(|error| format!("cannot find library {file:?}: {error}"))?;
        if !path.starts_with(&dir) {
            return Err(format!(
                "library {file:?} resolves outside the extension directory"
            ));
        }
        Ok(path)
    }
}

fn settings_from_tables(tables: &[SettingTable]) -> Result<Vec<SettingDef>, String> {
    let mut defs: Vec<SettingDef> = Vec::with_capacity(tables.len());
    for table in tables {
        let def =
            setting_def(table).map_err(|error| format!("invalid {MANIFEST_FILE}: {error}"))?;
        if defs.iter().any(|other| other.key == def.key) {
            return Err(format!(
                "invalid {MANIFEST_FILE}: setting {:?} is declared twice",
                def.key
            ));
        }
        defs.push(def);
    }
    Ok(defs)
}

fn setting_def(table: &SettingTable) -> Result<SettingDef, String> {
    let key = &table.key;
    let no_bounds = || -> Result<(), String> {
        if table.min.is_some() || table.max.is_some() {
            return Err(format!(
                "setting {key}: min and max only apply to number and duration_secs"
            ));
        }
        Ok(())
    };
    let no_options = || -> Result<(), String> {
        if !table.options.is_empty() {
            return Err(format!("setting {key}: options only apply to choice"));
        }
        Ok(())
    };
    let (kind, secret) = match table.kind.as_str() {
        "text" | "path" | "bool" => {
            no_bounds()?;
            no_options()?;
            let kind = match table.kind.as_str() {
                "text" => SettingKind::Text,
                "path" => SettingKind::Path,
                _ => SettingKind::Bool,
            };
            (kind, false)
        }
        "secret" => {
            no_bounds()?;
            no_options()?;
            (SettingKind::Text, true)
        }
        "number" => {
            no_options()?;
            let kind = SettingKind::Number {
                min: bound(key, "min", table.min.as_ref(), toml_number)?,
                max: bound(key, "max", table.max.as_ref(), toml_number)?,
            };
            (kind, false)
        }
        "duration_secs" => {
            no_options()?;
            let kind = SettingKind::DurationSecs {
                min: bound(key, "min", table.min.as_ref(), toml_seconds)?,
                max: bound(key, "max", table.max.as_ref(), toml_seconds)?,
            };
            (kind, false)
        }
        "choice" => {
            no_bounds()?;
            let options = table
                .options
                .iter()
                .map(|option| SettingOption {
                    value: option.value.clone(),
                    label: option.label.clone(),
                })
                .collect();
            (SettingKind::Choice(options), false)
        }
        other => {
            return Err(format!(
                "setting {key}: unknown kind {other:?}; use text, path, choice, bool, number, duration_secs or secret"
            ));
        }
    };
    let default = table
        .default
        .as_ref()
        .map(|value| default_value(key, &kind, secret, value))
        .transpose()?;
    let def = SettingDef {
        key: key.clone(),
        label: table.label.clone(),
        description: table.description.clone(),
        group: table.group.clone(),
        kind,
        default,
        secret,
    };
    def.check().map_err(|error| error.to_string())?;
    Ok(def)
}

fn bound<T>(
    key: &str,
    name: &str,
    value: Option<&toml::Value>,
    read: fn(&toml::Value) -> Option<T>,
) -> Result<Option<T>, String> {
    value
        .map(|value| read(value).ok_or_else(|| format!("setting {key}: {name} has the wrong type")))
        .transpose()
}

fn toml_number(value: &toml::Value) -> Option<f64> {
    value
        .as_float()
        .or_else(|| value.as_integer().map(|i| i as f64))
}

fn toml_seconds(value: &toml::Value) -> Option<u64> {
    value.as_integer().and_then(|i| u64::try_from(i).ok())
}

fn default_value(
    key: &str,
    kind: &SettingKind,
    secret: bool,
    value: &toml::Value,
) -> Result<SettingValue, String> {
    let mismatch = || format!("setting {key}: default has the wrong type for its kind");
    if secret {
        return value
            .as_str()
            .map(|text| SettingValue::Text(text.to_string()))
            .ok_or_else(mismatch);
    }
    match kind {
        SettingKind::Text | SettingKind::Path | SettingKind::Choice(_) => value
            .as_str()
            .map(|text| SettingValue::Text(text.to_string()))
            .ok_or_else(mismatch),
        SettingKind::Bool => value.as_bool().map(SettingValue::Bool).ok_or_else(mismatch),
        SettingKind::Number { .. } => toml_number(value)
            .map(SettingValue::Number)
            .ok_or_else(mismatch),
        SettingKind::DurationSecs { .. } => toml_seconds(value)
            .map(SettingValue::DurationSecs)
            .ok_or_else(mismatch),
    }
}

pub(crate) fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

fn default_library_file(id: &str) -> String {
    let stem = format!("lib{}", id.replace('-', "_"));
    if cfg!(target_os = "macos") {
        format!("{stem}.dylib")
    } else {
        format!("{stem}.so")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
id = "toy"
name = "Toy"
version = "0.1.0"
schema_version = 1
api_version = 4
authors = ["Esteban"]

[providers.toy]
name = "Toy cloud"

[[capabilities]]
kind = "network:http"
hosts = ["api.example.com"]

[[capabilities]]
kind = "secrets"
"#;

    #[test]
    fn parses_providers_and_capabilities() {
        let manifest = Manifest::parse(MANIFEST).unwrap();
        assert_eq!(manifest.id, "toy");
        assert_eq!(manifest.providers["toy"].name, "Toy cloud");
        assert_eq!(
            manifest.capabilities,
            vec![
                Capability::NetworkHttp {
                    hosts: vec!["api.example.com".into()]
                },
                Capability::Secrets,
            ]
        );
    }

    #[test]
    fn unknown_capability_kind_is_refused() {
        let text = MANIFEST.replace("kind = \"secrets\"", "kind = \"root:everything\"");
        assert!(Manifest::parse(&text).is_err());
    }

    #[test]
    fn api_mismatch_asks_for_rebuild() {
        let manifest = Manifest::parse(MANIFEST).unwrap();
        let message = manifest.check_api(1, "0.3.0").unwrap_err();
        assert!(
            message.contains("Rebuild toy for CloudDeck 0.3.0"),
            "{message}"
        );
    }

    #[test]
    fn library_path_stays_inside_extension_directory() {
        let manifest = Manifest::parse(&MANIFEST.replace(
            "[providers.toy]",
            "library = \"../escape.so\"\n[providers.toy]",
        ))
        .unwrap();
        let dir = tempfile::tempdir().unwrap();
        assert!(manifest.library_path(dir.path()).is_err());
    }

    #[test]
    fn default_library_name_follows_id() {
        let manifest = Manifest::parse(MANIFEST).unwrap();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(default_library_file("toy")), b"").unwrap();
        let path = manifest.library_path(dir.path()).unwrap();
        assert!(path.ends_with(default_library_file("toy")));
    }

    #[test]
    fn rejects_ids_that_are_not_plain_names() {
        let text = MANIFEST.replace("id = \"toy\"", "id = \"../toy\"");
        assert!(Manifest::parse(&text).is_err());
    }

    #[test]
    fn parses_settings_into_definitions() {
        let text = format!(
            "{MANIFEST}\n[[settings]]\nkey = \"display_name\"\nlabel = \"Display name\"\nkind = \"text\"\ndefault = \"Default\"\n"
        );
        let manifest = Manifest::parse(&text).unwrap();
        assert_eq!(manifest.settings.len(), 1);
        assert_eq!(manifest.settings[0].key, "display_name");
        assert_eq!(
            manifest.settings[0].default,
            Some(SettingValue::Text("Default".into()))
        );
    }

    #[test]
    fn refuses_invalid_settings() {
        let setting = |body: &str| {
            Manifest::parse(&format!(
                "{MANIFEST}\n[[settings]]\nkey = \"k\"\nlabel = \"K\"\n{body}\n"
            ))
        };
        assert!(setting("kind = \"bogus\"").is_err());
        assert!(setting("kind = \"text\"\nmin = 1").is_err());
        assert!(setting("kind = \"text\"\ndefault = 3").is_err());

        let duplicate = format!(
            "{MANIFEST}\n[[settings]]\nkey = \"k\"\nlabel = \"K\"\nkind = \"text\"\n\n[[settings]]\nkey = \"k\"\nlabel = \"K\"\nkind = \"text\"\n"
        );
        assert!(Manifest::parse(&duplicate).is_err());
    }

    #[test]
    fn description_and_platforms_default_when_absent() {
        let manifest = Manifest::parse(MANIFEST).unwrap();
        assert_eq!(manifest.description, "");
        assert!(manifest.platforms.is_empty());

        let text = MANIFEST.replace(
            "authors = [\"Esteban\"]",
            "authors = [\"Esteban\"]\ndescription = \"Lists queues\"\nplatforms = [\"macos\", \"linux\"]",
        );
        let manifest = Manifest::parse(&text).unwrap();
        assert_eq!(manifest.description, "Lists queues");
        assert_eq!(manifest.platforms, ["macos", "linux"]);
    }
}
