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
}
