use crate::{FLEET_REGIONS, context_path};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

/// Allowed auto-refresh intervals in minutes.
pub const REFRESH_INTERVALS: [u32; 5] = [1, 2, 5, 10, 15];
/// Allowed recent-search list lengths.
pub const RECENT_SEARCH_LIMITS: [u32; 4] = [0, 5, 10, 20];
/// Allowed request timeouts in seconds.
pub const REQUEST_TIMEOUTS: [u32; 3] = [15, 30, 60];

/// Appearance theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Match the system appearance.
    FollowSystem,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// Row density of lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    /// Tighter rows.
    Compact,
    /// Looser rows.
    Comfortable,
}

/// What happens when CloudDeck launches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaunchBehavior {
    /// Reopen the last profile and region.
    RememberLast,
    /// Ask which profile and region to use.
    AlwaysAsk,
}

/// Default ordering of instance lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultSort {
    /// Sort by name.
    Name,
    /// Most recently launched first.
    LaunchedNewest,
    /// Sort by instance state.
    State,
}

/// Diagnostic logging verbosity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Only errors.
    ErrorsOnly,
    /// Informational messages and errors.
    Info,
    /// Everything, including debug output.
    Debug,
}

/// User preferences that apply immediately and persist across launches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Appearance theme.
    pub theme: Theme,
    /// List row density.
    pub density: Density,
    /// Whether the sidebar is shown at launch.
    pub show_sidebar_on_launch: bool,
    /// Whether selecting an instance opens the inspector.
    pub open_inspector_on_select: bool,
    /// Whether the last-updated time is shown.
    pub show_updated_time: bool,
    /// Whether instances refresh automatically.
    pub auto_refresh: bool,
    /// Minutes between automatic refreshes.
    pub refresh_interval_minutes: u32,
    /// Whether refreshing happens when the window gains focus.
    pub refresh_on_focus: bool,
    /// What happens when CloudDeck launches.
    pub on_launch: LaunchBehavior,
    /// Regions shown in the fleet.
    pub regions: Vec<String>,
    /// Default ordering of instance lists.
    pub default_sort: DefaultSort,
    /// Whether instances without a name show an unnamed label.
    pub show_unnamed_label: bool,
    /// Number of recent searches kept.
    pub recent_search_limit: u32,
    /// Whether search matches tag values.
    pub match_tags: bool,
    /// Path to the AWS CLI, or `None` to auto-detect.
    pub aws_cli_path: Option<String>,
    /// Path to the AWS config file.
    pub aws_config_file: String,
    /// Seconds before an AWS request times out.
    pub request_timeout_seconds: u32,
    /// Whether to use demo data instead of AWS.
    pub demo_mode: bool,
    /// Diagnostic logging verbosity.
    pub log_level: LogLevel,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::FollowSystem,
            density: Density::Compact,
            show_sidebar_on_launch: true,
            open_inspector_on_select: true,
            show_updated_time: true,
            auto_refresh: true,
            refresh_interval_minutes: 5,
            refresh_on_focus: true,
            on_launch: LaunchBehavior::RememberLast,
            regions: FLEET_REGIONS
                .iter()
                .map(|region| (*region).to_owned())
                .collect(),
            default_sort: DefaultSort::Name,
            show_unnamed_label: true,
            recent_search_limit: 10,
            match_tags: true,
            aws_cli_path: None,
            aws_config_file: "~/.aws/config".to_owned(),
            request_timeout_seconds: 30,
            demo_mode: false,
            log_level: LogLevel::ErrorsOnly,
        }
    }
}

fn allowed_or(value: u32, allowed: &[u32], default: u32) -> u32 {
    if allowed.contains(&value) {
        value
    } else {
        default
    }
}

impl Settings {
    /// Replaces out-of-range values with defaults and blank values with `None` or the default regions.
    pub fn normalized(self) -> Self {
        let defaults = Self::default();
        Self {
            refresh_interval_minutes: allowed_or(
                self.refresh_interval_minutes,
                &REFRESH_INTERVALS,
                defaults.refresh_interval_minutes,
            ),
            recent_search_limit: allowed_or(
                self.recent_search_limit,
                &RECENT_SEARCH_LIMITS,
                defaults.recent_search_limit,
            ),
            request_timeout_seconds: allowed_or(
                self.request_timeout_seconds,
                &REQUEST_TIMEOUTS,
                defaults.request_timeout_seconds,
            ),
            regions: if self.regions.is_empty() {
                defaults.regions
            } else {
                self.regions
            },
            aws_cli_path: self.aws_cli_path.filter(|path| !path.trim().is_empty()),
            ..self
        }
    }

    /// Loads settings from `path`, falling back to defaults when missing or invalid.
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Self>(&bytes).ok())
            .unwrap_or_default()
            .normalized()
    }

    /// Atomically writes settings to `path`, creating parent directories as needed.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("settings.json");
        let temp = path.with_file_name(format!(
            ".{file_name}.{}.{}.tmp",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = std::fs::File::create(&temp)?;
        use std::io::Write;
        file.write_all(&serde_json::to_vec(self).map_err(std::io::Error::other)?)?;
        file.sync_all()?;
        std::fs::rename(temp, path)
    }
}

/// Location of `settings.json`, next to `context.json`.
pub fn settings_path() -> PathBuf {
    context_path().with_file_name("settings.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "clouddeck-settings-{label}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn defaults_match_design() {
        let settings = Settings::default();
        assert_eq!(settings.theme, Theme::FollowSystem);
        assert_eq!(settings.density, Density::Compact);
        assert!(settings.show_sidebar_on_launch);
        assert!(settings.open_inspector_on_select);
        assert!(settings.show_updated_time);
        assert!(settings.auto_refresh);
        assert_eq!(settings.refresh_interval_minutes, 5);
        assert!(settings.refresh_on_focus);
        assert_eq!(settings.on_launch, LaunchBehavior::RememberLast);
        assert_eq!(
            settings.regions,
            vec!["us-east-1", "us-west-2", "eu-west-1", "eu-central-1"]
        );
        assert_eq!(settings.default_sort, DefaultSort::Name);
        assert!(settings.show_unnamed_label);
        assert_eq!(settings.recent_search_limit, 10);
        assert!(settings.match_tags);
        assert_eq!(settings.aws_cli_path, None);
        assert_eq!(settings.aws_config_file, "~/.aws/config");
        assert_eq!(settings.request_timeout_seconds, 30);
        assert!(!settings.demo_mode);
        assert_eq!(settings.log_level, LogLevel::ErrorsOnly);
    }

    #[test]
    fn json_field_names_are_snake_case() {
        let value = serde_json::to_value(Settings::default()).unwrap();
        assert_eq!(value["refresh_interval_minutes"], 5);
        assert_eq!(value["theme"], "follow_system");
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_file("round-trip");
        let settings = Settings {
            theme: Theme::Dark,
            density: Density::Comfortable,
            auto_refresh: false,
            refresh_interval_minutes: 10,
            aws_cli_path: Some("/opt/homebrew/bin/aws".into()),
            ..Settings::default()
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn load_missing_or_invalid_returns_default() {
        let missing = temp_file("missing");
        assert_eq!(Settings::load(&missing), Settings::default());

        let garbage = temp_file("garbage");
        std::fs::write(&garbage, b"garbage").unwrap();
        assert_eq!(Settings::load(&garbage), Settings::default());
        let _ = std::fs::remove_file(garbage);
    }

    #[test]
    fn load_partial_json_fills_missing_fields_with_defaults() {
        let path = temp_file("partial");
        std::fs::write(&path, br#"{"theme":"dark"}"#).unwrap();
        assert_eq!(
            Settings::load(&path),
            Settings {
                theme: Theme::Dark,
                ..Settings::default()
            }
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn normalized_replaces_out_of_range_values() {
        let settings = Settings {
            refresh_interval_minutes: 7,
            recent_search_limit: 3,
            request_timeout_seconds: 45,
            ..Settings::default()
        }
        .normalized();
        assert_eq!(settings.refresh_interval_minutes, 5);
        assert_eq!(settings.recent_search_limit, 10);
        assert_eq!(settings.request_timeout_seconds, 30);
    }

    #[test]
    fn normalized_restores_default_regions_when_empty() {
        let settings = Settings {
            regions: Vec::new(),
            ..Settings::default()
        }
        .normalized();
        assert_eq!(settings.regions, Settings::default().regions);
    }

    #[test]
    fn normalized_clears_blank_cli_path() {
        let settings = Settings {
            aws_cli_path: Some("  ".into()),
            ..Settings::default()
        }
        .normalized();
        assert_eq!(settings.aws_cli_path, None);
    }

    #[test]
    fn normalized_keeps_allowed_values() {
        let settings = Settings {
            refresh_interval_minutes: 15,
            ..Settings::default()
        }
        .normalized();
        assert_eq!(settings.refresh_interval_minutes, 15);
    }

    #[test]
    fn settings_path_is_under_clouddeck() {
        assert!(settings_path().ends_with("clouddeck/settings.json"));
    }
}
