use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use clouddeck_provider_api::{SettingDef, SettingError, SettingKind, SettingValue};

use crate::settings::Settings;

const SECRET_UNAVAILABLE: &str = "secret storage is not available in this build";

/// An extension whose manifest declares settings, shown as one Settings category.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsCategory {
    pub extension: String,
    pub name: String,
    pub settings: Vec<SettingDef>,
}

/// Categories with at least one setting matching `query`, keeping only the matching settings.
pub fn search_provider_settings(
    categories: &[SettingsCategory],
    query: &str,
) -> Vec<SettingsCategory> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    categories
        .iter()
        .filter_map(|category| {
            let settings: Vec<SettingDef> = category
                .settings
                .iter()
                .filter(|def| {
                    [
                        Some(category.name.as_str()),
                        Some(def.label.as_str()),
                        def.description.as_deref(),
                        def.group.as_deref(),
                    ]
                    .into_iter()
                    .flatten()
                    .any(|text| text.to_lowercase().contains(&query))
                })
                .cloned()
                .collect();
            (!settings.is_empty()).then(|| SettingsCategory {
                settings,
                ..category.clone()
            })
        })
        .collect()
}

/// Values of extension-declared settings, kept in the host settings file under `providers.<id>.<key>`.
/// Each extension sees only its own values, and only for keys it declared.
pub struct ProviderSettings {
    path: PathBuf,
    declared: Mutex<BTreeMap<String, Vec<SettingDef>>>,
    /// Read from disk on first use and dropped after each write. Held across writes so a
    /// reader cannot cache the file contents from before a write.
    values: Mutex<Option<BTreeMap<String, BTreeMap<String, SettingValue>>>>,
}

impl ProviderSettings {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            declared: Mutex::new(BTreeMap::new()),
            values: Mutex::new(None),
        }
    }

    pub fn declare(&self, extension: &str, settings: Vec<SettingDef>) {
        lock(&self.declared).insert(extension.to_string(), settings);
    }

    pub fn declared(&self, extension: &str) -> Vec<SettingDef> {
        lock(&self.declared)
            .get(extension)
            .cloned()
            .unwrap_or_default()
    }

    /// The stored value, else the declared default. `None` for undeclared keys and for secrets.
    pub fn get(&self, extension: &str, key: &str) -> Option<SettingValue> {
        let def = self.definition(extension, key)?;
        if def.secret {
            return None;
        }
        self.stored(extension, &def).or(def.default)
    }

    /// The value as the extension receives it from `Host::setting`; a folder's leading `~/` is expanded.
    pub fn host_text(&self, extension: &str, key: &str) -> Option<String> {
        let is_folder = self
            .definition(extension, key)
            .is_some_and(|def| def.kind == SettingKind::Folder);
        let text = self.get(extension, key)?.host_text();
        Some(match text.strip_prefix("~/") {
            Some(rest) if is_folder => expand_home(rest),
            _ => text,
        })
    }

    pub fn set(&self, extension: &str, key: &str, value: SettingValue) -> Result<(), SettingError> {
        let def = self.writable(extension, key)?;
        def.validate(&value)?;
        self.write(extension, |values| {
            values.insert(key.to_string(), value);
        })
    }

    pub fn reset(&self, extension: &str, key: &str) -> Result<(), SettingError> {
        self.writable(extension, key)?;
        self.write(extension, |values| {
            values.remove(key);
        })
    }

    pub fn reset_all(&self, extension: &str) -> Result<(), SettingError> {
        self.write(extension, BTreeMap::clear)
    }

    /// Whether the stored value differs from the default. Secrets are never reported as changed.
    pub fn is_changed(&self, extension: &str, key: &str) -> bool {
        let Some(def) = self.definition(extension, key) else {
            return false;
        };
        if def.secret {
            return false;
        }
        self.stored(extension, &def)
            .is_some_and(|value| Some(&value) != def.default.as_ref())
    }

    fn definition(&self, extension: &str, key: &str) -> Option<SettingDef> {
        lock(&self.declared)
            .get(extension)?
            .iter()
            .find(|def| def.key == key)
            .cloned()
    }

    fn writable(&self, extension: &str, key: &str) -> Result<SettingDef, SettingError> {
        let def = self
            .definition(extension, key)
            .ok_or_else(|| SettingError(format!("{extension} declares no setting {key:?}")))?;
        if def.secret {
            return Err(SettingError(SECRET_UNAVAILABLE.to_string()));
        }
        Ok(def)
    }

    fn stored(&self, extension: &str, def: &SettingDef) -> Option<SettingValue> {
        let mut cache = lock(&self.values);
        let values = cache.get_or_insert_with(|| Settings::load(&self.path).providers);
        values
            .get(extension)?
            .get(&def.key)
            .filter(|value| def.validate(value).is_ok())
            .cloned()
    }

    fn write(
        &self,
        extension: &str,
        change: impl FnOnce(&mut BTreeMap<String, SettingValue>),
    ) -> Result<(), SettingError> {
        let mut cache = lock(&self.values);
        let result = Settings::update_providers(&self.path, |providers| {
            let values = providers.entry(extension.to_string()).or_default();
            change(values);
            if values.is_empty() {
                providers.remove(extension);
            }
        });
        *cache = None;
        result.map_err(|error| SettingError(format!("cannot save settings: {error}")))
    }
}

fn expand_home(rest: &str) -> String {
    match std::env::var_os("HOME").filter(|home| !home.is_empty()) {
        Some(home) => PathBuf::from(home)
            .join(rest.trim_start_matches('/'))
            .display()
            .to_string(),
        None => format!("~/{rest}"),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clouddeck_provider_api::{SettingKind, SettingOption};

    use super::*;

    fn text(key: &str, default: Option<&str>) -> SettingDef {
        SettingDef {
            key: key.into(),
            label: key.into(),
            description: None,
            group: None,
            kind: SettingKind::Text,
            default: default.map(|value| SettingValue::Text(value.into())),
            secret: false,
        }
    }

    fn text_value(value: &str) -> SettingValue {
        SettingValue::Text(value.into())
    }

    #[test]
    fn stored_value_persists_under_providers_and_overrides_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = ProviderSettings::new(path.clone());
        settings.declare("toy", vec![text("display_name", Some("Default"))]);

        assert_eq!(
            settings.get("toy", "display_name"),
            Some(text_value("Default"))
        );
        settings
            .set("toy", "display_name", text_value("Hi"))
            .unwrap();
        assert_eq!(
            settings.host_text("toy", "display_name").as_deref(),
            Some("Hi")
        );
        assert!(settings.is_changed("toy", "display_name"));

        let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(json["providers"]["toy"]["display_name"]["text"], "Hi");
    }

    #[test]
    fn folder_host_text_expands_a_home_relative_path_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let settings = ProviderSettings::new(dir.path().join("settings.json"));
        let folder = SettingDef {
            kind: SettingKind::Folder,
            ..text("config_dir", None)
        };
        settings.declare("toy", vec![folder, text("display_name", None)]);

        settings
            .set("toy", "config_dir", text_value("~/configs"))
            .unwrap();
        let expected = match std::env::var_os("HOME").filter(|home| !home.is_empty()) {
            Some(home) => std::path::PathBuf::from(home)
                .join("configs")
                .display()
                .to_string(),
            None => "~/configs".into(),
        };
        assert_eq!(
            settings.host_text("toy", "config_dir").as_deref(),
            Some(expected.as_str())
        );

        settings
            .set("toy", "config_dir", text_value("/srv/configs"))
            .unwrap();
        assert_eq!(
            settings.host_text("toy", "config_dir").as_deref(),
            Some("/srv/configs")
        );

        settings
            .set("toy", "display_name", text_value("~/Notes"))
            .unwrap();
        assert_eq!(
            settings.host_text("toy", "display_name").as_deref(),
            Some("~/Notes")
        );
    }

    #[test]
    fn reset_restores_default_and_removes_the_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = ProviderSettings::new(path.clone());
        settings.declare("toy", vec![text("display_name", Some("Default"))]);
        settings
            .set("toy", "display_name", text_value("Hi"))
            .unwrap();
        settings.reset("toy", "display_name").unwrap();

        assert_eq!(
            settings.get("toy", "display_name"),
            Some(text_value("Default"))
        );
        assert!(!settings.is_changed("toy", "display_name"));
        let json: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(json["providers"].get("toy").is_none());
    }

    #[test]
    fn stored_values_that_no_longer_validate_fall_back_to_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, br#"{"providers":{"toy":{"mode":{"text":"z"}}}}"#).unwrap();
        let settings = ProviderSettings::new(path);
        settings.declare(
            "toy",
            vec![SettingDef {
                kind: SettingKind::Choice(vec![SettingOption {
                    value: "a".into(),
                    label: "A".into(),
                }]),
                ..text("mode", Some("a"))
            }],
        );
        assert_eq!(settings.get("toy", "mode"), Some(text_value("a")));
        assert!(!settings.is_changed("toy", "mode"));
    }

    #[test]
    fn setting_the_default_is_not_a_change() {
        let dir = tempfile::tempdir().unwrap();
        let settings = ProviderSettings::new(dir.path().join("settings.json"));
        settings.declare("toy", vec![text("display_name", Some("Default"))]);
        settings
            .set("toy", "display_name", text_value("Default"))
            .unwrap();
        assert!(!settings.is_changed("toy", "display_name"));
    }

    #[test]
    fn undeclared_and_invalid_values_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = ProviderSettings::new(path.clone());
        settings.declare(
            "toy",
            vec![SettingDef {
                kind: SettingKind::Choice(vec![SettingOption {
                    value: "a".into(),
                    label: "A".into(),
                }]),
                ..text("mode", Some("a"))
            }],
        );

        assert!(settings.set("toy", "other", text_value("x")).is_err());
        assert!(settings.set("toy", "mode", text_value("z")).is_err());
        assert_eq!(settings.host_text("toy", "other"), None);
        assert!(!path.exists());
    }

    #[test]
    fn secrets_are_never_stored_or_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = ProviderSettings::new(path.clone());
        settings.declare(
            "toy",
            vec![SettingDef {
                secret: true,
                ..text("token", None)
            }],
        );

        assert!(settings.set("toy", "token", text_value("hunter2")).is_err());
        assert_eq!(settings.get("toy", "token"), None);
        assert_eq!(settings.host_text("toy", "token"), None);
        assert!(!path.exists());
    }

    #[test]
    fn extensions_only_see_their_own_declared_keys() {
        let dir = tempfile::tempdir().unwrap();
        let settings = ProviderSettings::new(dir.path().join("settings.json"));
        settings.declare("a", vec![text("region", Some("us-east-1"))]);
        settings.declare("b", vec![text("region", Some("eu-west-1"))]);

        settings
            .set("a", "region", text_value("ap-south-1"))
            .unwrap();
        assert_eq!(
            settings.host_text("a", "region").as_deref(),
            Some("ap-south-1")
        );
        assert_eq!(
            settings.host_text("b", "region").as_deref(),
            Some("eu-west-1")
        );
        assert_eq!(settings.host_text("c", "region"), None);
        assert_eq!(settings.host_text("a", "clouddeck.theme"), None);
    }

    #[test]
    fn search_matches_label_description_group_and_category_name() {
        let categories = vec![SettingsCategory {
            extension: "toy".into(),
            name: "Toy".into(),
            settings: vec![
                SettingDef {
                    label: "Display name".into(),
                    description: Some("Shown in lists".into()),
                    group: Some("Appearance".into()),
                    ..text("display_name", None)
                },
                SettingDef {
                    label: "Retries".into(),
                    ..text("retries", None)
                },
            ],
        }];
        let keys = |query: &str| -> Vec<String> {
            search_provider_settings(&categories, query)
                .into_iter()
                .flat_map(|category| category.settings)
                .map(|def| def.key)
                .collect()
        };

        assert_eq!(keys("display"), ["display_name"]);
        assert_eq!(keys("SHOWN IN"), ["display_name"]);
        assert_eq!(keys("appear"), ["display_name"]);
        assert_eq!(keys("toy"), ["display_name", "retries"]);
        assert_eq!(keys("zzz"), Vec::<String>::new());
    }

    #[test]
    fn empty_query_matches_nothing() {
        let categories = vec![SettingsCategory {
            extension: "toy".into(),
            name: "Toy".into(),
            settings: vec![text("display_name", None)],
        }];
        assert!(search_provider_settings(&categories, "  ").is_empty());
    }

    #[test]
    fn reset_all_clears_only_that_extension() {
        let dir = tempfile::tempdir().unwrap();
        let settings = ProviderSettings::new(dir.path().join("settings.json"));
        settings.declare("a", vec![text("region", None)]);
        settings.declare("b", vec![text("region", None)]);
        settings.set("a", "region", text_value("x")).unwrap();
        settings.set("b", "region", text_value("y")).unwrap();

        settings.reset_all("a").unwrap();
        assert_eq!(settings.host_text("a", "region"), None);
        assert_eq!(settings.host_text("b", "region").as_deref(), Some("y"));
    }

    #[test]
    fn reads_are_served_from_memory_until_the_next_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let settings = ProviderSettings::new(path.clone());
        settings.declare("toy", vec![text("display_name", Some("Default"))]);
        settings
            .set("toy", "display_name", text_value("Hi"))
            .unwrap();
        assert_eq!(
            settings.host_text("toy", "display_name").as_deref(),
            Some("Hi")
        );

        fs::remove_file(&path).unwrap();
        assert_eq!(
            settings.host_text("toy", "display_name").as_deref(),
            Some("Hi")
        );
        assert!(settings.is_changed("toy", "display_name"));

        settings.reset("toy", "display_name").unwrap();
        assert_eq!(
            settings.host_text("toy", "display_name").as_deref(),
            Some("Default")
        );
    }
}
