//! Flattens the filtered instance list into the rows the table draws, with optional group headers.

use std::collections::{BTreeMap, BTreeSet};

use crate::{Column, Instance, cell_text};

/// The column the instance table groups rows by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupBy {
    #[default]
    None,
    Provider,
    State,
    Type,
    Location,
}

impl GroupBy {
    pub const ALL: [GroupBy; 5] = [
        GroupBy::None,
        GroupBy::Provider,
        GroupBy::State,
        GroupBy::Type,
        GroupBy::Location,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GroupBy::None => "None",
            GroupBy::Provider => "Provider",
            GroupBy::State => "State",
            GroupBy::Type => "Type",
            GroupBy::Location => "Location",
        }
    }

    /// The option after this one, wrapping to `None` after `Location`.
    pub fn next(self) -> GroupBy {
        let index = GroupBy::ALL.iter().position(|by| *by == self).unwrap_or(0);
        GroupBy::ALL[(index + 1) % GroupBy::ALL.len()]
    }
}

/// A group header row; `key` identifies the group in the collapsed set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupHeader {
    pub key: String,
    pub label: String,
    pub count: usize,
    pub collapsed: bool,
    pub failed: bool,
}

/// One row of the table: a group header, or the instance at this index of the input slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayRow {
    Header(GroupHeader),
    Instance(usize),
}

/// The value an instance is grouped under; an empty string means no value.
pub fn group_key(instance: &Instance, provider_id: &str, by: GroupBy) -> String {
    match by {
        GroupBy::None => String::new(),
        GroupBy::Provider => provider_id.to_string(),
        GroupBy::State if instance.state.raw.is_empty() => String::new(),
        GroupBy::State => cell_text(instance, Column::State).text,
        GroupBy::Type => instance
            .instance_type
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_default(),
        GroupBy::Location => instance
            .location
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_default(),
    }
}

/// Builds the table rows. Each input row pairs an instance with its provider id.
///
/// Groups are ordered by label, ignoring case, with the empty group last. Instances keep the
/// input order inside a group. A collapsed group emits only its header. Groups without rows
/// are omitted, except that grouping by provider keeps a header for every failed provider.
pub fn build_display_rows(
    rows: &[(String, Instance)],
    by: GroupBy,
    collapsed: &BTreeSet<String>,
    failed_providers: &[String],
    provider_names: &BTreeMap<String, String>,
) -> Vec<DisplayRow> {
    if by == GroupBy::None {
        return (0..rows.len()).map(DisplayRow::Instance).collect();
    }

    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    if by == GroupBy::Provider {
        for provider in failed_providers {
            groups.entry(provider.clone()).or_default();
        }
    }
    for (index, (provider_id, instance)) in rows.iter().enumerate() {
        groups
            .entry(group_key(instance, provider_id, by))
            .or_default()
            .push(index);
    }

    let mut ordered: Vec<(String, String, Vec<usize>)> = groups
        .into_iter()
        .map(|(key, members)| {
            let label = if by == GroupBy::Provider {
                provider_names
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| key.clone())
            } else {
                key.clone()
            };
            (key, label, members)
        })
        .collect();
    ordered.sort_by(|(key_a, label_a, _), (key_b, label_b, _)| {
        key_a
            .is_empty()
            .cmp(&key_b.is_empty())
            .then_with(|| label_a.to_lowercase().cmp(&label_b.to_lowercase()))
            .then_with(|| key_a.cmp(key_b))
    });

    let mut display = Vec::new();
    for (key, label, members) in ordered {
        let failed = by == GroupBy::Provider && failed_providers.contains(&key);
        if members.is_empty() && !failed {
            continue;
        }
        let is_collapsed = collapsed.contains(&key);
        let label = if key.is_empty() {
            "(none)".to_string()
        } else {
            label
        };
        display.push(DisplayRow::Header(GroupHeader {
            key,
            label,
            count: members.len(),
            collapsed: is_collapsed,
            failed,
        }));
        if !is_collapsed {
            display.extend(members.into_iter().map(DisplayRow::Instance));
        }
    }
    display
}

/// Input indices of the instance rows a keyboard can reach, in display order.
pub fn navigable_indices(display: &[DisplayRow]) -> Vec<usize> {
    display
        .iter()
        .filter_map(|row| match row {
            DisplayRow::Instance(index) => Some(*index),
            DisplayRow::Header(_) => None,
        })
        .collect()
}

/// The display position of the instance at `input_index`, or `None` when it is hidden.
pub fn display_index_of(display: &[DisplayRow], input_index: usize) -> Option<usize> {
    display
        .iter()
        .position(|row| *row == DisplayRow::Instance(input_index))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture_fleet as fleet;

    fn rows_of(specs: &[(&str, &str, &str, &str)]) -> Vec<(String, Instance)> {
        fleet::instances(specs)
            .into_iter()
            .map(|instance| ("aws".to_string(), instance))
            .collect()
    }

    fn headers(display: &[DisplayRow]) -> Vec<(&str, usize, bool)> {
        display
            .iter()
            .filter_map(|row| match row {
                DisplayRow::Header(h) => Some((h.label.as_str(), h.count, h.failed)),
                DisplayRow::Instance(_) => None,
            })
            .collect()
    }

    fn no_collapse() -> BTreeSet<String> {
        BTreeSet::new()
    }

    fn names() -> BTreeMap<String, String> {
        BTreeMap::from([("aws".to_string(), "AWS".to_string())])
    }

    #[test]
    fn group_by_none_gives_one_instance_row_per_input_in_order() {
        let rows = rows_of(&[
            ("i-1", "a", "running", "t3.medium"),
            ("i-2", "b", "stopped", "t3.small"),
        ]);

        let display = build_display_rows(&rows, GroupBy::None, &no_collapse(), &[], &names());

        assert_eq!(
            display,
            vec![DisplayRow::Instance(0), DisplayRow::Instance(1)]
        );
    }

    #[test]
    fn grouping_by_state_orders_labels_case_insensitively_with_none_last() {
        let mut rows = rows_of(&[
            ("i-1", "a", "stopped", "t3.medium"),
            ("i-2", "b", "running", "t3.medium"),
            ("i-3", "c", "pending", "t3.medium"),
            ("i-4", "d", "running", "t3.medium"),
        ]);
        rows[2].1.state.raw = String::new();

        let display = build_display_rows(&rows, GroupBy::State, &no_collapse(), &[], &names());

        assert_eq!(
            headers(&display),
            vec![
                ("Running", 2, false),
                ("Stopped", 1, false),
                ("(none)", 1, false)
            ]
        );
    }

    #[test]
    fn grouping_by_type_and_location_uses_the_empty_group_last() {
        let mut rows = rows_of(&[
            ("i-1", "a", "running", "t3.medium"),
            ("i-2", "b", "running", "c6i.large"),
            ("i-3", "c", "running", "t3.medium"),
        ]);
        rows[1].1.instance_type = None;
        rows[0].1.location = Some("us-west-2b".into());
        rows[1].1.location = Some("us-west-2a".into());
        rows[2].1.location = Some("us-west-2a".into());

        let by_type = build_display_rows(&rows, GroupBy::Type, &no_collapse(), &[], &names());
        assert_eq!(
            headers(&by_type),
            vec![("t3.medium", 2, false), ("(none)", 1, false)]
        );

        let by_location =
            build_display_rows(&rows, GroupBy::Location, &no_collapse(), &[], &names());
        assert_eq!(
            headers(&by_location),
            vec![("us-west-2a", 2, false), ("us-west-2b", 1, false)]
        );
    }

    #[test]
    fn instances_keep_input_order_inside_a_group() {
        let rows = rows_of(&[
            ("i-3", "c", "running", "t3.medium"),
            ("i-1", "a", "stopped", "t3.medium"),
            ("i-2", "b", "running", "t3.medium"),
        ]);

        let display = build_display_rows(&rows, GroupBy::State, &no_collapse(), &[], &names());

        assert_eq!(
            display,
            vec![
                DisplayRow::Header(GroupHeader {
                    key: "Running".into(),
                    label: "Running".into(),
                    count: 2,
                    collapsed: false,
                    failed: false,
                }),
                DisplayRow::Instance(0),
                DisplayRow::Instance(2),
                DisplayRow::Header(GroupHeader {
                    key: "Stopped".into(),
                    label: "Stopped".into(),
                    count: 1,
                    collapsed: false,
                    failed: false,
                }),
                DisplayRow::Instance(1),
            ]
        );
    }

    #[test]
    fn collapsed_group_emits_header_with_full_count_and_no_instance_rows() {
        let rows = rows_of(&[
            ("i-1", "a", "running", "t3.medium"),
            ("i-2", "b", "stopped", "t3.medium"),
            ("i-3", "c", "running", "t3.medium"),
        ]);
        let collapsed = BTreeSet::from(["Running".to_string()]);

        let display = build_display_rows(&rows, GroupBy::State, &collapsed, &[], &names());

        assert_eq!(
            display,
            vec![
                DisplayRow::Header(GroupHeader {
                    key: "Running".into(),
                    label: "Running".into(),
                    count: 2,
                    collapsed: true,
                    failed: false,
                }),
                DisplayRow::Header(GroupHeader {
                    key: "Stopped".into(),
                    label: "Stopped".into(),
                    count: 1,
                    collapsed: false,
                    failed: false,
                }),
                DisplayRow::Instance(1),
            ]
        );
    }

    #[test]
    fn navigable_indices_skip_headers_and_collapsed_rows() {
        let rows = rows_of(&[
            ("i-1", "a", "running", "t3.medium"),
            ("i-2", "b", "stopped", "t3.medium"),
            ("i-3", "c", "running", "t3.medium"),
        ]);
        let collapsed = BTreeSet::from(["Running".to_string()]);

        let display = build_display_rows(&rows, GroupBy::State, &collapsed, &[], &names());

        assert_eq!(navigable_indices(&display), vec![1]);
    }

    #[test]
    fn display_index_of_maps_instances_and_returns_none_when_collapsed() {
        let rows = rows_of(&[
            ("i-1", "a", "running", "t3.medium"),
            ("i-2", "b", "stopped", "t3.medium"),
            ("i-3", "c", "running", "t3.medium"),
        ]);
        let expanded = build_display_rows(&rows, GroupBy::State, &no_collapse(), &[], &names());
        assert_eq!(display_index_of(&expanded, 0), Some(1));
        assert_eq!(display_index_of(&expanded, 1), Some(4));
        assert_eq!(display_index_of(&expanded, 2), Some(2));

        let collapsed = BTreeSet::from(["Running".to_string()]);
        let display = build_display_rows(&rows, GroupBy::State, &collapsed, &[], &names());
        assert_eq!(display_index_of(&display, 0), None);
        assert_eq!(display_index_of(&display, 2), None);
    }

    #[test]
    fn groups_with_no_rows_are_not_emitted() {
        let rows = rows_of(&[("i-1", "a", "running", "t3.medium")]);

        let display = build_display_rows(&rows, GroupBy::State, &no_collapse(), &[], &names());

        assert_eq!(headers(&display), vec![("Running", 1, false)]);
    }

    #[test]
    fn failed_provider_gets_a_header_even_with_no_rows() {
        let rows: Vec<(String, Instance)> = Vec::new();
        let failed = vec!["aws".to_string()];

        let display =
            build_display_rows(&rows, GroupBy::Provider, &no_collapse(), &failed, &names());

        assert_eq!(headers(&display), vec![("AWS", 0, true)]);
    }

    #[test]
    fn failed_flag_is_never_set_for_state_type_or_location() {
        let rows = rows_of(&[("i-1", "a", "running", "t3.medium")]);
        let failed = vec!["aws".to_string()];

        for by in [GroupBy::State, GroupBy::Type, GroupBy::Location] {
            let display = build_display_rows(&rows, by, &no_collapse(), &failed, &names());
            assert!(
                headers(&display).iter().all(|(_, _, failed)| !failed),
                "{by:?}"
            );
        }
    }

    #[test]
    fn group_by_next_cycles_through_every_option() {
        let mut by = GroupBy::None;
        let mut seen = Vec::new();
        for _ in 0..GroupBy::ALL.len() {
            seen.push(by);
            by = by.next();
        }
        assert_eq!(seen, GroupBy::ALL.to_vec());
        assert_eq!(by, GroupBy::None);
    }
}
