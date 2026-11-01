use crate::{FLEET_REGIONS, context_path};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// Allowed auto-refresh intervals in minutes.
pub const REFRESH_INTERVALS: [u32; 5] = [1, 2, 5, 10, 15];
/// Allowed recent-search list lengths.
pub const RECENT_SEARCH_LIMITS: [u32; 4] = [0, 5, 10, 20];
/// Allowed request timeouts in seconds.
pub const REQUEST_TIMEOUTS: [u32; 3] = [15, 30, 60];
/// Settings categories in sidebar order.
pub const SETTINGS_CATEGORIES: [&str; 8] = [
    "Appearance",
    "Fleet",
    "Table",
    "Search",
    "Keymap",
    "AWS & Privacy",
    "Advanced",
    "About",
];

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
    /// Path to the AWS config file.
    pub aws_config_file: String,
    /// Seconds before an AWS request times out.
    pub request_timeout_seconds: u32,
    /// Whether to use demo data instead of AWS.
    pub demo_mode: bool,
    /// Whether CloudDeck checks for new releases automatically.
    pub check_for_updates: bool,
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
            aws_config_file: "~/.aws/config".to_owned(),
            request_timeout_seconds: 30,
            demo_mode: false,
            check_for_updates: true,
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
            ..self
        }
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_secs(u64::from(self.clone().normalized().request_timeout_seconds))
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

/// Per-region fleet deadline, kept above the SDK request deadline so the SDK's own timeout message wins.
pub fn fleet_region_timeout(request_timeout: Duration) -> Duration {
    request_timeout + Duration::from_secs(5)
}

/// Location of `settings.json`, next to `context.json`.
pub fn settings_path() -> PathBuf {
    context_path().with_file_name("settings.json")
}

/// One user-facing setting, in display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingId {
    /// Appearance theme.
    Theme,
    /// Table row density.
    Density,
    /// Show the sidebar at launch.
    ShowSidebarOnLaunch,
    /// Open the inspector on selection.
    OpenInspectorOnSelect,
    /// Show the updated time in the status bar.
    ShowUpdatedTime,
    /// Refresh automatically.
    AutoRefresh,
    /// Minutes between automatic refreshes.
    RefreshInterval,
    /// Refresh when the window regains focus.
    RefreshOnFocus,
    /// What happens at launch.
    OnLaunch,
    /// Regions to load.
    Regions,
    /// Default table sort.
    DefaultSort,
    /// Label instances without a Name tag as unnamed.
    ShowUnnamedLabel,
    /// Recent search list length.
    RecentSearchLimit,
    /// Match tag keys and values in search.
    MatchTags,
    /// Path to the AWS config file.
    AwsConfigFile,
    /// Seconds before an AWS request times out.
    RequestTimeout,
    /// Check for new releases automatically.
    CheckForUpdates,
    /// Simulated fleet without AWS calls.
    DemoMode,
}

impl SettingId {
    /// Every setting in display order.
    pub const ALL: [SettingId; 18] = [
        Self::Theme,
        Self::Density,
        Self::ShowSidebarOnLaunch,
        Self::OpenInspectorOnSelect,
        Self::ShowUpdatedTime,
        Self::AutoRefresh,
        Self::RefreshInterval,
        Self::RefreshOnFocus,
        Self::OnLaunch,
        Self::Regions,
        Self::DefaultSort,
        Self::ShowUnnamedLabel,
        Self::RecentSearchLimit,
        Self::MatchTags,
        Self::AwsConfigFile,
        Self::RequestTimeout,
        Self::CheckForUpdates,
        Self::DemoMode,
    ];

    /// Settings category this setting is listed under.
    pub fn category(self) -> &'static str {
        match self {
            Self::Theme
            | Self::Density
            | Self::ShowSidebarOnLaunch
            | Self::OpenInspectorOnSelect
            | Self::ShowUpdatedTime => "Appearance",
            Self::AutoRefresh
            | Self::RefreshInterval
            | Self::RefreshOnFocus
            | Self::OnLaunch
            | Self::Regions => "Fleet",
            Self::DefaultSort | Self::ShowUnnamedLabel => "Table",
            Self::RecentSearchLimit | Self::MatchTags => "Search",
            Self::AwsConfigFile | Self::RequestTimeout => "AWS & Privacy",
            Self::CheckForUpdates | Self::DemoMode => "Advanced",
        }
    }

    /// Subsection heading within the category.
    pub fn group(self) -> &'static str {
        match self {
            Self::Theme | Self::Density => "Window",
            Self::ShowSidebarOnLaunch | Self::OpenInspectorOnSelect | Self::ShowUpdatedTime => {
                "Panels"
            }
            Self::AutoRefresh | Self::RefreshInterval | Self::RefreshOnFocus => "Loading",
            Self::OnLaunch | Self::Regions => "Scope",
            Self::DefaultSort | Self::ShowUnnamedLabel => "Display",
            Self::RecentSearchLimit | Self::MatchTags => "Behaviour",
            Self::AwsConfigFile | Self::RequestTimeout => "AWS",
            Self::CheckForUpdates => "Updates",
            Self::DemoMode => "Developer",
        }
    }

    /// Row label shown in the settings window.
    pub fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Density => "Table density",
            Self::ShowSidebarOnLaunch => "Show sidebar on launch",
            Self::OpenInspectorOnSelect => "Open the inspector when selecting an instance",
            Self::ShowUpdatedTime => "Show “Updated” time in the status bar",
            Self::AutoRefresh => "Refresh automatically",
            Self::RefreshInterval => "Refresh interval",
            Self::RefreshOnFocus => "Refresh when the window regains focus",
            Self::OnLaunch => "On launch",
            Self::Regions => "Regions to load",
            Self::DefaultSort => "Default sort",
            Self::ShowUnnamedLabel => "Show instances without a Name tag as “unnamed”",
            Self::RecentSearchLimit => "Remember recent searches",
            Self::MatchTags => "Match tag keys and values",
            Self::AwsConfigFile => "AWS config file",
            Self::RequestTimeout => "Request timeout",
            Self::CheckForUpdates => "Check for updates automatically",
            Self::DemoMode => "Demo mode",
        }
    }

    /// One-line explanation shown under the label.
    pub fn description(self) -> &'static str {
        match self {
            Self::Theme => "Follows macOS appearance by default.",
            Self::Density => "Row height in the instance table.",
            Self::ShowSidebarOnLaunch => "Facets, pinned searches and recent instances.",
            Self::OpenInspectorOnSelect => {
                "Turn off to keep the table full width until you ask for details (⌘⌥I)."
            }
            Self::ShowUpdatedTime => "Hidden automatically in narrow windows.",
            Self::AutoRefresh => {
                "Only while the window is active. Old rows stay visible while loading."
            }
            Self::RefreshInterval => "Data older than twice this interval is marked stale.",
            Self::RefreshOnFocus => "After more than one interval away.",
            Self::OnLaunch => "Reopen into your last profile and regions.",
            Self::Regions => "Region list the picker offers. At least one.",
            Self::DefaultSort => "Applied until you click a column header.",
            Self::ShowUnnamedLabel => "Otherwise they show their instance ID in the Name column.",
            Self::RecentSearchLimit => "Shown when you focus an empty search field.",
            Self::MatchTags => "Search also looks at name, ID, IPs, zone and type.",
            Self::AwsConfigFile => "Where profiles are read from.",
            Self::RequestTimeout => "Per region, per request.",
            Self::CheckForUpdates => {
                "Checks on launch and about once an hour while CloudDeck runs."
            }
            Self::DemoMode => "Simulated fleet, no AWS calls. A DEMO tag stays in the status bar.",
        }
    }

    /// Whether this setting differs from its default in `settings`.
    pub fn is_changed(self, settings: &Settings) -> bool {
        let mut restored = settings.clone();
        self.reset(&mut restored);
        restored != *settings
    }

    /// Restores this setting's field to its default value.
    pub fn reset(self, settings: &mut Settings) {
        let defaults = Settings::default();
        match self {
            Self::Theme => settings.theme = defaults.theme,
            Self::Density => settings.density = defaults.density,
            Self::ShowSidebarOnLaunch => {
                settings.show_sidebar_on_launch = defaults.show_sidebar_on_launch
            }
            Self::OpenInspectorOnSelect => {
                settings.open_inspector_on_select = defaults.open_inspector_on_select
            }
            Self::ShowUpdatedTime => settings.show_updated_time = defaults.show_updated_time,
            Self::AutoRefresh => settings.auto_refresh = defaults.auto_refresh,
            Self::RefreshInterval => {
                settings.refresh_interval_minutes = defaults.refresh_interval_minutes
            }
            Self::RefreshOnFocus => settings.refresh_on_focus = defaults.refresh_on_focus,
            Self::OnLaunch => settings.on_launch = defaults.on_launch,
            Self::Regions => settings.regions = defaults.regions,
            Self::DefaultSort => settings.default_sort = defaults.default_sort,
            Self::ShowUnnamedLabel => settings.show_unnamed_label = defaults.show_unnamed_label,
            Self::RecentSearchLimit => settings.recent_search_limit = defaults.recent_search_limit,
            Self::MatchTags => settings.match_tags = defaults.match_tags,
            Self::AwsConfigFile => settings.aws_config_file = defaults.aws_config_file,
            Self::RequestTimeout => {
                settings.request_timeout_seconds = defaults.request_timeout_seconds
            }
            Self::CheckForUpdates => settings.check_for_updates = defaults.check_for_updates,
            Self::DemoMode => settings.demo_mode = defaults.demo_mode,
        }
    }

    /// Whether this setting is greyed out in `settings`.
    pub fn is_disabled(self, settings: &Settings) -> bool {
        matches!(self, Self::RefreshInterval) && !settings.auto_refresh
    }
}

/// Categories containing at least one changed setting, in catalog order.
pub fn changed_categories(settings: &Settings) -> Vec<&'static str> {
    SETTINGS_CATEGORIES
        .iter()
        .copied()
        .filter(|category| {
            SettingId::ALL
                .iter()
                .any(|id| id.category() == *category && id.is_changed(settings))
        })
        .collect()
}

/// Number of settings that differ from their defaults.
pub fn changed_count(settings: &Settings) -> usize {
    SettingId::ALL
        .iter()
        .filter(|id| id.is_changed(settings))
        .count()
}

/// Keyboard shortcuts listed in the Keymap category, as (action, shortcut).
pub const KEYMAP: [(&str, &str); 10] = [
    ("Switch Profile…", "⌘P"),
    ("Switch Region…", "⌘⇧R"),
    ("Refresh", "⌘R"),
    ("Command Palette", "⌘⇧P"),
    ("Toggle Sidebar", "⌘B"),
    ("Toggle Inspector", "⌘⌥I"),
    ("Focus Search", "⌘F"),
    ("Copy Instance ID", "⌘C"),
    ("Open in Console", "⌘O"),
    ("Settings", "⌘,"),
];

/// Settings and keymap rows matching a search query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsSearchResults {
    /// Matching settings grouped by category, in catalog order.
    pub groups: Vec<(&'static str, Vec<SettingId>)>,
    /// Matching keymap rows as (action, shortcut), in keymap order.
    pub keymap_rows: Vec<(&'static str, &'static str)>,
}

/// Finds settings and keymap rows containing `query`, ignoring case and surrounding spaces.
pub fn search_settings(query: &str) -> SettingsSearchResults {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return SettingsSearchResults {
            groups: Vec::new(),
            keymap_rows: Vec::new(),
        };
    }
    let groups = SETTINGS_CATEGORIES
        .iter()
        .filter_map(|category| {
            let ids: Vec<SettingId> = SettingId::ALL
                .into_iter()
                .filter(|id| id.category() == *category)
                .filter(|id| {
                    [id.category(), id.label(), id.description()]
                        .iter()
                        .any(|text| text.to_lowercase().contains(&query))
                })
                .collect();
            (!ids.is_empty()).then_some((*category, ids))
        })
        .collect();
    let keymap_rows = KEYMAP
        .into_iter()
        .filter(|(action, shortcut)| {
            format!("{action} {shortcut}")
                .to_lowercase()
                .contains(&query)
        })
        .collect();
    SettingsSearchResults {
        groups,
        keymap_rows,
    }
}

impl SettingsSearchResults {
    /// Number of matching settings plus matching keymap rows.
    pub fn count(&self) -> usize {
        self.groups.iter().map(|(_, ids)| ids.len()).sum::<usize>() + self.keymap_rows.len()
    }

    /// Result count heading such as "1 result" or "3 results".
    pub fn title(&self) -> String {
        let count = self.count();
        if count == 1 {
            "1 result".to_owned()
        } else {
            format!("{count} results")
        }
    }

    /// Categories with results, in catalog order, including Keymap.
    pub fn visible_categories(&self) -> Vec<&'static str> {
        SETTINGS_CATEGORIES
            .iter()
            .copied()
            .filter(|category| {
                self.groups.iter().any(|(group, _)| group == category)
                    || (*category == "Keymap" && !self.keymap_rows.is_empty())
            })
            .collect()
    }
}

/// Breadcrumb shown above search results.
pub fn settings_search_breadcrumb(query: &str) -> String {
    format!("Settings › Search results for “{}”", query.trim())
}

/// Message shown when a search matches nothing.
pub fn settings_no_match_text(query: &str) -> String {
    format!("Nothing matches “{}”. Try a shorter word.", query.trim())
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
        assert_eq!(settings.aws_config_file, "~/.aws/config");
        assert_eq!(settings.request_timeout_seconds, 30);
        assert!(!settings.demo_mode);
        assert!(settings.check_for_updates);
        assert_eq!(SettingId::CheckForUpdates.category(), "Advanced");
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
            check_for_updates: false,
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
    fn load_ignores_retired_aws_cli_path_key() {
        let path = temp_file("retired-cli-path");
        std::fs::write(
            &path,
            br#"{"aws_cli_path":"/usr/local/bin/aws","theme":"dark"}"#,
        )
        .unwrap();
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
    fn load_ignores_retired_log_level_key() {
        let path = temp_file("retired-log-level");
        std::fs::write(&path, br#"{"log_level":"debug","theme":"dark"}"#).unwrap();
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
    fn request_timeout_maps_to_sdk_and_fleet_deadlines() {
        let cases = [(15, 15, 20), (30, 30, 35), (60, 60, 65)];
        for (seconds, sdk, fleet) in cases {
            let settings = Settings {
                request_timeout_seconds: seconds,
                ..Settings::default()
            };
            let request = settings.request_timeout();
            assert_eq!(request, Duration::from_secs(sdk));
            assert_eq!(fleet_region_timeout(request), Duration::from_secs(fleet));
        }
        assert_eq!(
            Settings::default().request_timeout(),
            Duration::from_secs(30)
        );
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

    #[test]
    fn catalog_has_twenty_labels_in_display_order() {
        assert_eq!(SettingId::ALL.len(), 18);
        let labels: Vec<&str> = SettingId::ALL.iter().map(|id| id.label()).collect();
        assert_eq!(
            labels,
            vec![
                "Theme",
                "Table density",
                "Show sidebar on launch",
                "Open the inspector when selecting an instance",
                "Show “Updated” time in the status bar",
                "Refresh automatically",
                "Refresh interval",
                "Refresh when the window regains focus",
                "On launch",
                "Regions to load",
                "Default sort",
                "Show instances without a Name tag as “unnamed”",
                "Remember recent searches",
                "Match tag keys and values",
                "AWS config file",
                "Request timeout",
                "Check for updates automatically",
                "Demo mode",
            ]
        );
    }

    #[test]
    fn catalog_strings_match_design() {
        assert_eq!(
            SettingId::OpenInspectorOnSelect.description(),
            "Turn off to keep the table full width until you ask for details (⌘⌥I)."
        );
        assert_eq!(
            SettingId::ShowUpdatedTime.label(),
            "Show “Updated” time in the status bar"
        );
        assert_eq!(SettingId::AwsConfigFile.category(), "AWS & Privacy");
        assert_eq!(SettingId::DemoMode.group(), "Developer");
    }

    #[test]
    fn every_setting_category_is_listed_and_skips_keymap_and_about() {
        for id in SettingId::ALL {
            assert!(SETTINGS_CATEGORIES.contains(&id.category()));
            assert_ne!(id.category(), "Keymap");
            assert_ne!(id.category(), "About");
        }
    }

    #[test]
    fn default_settings_have_no_changes() {
        let settings = Settings::default();
        assert!(SettingId::ALL.iter().all(|id| !id.is_changed(&settings)));
        assert!(changed_categories(&settings).is_empty());
        assert_eq!(changed_count(&settings), 0);
    }

    #[test]
    fn changed_settings_report_ids_categories_and_count() {
        let settings = Settings {
            theme: Theme::Dark,
            refresh_interval_minutes: 10,
            demo_mode: true,
            ..Settings::default()
        };
        let changed: Vec<SettingId> = SettingId::ALL
            .into_iter()
            .filter(|id| id.is_changed(&settings))
            .collect();
        assert_eq!(
            changed,
            vec![
                SettingId::Theme,
                SettingId::RefreshInterval,
                SettingId::DemoMode
            ]
        );
        assert_eq!(
            changed_categories(&settings),
            vec!["Appearance", "Fleet", "Advanced"]
        );
        assert_eq!(changed_count(&settings), 3);
    }

    #[test]
    fn regions_reset_restores_default_regions() {
        let mut settings = Settings {
            regions: vec!["us-east-1".into()],
            ..Settings::default()
        };
        assert!(SettingId::Regions.is_changed(&settings));
        SettingId::Regions.reset(&mut settings);
        assert_eq!(settings.regions, Settings::default().regions);
        assert_eq!(settings.regions.len(), 4);
    }

    #[test]
    fn theme_reset_returns_to_follow_system() {
        let mut settings = Settings {
            theme: Theme::Dark,
            ..Settings::default()
        };
        SettingId::Theme.reset(&mut settings);
        assert_eq!(settings.theme, Theme::FollowSystem);
        assert!(!SettingId::Theme.is_changed(&settings));
    }

    #[test]
    fn refresh_interval_is_disabled_only_without_auto_refresh() {
        let on = Settings {
            auto_refresh: true,
            ..Settings::default()
        };
        let off = Settings {
            auto_refresh: false,
            ..Settings::default()
        };
        assert!(!SettingId::RefreshInterval.is_disabled(&on));
        assert!(SettingId::RefreshInterval.is_disabled(&off));
        assert!(!SettingId::Theme.is_disabled(&on));
        assert!(!SettingId::Theme.is_disabled(&off));
    }

    #[test]
    fn search_refresh_finds_fleet_settings_and_keymap_row() {
        let results = search_settings("refresh");
        assert_eq!(
            results.groups,
            vec![(
                "Fleet",
                vec![
                    SettingId::AutoRefresh,
                    SettingId::RefreshInterval,
                    SettingId::RefreshOnFocus
                ]
            )]
        );
        assert_eq!(results.keymap_rows, vec![("Refresh", "⌘R")]);
        assert_eq!(results.count(), 4);
        assert_eq!(results.title(), "4 results");
        assert_eq!(results.visible_categories(), vec!["Fleet", "Keymap"]);
    }

    #[test]
    fn search_is_case_insensitive_on_category() {
        let results = search_settings("APPEARANCE");
        assert_eq!(
            results.groups,
            vec![(
                "Appearance",
                vec![
                    SettingId::Theme,
                    SettingId::Density,
                    SettingId::ShowSidebarOnLaunch,
                    SettingId::OpenInspectorOnSelect,
                    SettingId::ShowUpdatedTime
                ]
            )]
        );
        assert!(results.keymap_rows.is_empty());
        assert_eq!(results.title(), "5 results");
    }

    #[test]
    fn search_tag_matches_labels_and_descriptions_across_groups() {
        let results = search_settings("tag");
        assert_eq!(
            results.groups,
            vec![
                ("Table", vec![SettingId::ShowUnnamedLabel]),
                ("Search", vec![SettingId::MatchTags]),
                ("Advanced", vec![SettingId::DemoMode]),
            ]
        );
        assert!(results.keymap_rows.is_empty());
        assert_eq!(results.count(), 3);
    }

    #[test]
    fn search_shortcut_matches_keymap_only() {
        let results = search_settings("⌘,");
        assert!(results.groups.is_empty());
        assert_eq!(results.keymap_rows, vec![("Settings", "⌘,")]);
        assert_eq!(results.title(), "1 result");
        assert_eq!(results.visible_categories(), vec!["Keymap"]);
    }

    #[test]
    fn search_without_matches_or_blank_query_is_empty() {
        let none = search_settings("zzz");
        assert_eq!(none.count(), 0);
        assert_eq!(none.title(), "0 results");
        assert!(none.visible_categories().is_empty());

        for blank in ["", "   "] {
            let results = search_settings(blank);
            assert!(results.groups.is_empty());
            assert!(results.keymap_rows.is_empty());
        }
    }

    #[test]
    fn search_text_helpers_trim_query() {
        assert_eq!(
            settings_search_breadcrumb(" refresh "),
            "Settings › Search results for “refresh”"
        );
        assert_eq!(
            settings_no_match_text("zzz"),
            "Nothing matches “zzz”. Try a shorter word."
        );
    }
}
