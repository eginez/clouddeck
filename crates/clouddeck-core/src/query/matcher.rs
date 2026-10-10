//! Compiles a query into a predicate over instances.

use super::term::{Term, TermKind, parse};
use crate::models::EC2Instance;
use chrono::{DateTime, Utc};
use std::time::SystemTime;

/// A term that could not be interpreted, with its position in the query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    /// Index of the offending term.
    pub term_index: usize,
    /// Explanation shown to the user.
    pub message: String,
}

/// A parsed query ready to test instances against.
pub struct CompiledQuery {
    terms: Vec<Term>,
    errors: Vec<QueryError>,
    // read by date filters, added in query engine beads 5-7
    #[allow(dead_code)]
    now: SystemTime,
}

/// Parses `query` into a compiled predicate; `now` anchors relative terms.
pub fn compile(query: &str, known_tag_keys: &[String], now: SystemTime) -> CompiledQuery {
    let terms = parse(query, known_tag_keys);
    let errors = terms
        .iter()
        .enumerate()
        .filter_map(|(term_index, term)| {
            term.error().map(|message| QueryError {
                term_index,
                message,
            })
        })
        .collect();
    CompiledQuery { terms, errors, now }
}

impl CompiledQuery {
    /// The classified terms in query order.
    pub fn terms(&self) -> &[Term] {
        &self.terms
    }

    /// Errors for unrecognised terms, in term order.
    pub fn errors(&self) -> &[QueryError] {
        &self.errors
    }

    /// Number of terms naming a known field; always 0 until field filters land.
    pub fn recognised_field_count(&self) -> usize {
        0
    }

    /// Whether `instance` satisfies every term.
    pub fn matches(&self, instance: &EC2Instance) -> bool {
        self.terms
            .iter()
            .all(|term| self.term_matches(term, instance) != term.negated)
    }

    /// Clones of the matching instances, in input order.
    pub fn filter_instances(&self, instances: &[EC2Instance]) -> Vec<EC2Instance> {
        instances
            .iter()
            .filter(|instance| self.matches(instance))
            .cloned()
            .collect()
    }

    fn term_matches(&self, term: &Term, instance: &EC2Instance) -> bool {
        match &term.kind {
            TermKind::Word { text } | TermKind::Unknown { text, .. } => {
                haystack(instance).contains(&text.to_lowercase())
            }
            // filled in by query engine beads 5-7
            TermKind::Field { .. } | TermKind::TagShorthand { .. } => true,
        }
    }
}

fn haystack(instance: &EC2Instance) -> String {
    let field = |value: &Option<String>| value.as_deref().unwrap_or("").to_string();
    let launch = instance
        .launch_time
        .map(|time| {
            DateTime::<Utc>::from(time)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    let mut parts = vec![
        instance.name().unwrap_or("").to_string(),
        instance.instance_id.clone(),
        field(&instance.state),
        field(&instance.instance_type),
        field(&instance.availability_zone),
        field(&instance.region),
        field(&instance.private_ip_address),
        field(&instance.public_ip_address),
        launch,
        field(&instance.vpc_id),
        field(&instance.subnet_id),
        field(&instance.image_id),
        field(&instance.key_name),
    ];
    parts.extend(instance.security_groups.iter().map(|group| {
        format!(
            "{} {}",
            group.group_name.as_deref().unwrap_or(""),
            group.group_id.as_deref().unwrap_or("")
        )
    }));
    parts.extend(
        instance
            .tags
            .iter()
            .map(|tag| format!("{} {}", tag.key, tag.value)),
    );
    parts.join(" ").to_lowercase()
}

/// Case-insensitive match; without `*` the pattern is a substring test, with it the whole text must match.
// called by field filters, added in query engine beads 5-7
#[allow(dead_code)]
pub(crate) fn glob(text: &str, pattern: &str) -> bool {
    let text = text.to_lowercase();
    let pattern = pattern.to_lowercase();
    if !pattern.contains('*') {
        return text.contains(&pattern);
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    let first = parts[0];
    let last = parts[parts.len() - 1];
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    for middle in &parts[1..parts.len() - 1] {
        match rest.find(middle) {
            Some(at) => rest = &rest[at + middle.len()..],
            None => return false,
        }
    }
    rest.ends_with(last)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::fixtures::{fleet, now};
    use crate::query::known_tag_keys;

    fn ids(query: &str) -> Vec<u8> {
        let fleet = fleet();
        compile(query, &known_tag_keys(&fleet), now())
            .filter_instances(&fleet)
            .iter()
            .map(|instance| {
                instance
                    .instance_id
                    .chars()
                    .last()
                    .unwrap()
                    .to_digit(10)
                    .unwrap() as u8
            })
            .collect()
    }

    #[test]
    fn empty_query_matches_everything() {
        let fleet = fleet();
        let query = compile("", &known_tag_keys(&fleet), now());
        assert_eq!(ids(""), vec![1, 2, 3, 4, 5]);
        assert!(query.errors().is_empty());
        assert_eq!(query.recognised_field_count(), 0);
    }

    #[test]
    fn plain_words_search_many_fields() {
        assert_eq!(ids("web"), vec![1, 3]);
        assert_eq!(ids("esteban"), vec![1, 5]);
        assert_eq!(ids("2026-10-01"), vec![1]);
        assert_eq!(ids("billing"), vec![2]);
        assert_eq!(ids("WEB"), vec![1, 3]);
    }

    #[test]
    fn quoted_phrases_match_literally() {
        assert_eq!(ids("\"web-frontend\""), vec![3]);
        assert_eq!(ids("\"billing worker\""), Vec::<u8>::new());
    }

    #[test]
    fn negation_excludes_matches() {
        assert_eq!(ids("-web"), vec![2, 4, 5]);
    }

    #[test]
    fn terms_are_anded() {
        assert_eq!(ids("web billing"), Vec::<u8>::new());
    }

    #[test]
    fn unknown_field_is_searched_as_text_with_error() {
        let fleet = fleet();
        let query = compile("colour:red", &known_tag_keys(&fleet), now());
        assert_eq!(ids("colour:red"), Vec::<u8>::new());
        assert_eq!(
            query.errors(),
            &[QueryError {
                term_index: 0,
                message: "No field named “colour”. Searching for the text instead.".into(),
            }]
        );
    }

    #[test]
    fn unknown_field_error_carries_term_index() {
        let fleet = fleet();
        let query = compile("web colour:x", &known_tag_keys(&fleet), now());
        assert_eq!(query.errors().len(), 1);
        assert_eq!(query.errors()[0].term_index, 1);
    }

    #[test]
    fn haystack_covers_launch_security_groups_and_tags() {
        assert_eq!(ids("2026-10-01 12:00"), vec![1]);
        assert_eq!(ids("sg-0ccc3333"), vec![1]);
        assert_eq!(ids("cc-1004"), vec![2]);
        assert_eq!(ids("billing-stack"), vec![2]);
        assert_eq!(ids("us-east-1a"), vec![1]);
    }

    #[test]
    fn glob_cases() {
        assert!(glob("t3.small", "t3.*"));
        assert!(!glob("xt3.small", "t3.*"));
        assert!(glob("abc", "*"));
        assert!(glob("", "*"));
        assert!(glob("abc", "a*c*"));
        assert!(!glob("ab", "a*c"));
        assert!(glob("T3.Small", "t3.*"));
        assert!(glob("hello", "ell"));
        assert!(!glob("aba", "ab*ba"));
        assert!(glob("abba", "ab*ba"));
    }
}
