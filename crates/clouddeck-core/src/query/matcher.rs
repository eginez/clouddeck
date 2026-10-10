//! Compiles a query into a predicate over instances.

use super::term::{Term, TermKind, parse};
use super::tokenize::QueryField;
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

    /// Number of terms naming an instance field or tag with a non-empty value, negated ones included.
    pub fn recognised_field_count(&self) -> usize {
        self.terms
            .iter()
            .filter(|term| match &term.kind {
                TermKind::Field { field, value, .. } => {
                    is_counted_field(*field) && !field_values(value).is_empty()
                }
                TermKind::TagShorthand { value, .. } => !field_values(value).is_empty(),
                _ => false,
            })
            .count()
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
            TermKind::Field { field, value, .. } => {
                let values = field_values(value);
                values.is_empty() || values.iter().any(|v| field_matches(*field, instance, v))
            }
            TermKind::TagShorthand { key, value, .. } => {
                let values = field_values(value);
                values.is_empty() || values.iter().any(|v| tag_value_matches(instance, key, v))
            }
        }
    }
}

// age and has are placeholders until query engine bead 7
fn is_counted_field(field: QueryField) -> bool {
    !matches!(field, QueryField::Age | QueryField::Has)
}

fn field_values(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|piece| piece.trim().to_lowercase())
        .filter(|piece| !piece.is_empty())
        .collect()
}

fn field_text(value: &Option<String>) -> String {
    value.as_deref().unwrap_or("").to_lowercase()
}

fn field_matches(field: QueryField, instance: &EC2Instance, value: &str) -> bool {
    let prefix_or_glob = |text: &Option<String>| {
        if value.contains('*') {
            glob(text.as_deref().unwrap_or(""), value)
        } else {
            field_text(text).starts_with(value)
        }
    };
    match field {
        QueryField::Name => glob(instance.name().unwrap_or(""), value),
        QueryField::Id => glob(&instance.instance_id, value),
        QueryField::State => field_text(&instance.state).starts_with(value),
        QueryField::Region => field_text(&instance.region).starts_with(value),
        QueryField::Type => prefix_or_glob(&instance.instance_type),
        QueryField::Az => prefix_or_glob(&instance.availability_zone),
        QueryField::Ip => {
            field_text(&instance.private_ip_address).contains(value)
                || field_text(&instance.public_ip_address).contains(value)
        }
        QueryField::Private => field_text(&instance.private_ip_address).contains(value),
        QueryField::Public => field_text(&instance.public_ip_address).contains(value),
        QueryField::Vpc => field_text(&instance.vpc_id).contains(value),
        QueryField::Subnet => field_text(&instance.subnet_id).contains(value),
        QueryField::Ami => field_text(&instance.image_id).contains(value),
        QueryField::Key => field_text(&instance.key_name).contains(value),
        QueryField::Sg => instance.security_groups.iter().any(|group| {
            field_text(&group.group_name).contains(value)
                || field_text(&group.group_id).contains(value)
        }),
        QueryField::Tag => tag_field_matches(instance, value),
        // filled in by query engine bead 7
        QueryField::Age | QueryField::Has => true,
    }
}

/// Whether some tag matches `key` (as in `tag:Team`) or `key=value` (value is a substring test).
fn tag_field_matches(instance: &EC2Instance, spec: &str) -> bool {
    let (key, wanted) = match spec.split_once('=') {
        Some((key, wanted)) => (key, Some(wanted)),
        None => (spec, None),
    };
    instance.tags.iter().any(|tag| {
        tag.key.to_lowercase() == key
            && wanted.is_none_or(|wanted| tag.value.to_lowercase().contains(wanted))
    })
}

/// Whether a tag named `key` (ignoring case) has a value containing `value`.
fn tag_value_matches(instance: &EC2Instance, key: &str, value: &str) -> bool {
    let key = key.to_lowercase();
    instance
        .tags
        .iter()
        .any(|tag| tag.key.to_lowercase() == key && tag.value.to_lowercase().contains(value))
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

    #[test]
    fn state_filters() {
        assert_eq!(ids("state:stopped"), vec![2, 5]);
        assert_eq!(ids("-state:stopped"), vec![1, 3, 4]);
        assert_eq!(ids("state:running,pending"), vec![1, 3, 4]);
        assert_eq!(ids("state:run"), vec![1, 4]);
        assert_eq!(ids("STATE:Running"), vec![1, 4]);
        assert_eq!(ids("state:"), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn type_filters() {
        assert_eq!(ids("type:t3"), vec![1, 3, 4]);
        assert_eq!(ids("type:t3.*"), vec![1, 3, 4]);
        assert_eq!(ids("type:*large"), vec![2, 5]);
        assert_eq!(ids("type:m6i.large"), vec![2]);
    }

    #[test]
    fn az_and_region_filters() {
        assert_eq!(ids("az:us-west-2"), vec![3, 4]);
        assert_eq!(ids("zone:us-east-1b"), vec![2]);
        assert_eq!(ids("az:*b"), vec![2, 5]);
        assert_eq!(ids("region:eu"), vec![5]);
        assert_eq!(ids("region:us"), vec![1, 2, 3, 4]);
    }

    #[test]
    fn ip_filters() {
        assert_eq!(ids("ip:54.8"), vec![1, 3]);
        assert_eq!(ids("private:10.1."), vec![3, 4]);
        assert_eq!(ids("public:10.0"), Vec::<u8>::new());
    }

    #[test]
    fn network_and_key_filters() {
        assert_eq!(ids("vpc:vpc-0aaa"), vec![1]);
        assert_eq!(ids("subnet:0bbb"), vec![1]);
        assert_eq!(ids("ami:0ddd"), vec![1]);
        assert_eq!(ids("key:ops"), vec![1, 4]);
        assert_eq!(ids("key:rsa"), vec![4]);
        assert_eq!(ids("sg:bastion"), vec![4]);
        assert_eq!(ids("sg:sg-0ccc"), vec![1]);
        assert_eq!(ids("sg:-sg"), vec![1, 2, 4]);
    }

    #[test]
    fn name_and_id_filters() {
        assert_eq!(ids("name:api*"), vec![1]);
        assert_eq!(ids("name:*01"), vec![1, 5]);
        assert_eq!(ids("name:web"), vec![3]);
        assert_eq!(ids("id:*0003"), vec![3]);
        assert_eq!(ids("id:0000000000000002"), vec![2]);
    }

    #[test]
    fn field_filters_combine_with_words() {
        assert_eq!(ids("web state:pending"), vec![3]);
    }

    #[test]
    fn recognised_field_count_cases() {
        let fleet = fleet();
        let count = |query: &str| {
            compile(query, &known_tag_keys(&fleet), now()).recognised_field_count()
        };
        assert_eq!(count("state:stopped type:t3 web"), 2);
        assert_eq!(count("-state:stopped"), 1);
        assert_eq!(count("state:"), 0);
        assert_eq!(count("colour:x"), 0);
    }

    #[test]
    fn tag_shorthand_filters() {
        assert_eq!(ids("owner:esteban"), vec![1, 5]);
        assert_eq!(ids("Owner:ESTEBAN"), vec![1, 5]);
        assert_eq!(ids("team:pay,plat"), vec![1, 2]);
        assert_eq!(ids("-owner:esteban"), vec![2, 3, 4]);
        assert_eq!(ids("owner:"), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn tag_field_filters() {
        assert_eq!(ids("tag:CostCenter"), vec![2]);
        assert_eq!(ids("tag:costcenter"), vec![2]);
        assert_eq!(ids("tag:team=data"), vec![5]);
        assert_eq!(ids("tag:team=pay,team=plat"), vec![1, 2]);
        assert_eq!(ids("tag:owner=e"), vec![1, 5]);
        assert_eq!(ids("tag:Nope"), Vec::<u8>::new());
        assert_eq!(ids("tag:"), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn tag_and_state_filters_combine() {
        assert_eq!(ids("state:running team:platform"), vec![1]);
    }

    #[test]
    fn recognised_field_count_counts_tags() {
        let fleet = fleet();
        let count =
            |query: &str| compile(query, &known_tag_keys(&fleet), now()).recognised_field_count();
        assert_eq!(count("owner:esteban tag:Team web"), 2);
        assert_eq!(count("owner:"), 0);
    }
}
