//! Pure text helpers for the bottom status bar.

use std::collections::BTreeSet;
use std::time::SystemTime;

use chrono::{DateTime, FixedOffset, Utc};

/// Label for the selected region scope.
pub fn region_scope_label(selected: &[String], available: &[String]) -> String {
    let distinct: BTreeSet<&str> = selected.iter().map(String::as_str).collect();
    if distinct.is_empty() {
        return "All regions".to_string();
    }
    if !available.is_empty()
        && available
            .iter()
            .all(|region| distinct.contains(region.as_str()))
    {
        return "All regions".to_string();
    }
    if distinct.len() == 1 {
        let only = distinct.into_iter().next().unwrap_or_default();
        return only.to_string();
    }
    format!("{} regions", distinct.len())
}

/// Instance total label, singular for exactly one.
pub fn instance_count_text(total: usize) -> String {
    if total == 1 {
        "1 instance".to_string()
    } else {
        format!("{total} instances")
    }
}

/// Count of instances passing the current query.
pub fn shown_count_text(shown: usize) -> String {
    format!("{shown} shown")
}

/// Wall-clock `HH:MM` (24-hour) for a time shifted by a UTC offset in minutes.
pub fn clock_text(at: SystemTime, utc_offset_minutes: i32) -> String {
    let offset = FixedOffset::east_opt(utc_offset_minutes * 60)
        .unwrap_or_else(|| FixedOffset::east_opt(0).expect("zero offset is valid"));
    DateTime::<Utc>::from(at)
        .with_timezone(&offset)
        .format("%H:%M")
        .to_string()
}

/// `Updated HH:MM` for the last refresh, or empty when nothing has refreshed yet.
pub fn updated_text(last_refresh: Option<SystemTime>, utc_offset_minutes: i32) -> String {
    match last_refresh {
        Some(at) => format!("Updated {}", clock_text(at, utc_offset_minutes)),
        None => String::new(),
    }
}

/// The single status item shown at the right end of the status bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusItem {
    /// Nothing is loading and no region failed.
    Ready,
    /// A profile is loading.
    Loading { profile: String },
    /// Exactly one region failed to load.
    RegionFailed { region: String },
    /// Two or more regions failed to load.
    RegionsFailed { count: usize },
    /// The fleet could not be loaded and no instances are shown.
    LoadFailed,
    /// A short confirmation after an action.
    Flash { text: String },
}

/// Picks the status item: flash, then loading, then load failure, then region failures, then ready.
pub fn status_item(
    flash: Option<&str>,
    loading: bool,
    profile: &str,
    failed_regions: &[String],
    load_failed: bool,
) -> StatusItem {
    if let Some(text) = flash.filter(|text| !text.is_empty()) {
        return StatusItem::Flash {
            text: text.to_string(),
        };
    }
    if loading {
        return StatusItem::Loading {
            profile: profile.to_string(),
        };
    }
    if load_failed {
        return StatusItem::LoadFailed;
    }
    match failed_regions {
        [] => StatusItem::Ready,
        [region] => StatusItem::RegionFailed {
            region: region.clone(),
        },
        many => StatusItem::RegionsFailed { count: many.len() },
    }
}

impl StatusItem {
    /// Text rendered in the status item.
    pub fn text(&self) -> String {
        match self {
            StatusItem::Ready => "Ready".to_string(),
            StatusItem::Loading { profile } => format!("Loading {profile}\u{2026}"),
            StatusItem::RegionFailed { region } => format!("{region} failed"),
            StatusItem::RegionsFailed { count } => format!("{count} regions failed"),
            StatusItem::LoadFailed => "Couldn\u{2019}t load".to_string(),
            StatusItem::Flash { text } => text.clone(),
        }
    }

    /// Whether the item is an amber region-failure warning.
    pub fn is_warning(&self) -> bool {
        matches!(
            self,
            StatusItem::RegionFailed { .. } | StatusItem::RegionsFailed { .. }
        )
    }

    /// Whether the item is the red load-failure segment.
    pub fn is_error(&self) -> bool {
        matches!(self, StatusItem::LoadFailed)
    }
}

/// Popover title for a single failed region.
pub fn failure_title(region: &str) -> String {
    format!("{region} couldn\u{2019}t be loaded")
}

/// Popover title for several failed regions.
pub fn several_failed_title(count: usize) -> String {
    format!("{count} regions couldn\u{2019}t be loaded")
}

/// Spells out counts one through nine; other counts use digits.
pub fn count_word(n: usize) -> String {
    match n {
        1 => "one",
        2 => "two",
        3 => "three",
        4 => "four",
        5 => "five",
        6 => "six",
        7 => "seven",
        8 => "eight",
        9 => "nine",
        _ => return n.to_string(),
    }
    .to_string()
}

/// Explains a region failure: stale instances and the state of the other regions.
pub fn failure_body(stale_instances: usize, refreshed_at: &str, other_regions: usize) -> String {
    let first = match stale_instances {
        0 => "Nothing from this region has loaded yet.".to_string(),
        1 => format!(
            "1 instance from this region is from the {refreshed_at} refresh and may be out of date."
        ),
        n => format!(
            "{n} instances from this region are from the {refreshed_at} refresh and may be out of date."
        ),
    };
    match other_regions {
        0 => first,
        1 => format!("{first} The other region is current."),
        n => format!("{first} The other {} regions are current.", count_word(n)),
    }
}

/// One `region: message` line per failure, newline-separated, for the copy action.
pub fn copy_errors_text(failures: &[(String, String)]) -> String {
    failures
        .iter()
        .map(|(region, message)| format!("{region}: {message}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn at(rfc3339: &str) -> SystemTime {
        let seconds = chrono::DateTime::parse_from_rfc3339(rfc3339)
            .unwrap()
            .timestamp();
        UNIX_EPOCH + Duration::from_secs(u64::try_from(seconds).unwrap())
    }

    fn regions(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn region_scope_label_covers_all_and_specific_scopes() {
        let ab = regions(&["a", "b"]);
        let abc = regions(&["a", "b", "c"]);
        let abcd = regions(&["a", "b", "c", "d"]);

        assert_eq!(region_scope_label(&[], &ab), "All regions");
        assert_eq!(region_scope_label(&ab, &ab), "All regions");
        assert_eq!(
            region_scope_label(&regions(&["b", "a"]), &ab),
            "All regions"
        );
        assert_eq!(region_scope_label(&regions(&["a"]), &ab), "a");
        assert_eq!(
            region_scope_label(&regions(&["a", "c"]), &abcd),
            "2 regions"
        );
        assert_eq!(region_scope_label(&regions(&["a"]), &[]), "a");
        assert_eq!(
            region_scope_label(&regions(&["a", "b", "a"]), &abc),
            "2 regions"
        );
    }

    #[test]
    fn instance_count_is_singular_only_for_one() {
        assert_eq!(instance_count_text(0), "0 instances");
        assert_eq!(instance_count_text(1), "1 instance");
        assert_eq!(instance_count_text(20), "20 instances");
    }

    #[test]
    fn shown_count_text_formats_number() {
        assert_eq!(shown_count_text(3), "3 shown");
    }

    #[test]
    fn clock_text_applies_utc_offset() {
        let noon = at("2026-10-08T12:04:00Z");
        assert_eq!(clock_text(noon, 0), "12:04");
        assert_eq!(clock_text(noon, -420), "05:04");
        assert_eq!(clock_text(at("2026-10-08T23:30:00Z"), 60), "00:30");
    }

    #[test]
    fn updated_text_is_empty_without_refresh() {
        assert_eq!(
            updated_text(Some(at("2026-10-08T12:04:00Z")), 0),
            "Updated 12:04"
        );
        assert_eq!(updated_text(None, 0), "");
    }

    #[test]
    fn status_item_prefers_flash_then_loading_then_failures() {
        let a = regions(&["a"]);
        assert_eq!(
            status_item(Some("Copied i-1"), true, "p", &a, false),
            StatusItem::Flash {
                text: "Copied i-1".to_string()
            }
        );
        assert_eq!(
            status_item(Some(""), false, "p", &[], false),
            StatusItem::Ready
        );
        assert_eq!(
            status_item(None, true, "prod-readonly", &a, false),
            StatusItem::Loading {
                profile: "prod-readonly".to_string()
            }
        );
        assert_eq!(
            status_item(None, false, "p", &regions(&["eu-central-1"]), false),
            StatusItem::RegionFailed {
                region: "eu-central-1".to_string()
            }
        );
        assert_eq!(
            status_item(None, false, "p", &regions(&["a", "b", "c"]), false),
            StatusItem::RegionsFailed { count: 3 }
        );
        assert_eq!(status_item(None, false, "p", &[], false), StatusItem::Ready);
    }

    #[test]
    fn load_failure_is_an_error_status_and_beats_region_failures() {
        let failed = regions(&["a"]);
        let item = status_item(None, false, "p", &[], true);
        assert_eq!(item, StatusItem::LoadFailed);
        assert_eq!(item.text(), "Couldn\u{2019}t load");
        assert!(item.is_error());
        assert!(!item.is_warning());
        assert_eq!(
            status_item(Some("Copied"), false, "p", &[], true),
            StatusItem::Flash {
                text: "Copied".to_string()
            }
        );
        assert_eq!(
            status_item(None, true, "p", &[], true),
            StatusItem::Loading {
                profile: "p".to_string()
            }
        );
        assert_eq!(
            status_item(None, false, "p", &failed, true),
            StatusItem::LoadFailed
        );
    }

    #[test]
    fn status_item_text_and_warning_state() {
        let loading = StatusItem::Loading {
            profile: "prod-readonly".to_string(),
        };
        assert_eq!(loading.text(), "Loading prod-readonly\u{2026}");
        assert!(loading.text().ends_with('\u{2026}'));
        assert_eq!(
            StatusItem::RegionFailed {
                region: "eu-central-1".to_string()
            }
            .text(),
            "eu-central-1 failed"
        );
        assert_eq!(
            StatusItem::RegionsFailed { count: 3 }.text(),
            "3 regions failed"
        );
        assert_eq!(StatusItem::Ready.text(), "Ready");
        assert_eq!(
            StatusItem::Flash {
                text: "Copied".to_string()
            }
            .text(),
            "Copied"
        );

        assert!(
            StatusItem::RegionFailed {
                region: "x".to_string()
            }
            .is_warning()
        );
        assert!(StatusItem::RegionsFailed { count: 2 }.is_warning());
        assert!(!loading.is_warning());
        assert!(!StatusItem::Ready.is_warning());
        assert!(
            !StatusItem::Flash {
                text: "x".to_string()
            }
            .is_warning()
        );
    }

    #[test]
    fn failure_titles_use_curly_apostrophe() {
        assert_eq!(
            failure_title("eu-central-1"),
            "eu-central-1 couldn\u{2019}t be loaded"
        );
        assert_eq!(
            several_failed_title(3),
            "3 regions couldn\u{2019}t be loaded"
        );
    }

    #[test]
    fn count_word_spells_one_through_nine() {
        assert_eq!(count_word(1), "one");
        assert_eq!(count_word(3), "three");
        assert_eq!(count_word(9), "nine");
        assert_eq!(count_word(10), "10");
        assert_eq!(count_word(0), "0");
    }

    #[test]
    fn failure_body_describes_stale_instances_and_other_regions() {
        assert_eq!(
            failure_body(4, "12:01", 3),
            "4 instances from this region are from the 12:01 refresh and may be out of date. The other three regions are current."
        );
        assert_eq!(
            failure_body(1, "12:01", 1),
            "1 instance from this region is from the 12:01 refresh and may be out of date. The other region is current."
        );
        assert_eq!(
            failure_body(0, "12:01", 0),
            "Nothing from this region has loaded yet."
        );
        assert!(failure_body(2, "09:15", 12).ends_with("The other 12 regions are current."));
    }

    #[test]
    fn copy_errors_text_joins_region_messages() {
        let failures = vec![
            ("us-west-2".to_string(), "Throttled".to_string()),
            ("eu-west-1".to_string(), "AccessDenied".to_string()),
        ];
        assert_eq!(
            copy_errors_text(&failures),
            "us-west-2: Throttled\neu-west-1: AccessDenied"
        );
        assert_eq!(copy_errors_text(&[]), "");
    }
}
