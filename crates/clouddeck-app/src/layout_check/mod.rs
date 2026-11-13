//! Headless layout/behaviour test harness (epic clouddeck-rs-mac-ui.69). Test-only.

use std::collections::HashSet;

pub(crate) mod flows_overlays;
pub(crate) mod flows_search;
pub(crate) mod geometry;
pub(crate) mod rules;
pub(crate) mod rules_about;
pub(crate) mod rules_bar;
pub(crate) mod rules_connect;
pub(crate) mod rules_main;
pub(crate) mod rules_overlays;
pub(crate) mod rules_plugins;
pub(crate) mod rules_providers;
pub(crate) mod rules_search;
pub(crate) mod rules_settings;
pub(crate) mod rules_style;
pub(crate) mod rules_table;
pub(crate) mod runner;
pub(crate) mod scripts;
pub(crate) mod states;
pub(crate) mod text;

/// Panics listing every `kind` id that appears more than once.
pub(crate) fn assert_unique_ids<T>(items: &[T], id: fn(&T) -> &'static str, kind: &str) {
    let mut seen = HashSet::new();
    let mut duplicates: Vec<&str> = items
        .iter()
        .map(id)
        .filter(|item_id| !seen.insert(*item_id))
        .collect();
    duplicates.sort_unstable();
    duplicates.dedup();
    assert!(
        duplicates.is_empty(),
        "duplicate {kind} ids: {}",
        duplicates.join(", ")
    );
}
