//! Counts the State and Type facets shown in the sidebar, ignoring each facet's own filter.

use super::edit::query_without_field;
use super::matcher::{SearchIndex, compile};
use super::tokenize::QueryField;
use crate::instance::Instance;
use std::collections::BTreeMap;
use std::time::SystemTime;

/// Match counts for the State facets.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StateFacetCounts {
    /// Instances matching the query without its State terms, in any state.
    pub all: usize,
    /// Matching instances in the `running` state.
    pub running: usize,
    /// Matching instances in the `stopped` state.
    pub stopped: usize,
    /// Matching instances in the `pending` state.
    pub pending: usize,
}

/// Counts each State facet over `instances`, ignoring any State terms in `query`.
pub fn state_facet_counts(
    instances: &[Instance],
    query: &str,
    known_tag_keys: &[String],
    now: SystemTime,
) -> StateFacetCounts {
    state_facet_counts_indexed(
        instances,
        &SearchIndex::new(instances),
        query,
        known_tag_keys,
        now,
    )
}

/// Like [`state_facet_counts`], matching through `index` built from `instances`.
pub fn state_facet_counts_indexed(
    instances: &[Instance],
    index: &SearchIndex,
    query: &str,
    known_tag_keys: &[String],
    now: SystemTime,
) -> StateFacetCounts {
    let q = query_without_field(query, QueryField::State, known_tag_keys);
    let compiled = compile(&q, known_tag_keys, now);
    let mut counts = StateFacetCounts::default();
    for instance in index.matching(&compiled, instances) {
        counts.all += 1;
        counts.running += has_state(instance, "running") as usize;
        counts.stopped += has_state(instance, "stopped") as usize;
        counts.pending += has_state(instance, "pending") as usize;
    }
    counts
}

/// Counts each distinct instance type over `instances`, ignoring any Type terms in `query`.
pub fn type_facet_counts(
    instances: &[Instance],
    query: &str,
    known_tag_keys: &[String],
    now: SystemTime,
) -> Vec<(String, usize)> {
    type_facet_counts_indexed(
        instances,
        &SearchIndex::new(instances),
        query,
        known_tag_keys,
        now,
    )
}

/// Like [`type_facet_counts`], matching through `index` built from `instances`.
pub fn type_facet_counts_indexed(
    instances: &[Instance],
    index: &SearchIndex,
    query: &str,
    known_tag_keys: &[String],
    now: SystemTime,
) -> Vec<(String, usize)> {
    let mut types: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for instance_type in instances.iter().filter_map(|i| i.instance_type.as_deref()) {
        if !instance_type.is_empty() {
            types
                .entry(instance_type.to_lowercase())
                .or_insert_with(|| (instance_type.to_string(), 0));
        }
    }

    let q = query_without_field(query, QueryField::Type, known_tag_keys);
    let compiled = compile(&q, known_tag_keys, now);
    for instance in index.matching(&compiled, instances) {
        if let Some((_, count)) = instance
            .instance_type
            .as_deref()
            .and_then(|t| types.get_mut(&t.to_lowercase()))
        {
            *count += 1;
        }
    }

    types.into_values().collect()
}

/// Keeps the `n` most frequent type counts, then returns them sorted by name.
pub fn top_types(counts: &[(String, usize)], n: usize) -> Vec<(String, usize)> {
    if counts.len() <= n {
        return counts.to_vec();
    }
    let mut ranked = counts.to_vec();
    ranked.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    ranked.truncate(n);
    ranked.sort_by_key(|(name, _)| name.to_lowercase());
    ranked
}

fn has_state(instance: &Instance, state: &str) -> bool {
    instance.state.raw.eq_ignore_ascii_case(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{
        fixtures::{fleet_instances, now},
        known_tag_keys,
    };

    fn types(pairs: &[(&str, usize)]) -> Vec<(String, usize)> {
        pairs.iter().map(|(n, c)| (n.to_string(), *c)).collect()
    }

    #[test]
    fn state_counts_follow_other_filters() {
        let fleet = fleet_instances();
        let k = known_tag_keys(&fleet);
        let count = |query: &str| state_facet_counts(&fleet, query, &k, now());

        assert_eq!(
            count(""),
            StateFacetCounts {
                all: 5,
                running: 2,
                stopped: 2,
                pending: 1
            }
        );
        assert_eq!(
            count("type:t3"),
            StateFacetCounts {
                all: 3,
                running: 2,
                stopped: 0,
                pending: 1
            }
        );
        assert_eq!(
            count("state:stopped type:t3"),
            StateFacetCounts {
                all: 3,
                running: 2,
                stopped: 0,
                pending: 1
            }
        );
        assert_eq!(
            count("owner:esteban"),
            StateFacetCounts {
                all: 2,
                running: 1,
                stopped: 1,
                pending: 0
            }
        );
    }

    #[test]
    fn state_counts_of_empty_slice_are_zero() {
        let k = known_tag_keys(&fleet_instances());
        assert_eq!(
            state_facet_counts(&[], "", &k, now()),
            StateFacetCounts::default()
        );
    }

    #[test]
    fn type_counts_ignore_own_type_filter() {
        let fleet = fleet_instances();
        let k = known_tag_keys(&fleet);
        let count = |query: &str| type_facet_counts(&fleet, query, &k, now());

        assert_eq!(
            count("state:stopped"),
            types(&[
                ("g5.xlarge", 1),
                ("m6i.large", 1),
                ("t3.medium", 0),
                ("t3.micro", 0),
                ("t3.small", 0),
            ])
        );
        assert_eq!(
            count("type:t3.small"),
            types(&[
                ("g5.xlarge", 1),
                ("m6i.large", 1),
                ("t3.medium", 1),
                ("t3.micro", 1),
                ("t3.small", 1),
            ])
        );
        assert_eq!(
            count(""),
            types(&[
                ("g5.xlarge", 1),
                ("m6i.large", 1),
                ("t3.medium", 1),
                ("t3.micro", 1),
                ("t3.small", 1),
            ])
        );
    }

    #[test]
    fn type_counts_of_empty_slice_are_empty() {
        let k = known_tag_keys(&fleet_instances());
        assert!(type_facet_counts(&[], "", &k, now()).is_empty());
    }

    #[test]
    fn top_types_keeps_highest_counts_sorted_by_name() {
        let fleet = fleet_instances();
        let k = known_tag_keys(&fleet);
        let running = type_facet_counts(&fleet, "state:running", &k, now());
        let all = type_facet_counts(&fleet, "", &k, now());

        assert_eq!(
            top_types(&running, 2),
            types(&[("t3.medium", 1), ("t3.micro", 1)])
        );
        assert_eq!(
            top_types(&all, 2),
            types(&[("g5.xlarge", 1), ("m6i.large", 1)])
        );
        assert_eq!(top_types(&all, 10), all);
    }
}
