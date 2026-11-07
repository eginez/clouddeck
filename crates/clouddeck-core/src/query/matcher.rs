//! Compiles a query into a predicate over instances.

use super::fields::{provider_field, provider_fields};
use super::term::{Term, TermKind, parse};
use super::tokenize::QueryField;
use crate::instance::native_id;
use crate::{AddressKind, Instance, MatchMode};
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
    // per term: an invalid or empty-valued term, which always matches
    ignored: Vec<bool>,
    now: SystemTime,
}

/// Parses `query` into a compiled predicate; `now` anchors relative terms.
pub fn compile(query: &str, known_tag_keys: &[String], now: SystemTime) -> CompiledQuery {
    let terms = parse(query, known_tag_keys);
    let problems: Vec<Option<String>> = terms
        .iter()
        .map(|term| term.error().or_else(|| invalid_message(term)))
        .collect();
    let ignored = terms
        .iter()
        .map(|term| invalid_message(term).is_some() || is_blank(term))
        .collect();
    let errors = problems
        .into_iter()
        .enumerate()
        .filter_map(|(term_index, problem)| {
            problem.map(|message| QueryError {
                term_index,
                message,
            })
        })
        .collect();
    CompiledQuery {
        terms,
        errors,
        ignored,
        now,
    }
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
            .zip(&self.ignored)
            .filter(|(term, ignored)| {
                !**ignored
                    && match &term.kind {
                        TermKind::Field { value, .. } | TermKind::TagShorthand { value, .. } => {
                            !field_values(value).is_empty()
                        }
                        _ => false,
                    }
            })
            .count()
    }

    /// Whether `instance` satisfies every term.
    pub fn matches(&self, instance: &Instance) -> bool {
        self.matches_indexed(instance, &haystack(instance))
    }

    /// Like [`matches`](Self::matches), with `search_text` standing in for the instance's haystack.
    pub fn matches_indexed(&self, instance: &Instance, search_text: &str) -> bool {
        self.matches_in(None, instance, search_text)
    }

    /// Like [`matches_indexed`](Self::matches_indexed) for rows of `provider_id`; `provider:` terms are ignored without one.
    pub fn matches_in(
        &self,
        provider_id: Option<&str>,
        instance: &Instance,
        search_text: &str,
    ) -> bool {
        self.terms.iter().zip(&self.ignored).all(|(term, ignored)| {
            *ignored
                || (provider_id.is_none() && is_provider_term(term))
                || self.term_matches(term, provider_id, instance, search_text) != term.negated
        })
    }

    /// Clones of the matching instances, in input order.
    pub fn filter_instances(&self, instances: &[Instance]) -> Vec<Instance> {
        instances
            .iter()
            .filter(|instance| self.matches(instance))
            .cloned()
            .collect()
    }

    fn term_matches(
        &self,
        term: &Term,
        provider_id: Option<&str>,
        instance: &Instance,
        search_text: &str,
    ) -> bool {
        match &term.kind {
            TermKind::Word { text } | TermKind::Unknown { text, .. } => {
                search_text.contains(&text.to_lowercase())
            }
            TermKind::Field {
                field: QueryField::Age,
                value,
                ..
            } => field_values(value)
                .first()
                .is_none_or(|age| age_matches(instance, age, self.now)),
            TermKind::Field {
                field: QueryField::ProviderFacet,
                value,
                ..
            } => provider_id.is_some_and(|id| {
                let id = id.to_lowercase();
                field_values(value).iter().any(|v| id.starts_with(v))
            }),
            TermKind::Field { field, value, .. } => field_values(value)
                .iter()
                .any(|v| field_matches(*field, instance, v)),
            TermKind::TagShorthand { key, value, .. } => field_values(value)
                .iter()
                .any(|v| tag_value_matches(instance, key, v)),
        }
    }
}

const AGE_HELP: &str = "age needs > or <, a number and h, d or w. Example: age:>30d";

/// Whether a term has nothing to search for, so it is ignored even when negated.
fn is_blank(term: &Term) -> bool {
    match &term.kind {
        TermKind::Field { value, .. } | TermKind::TagShorthand { value, .. } => {
            field_values(value).is_empty()
        }
        TermKind::Word { text } => text
            .trim_matches(|c: char| c == ',' || c.is_whitespace())
            .is_empty(),
        _ => false,
    }
}

/// Why an `age:` or `has:` term is ignored, or `None` when it is usable.
fn invalid_message(term: &Term) -> Option<String> {
    let TermKind::Field { field, value, .. } = &term.kind else {
        return None;
    };
    let values = field_values(value);
    match field {
        QueryField::Age => {
            let first = values.first()?;
            parse_age(first).is_none().then(|| AGE_HELP.to_string())
        }
        QueryField::Has => values
            .iter()
            .find(|v| !matches!(v.as_str(), "public-ip" | "tags" | "name"))
            .map(|bad| format!("has: knows public-ip, tags and name, not “{bad}”.")),
        _ => None,
    }
}

/// Parses an age such as `>30d`, `<36h` or `>2w` into its direction and limit in days.
fn parse_age(value: &str) -> Option<(char, f64)> {
    let direction = value.chars().next().filter(|c| matches!(c, '>' | '<'))?;
    let rest = &value[1..];
    let unit = rest.chars().last()?;
    let digits = &rest[..rest.len() - unit.len_utf8()];
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let count: f64 = digits.parse().ok()?;
    let days = match unit {
        'h' => count / 24.0,
        'd' => count,
        'w' => count * 7.0,
        _ => return None,
    };
    Some((direction, days))
}

/// Whether the launch time is older (`>`) or younger (`<`) than the parsed limit.
fn age_matches(instance: &Instance, value: &str, now: SystemTime) -> bool {
    let (direction, limit_days) = match parse_age(value) {
        Some(parsed) => parsed,
        None => return false,
    };
    let Some(launch) = instance.created else {
        return false;
    };
    let age_days = match now.duration_since(launch) {
        Ok(elapsed) => elapsed.as_secs_f64() / 86_400.0,
        Err(ahead) => -ahead.duration().as_secs_f64() / 86_400.0,
    };
    match direction {
        '>' => age_days > limit_days,
        _ => age_days < limit_days,
    }
}

fn has_matches(instance: &Instance, value: &str) -> bool {
    match value {
        "public-ip" => instance
            .address(AddressKind::Public)
            .is_some_and(|ip| !ip.is_empty()),
        "tags" => !instance.labels.is_empty(),
        "name" => instance.name.as_deref().is_some_and(|name| !name.is_empty()),
        _ => false,
    }
}

fn field_values(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|piece| piece.trim().to_lowercase())
        .filter(|piece| !piece.is_empty())
        .collect()
}

fn field_text(value: Option<&str>) -> String {
    value.unwrap_or("").to_lowercase()
}

fn field_matches(field: QueryField, instance: &Instance, value: &str) -> bool {
    let private = instance.address(AddressKind::Private);
    let public = instance.address(AddressKind::Public);
    let prefix_or_glob = |text: Option<&str>| {
        if value.contains('*') {
            glob(text.unwrap_or(""), value)
        } else {
            field_text(text).starts_with(value)
        }
    };
    match field {
        QueryField::Name => glob(instance.name.as_deref().unwrap_or(""), value),
        QueryField::Id => glob(native_id(instance), value),
        QueryField::State => field_text(Some(&instance.state.raw)).starts_with(value),
        QueryField::Region => field_text(instance.region.as_deref()).starts_with(value),
        QueryField::Type => prefix_or_glob(instance.instance_type.as_deref()),
        QueryField::Az => prefix_or_glob(instance.location.as_deref()),
        QueryField::Ip => {
            field_text(private).contains(value) || field_text(public).contains(value)
        }
        QueryField::Private => field_text(private).contains(value),
        QueryField::Public => field_text(public).contains(value),
        QueryField::Provider(name) => provider_field(name).is_some_and(|def| {
            def.source
                .values(instance)
                .iter()
                .any(|text| mode_matches(def.match_mode, text, value))
        }),
        QueryField::Tag => tag_field_matches(instance, value),
        QueryField::Has => has_matches(instance, value),
        QueryField::Age => unreachable!("age terms are matched in term_matches"),
        QueryField::ProviderFacet => unreachable!("provider terms are matched in term_matches"),
    }
}

fn is_provider_term(term: &Term) -> bool {
    matches!(
        &term.kind,
        TermKind::Field {
            field: QueryField::ProviderFacet,
            ..
        }
    )
}

fn mode_matches(mode: MatchMode, text: &str, value: &str) -> bool {
    let lower = text.to_lowercase();
    match mode {
        MatchMode::Contains => lower.contains(value),
        MatchMode::StartsWith => lower.starts_with(value),
        MatchMode::Glob => glob(text, value),
        MatchMode::PrefixOrGlob if value.contains('*') => glob(text, value),
        MatchMode::PrefixOrGlob => lower.starts_with(value),
    }
}

/// Whether some tag matches `key` (as in `tag:Team`) or `key=value` (value is a substring test).
fn tag_field_matches(instance: &Instance, spec: &str) -> bool {
    let (key, wanted) = match spec.split_once('=') {
        Some((key, wanted)) => (unquote(key), Some(unquote(wanted))),
        None => (unquote(spec), None),
    };
    instance.labels.iter().any(|label| {
        label.key.to_lowercase() == key
            && wanted.is_none_or(|wanted| {
                label.value.as_deref().unwrap_or("").to_lowercase().contains(wanted)
            })
    })
}

/// Drops one leading and one trailing quote left over from the tag spec.
fn unquote(text: &str) -> &str {
    let text = text.strip_prefix('"').unwrap_or(text);
    text.strip_suffix('"').unwrap_or(text)
}

/// Whether a tag named `key` (ignoring case) has a value containing `value`.
fn tag_value_matches(instance: &Instance, key: &str, value: &str) -> bool {
    let key = key.to_lowercase();
    instance.labels.iter().any(|label| {
        label.key.to_lowercase() == key
            && label
                .value
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(value)
    })
}

/// Each instance's search text, built once so repeated matches against the same instances skip rebuilding it.
pub struct SearchIndex {
    texts: Vec<String>,
}

impl SearchIndex {
    pub fn new(instances: &[Instance]) -> Self {
        Self {
            texts: instances.iter().map(haystack).collect(),
        }
    }

    /// The instances matching `query`, in input order. `instances` must be the slice this index was built from.
    pub fn matching<'a>(
        &self,
        provider_id: Option<&str>,
        query: &CompiledQuery,
        instances: &'a [Instance],
    ) -> Vec<&'a Instance> {
        instances
            .iter()
            .zip(&self.texts)
            .filter(|(instance, text)| query.matches_in(provider_id, instance, text))
            .map(|(instance, _)| instance)
            .collect()
    }
}

fn haystack(instance: &Instance) -> String {
    let field = |value: Option<&str>| value.unwrap_or("").to_string();
    let launch = instance
        .created
        .map(|time| {
            DateTime::<Utc>::from(time)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    let mut parts = vec![
        field(instance.name.as_deref()),
        native_id(instance).to_string(),
        instance.state.raw.clone(),
        field(instance.instance_type.as_deref()),
        field(instance.location.as_deref()),
        field(instance.region.as_deref()),
        field(instance.address(AddressKind::Private)),
        field(instance.address(AddressKind::Public)),
        launch,
    ];
    for def in provider_fields().iter().filter(|def| def.search) {
        parts.extend(def.source.values(instance));
    }
    parts.extend(
        instance
            .labels
            .iter()
            .map(|label| format!("{} {}", label.key, label.value.as_deref().unwrap_or(""))),
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
    use crate::instance::{Instance, Label, native_id};
    use crate::query::fixtures::{fleet_instances, now};
    use crate::query::known_tag_keys;

    fn ids(query: &str) -> Vec<u8> {
        let fleet = fleet_instances();
        compile(query, &known_tag_keys(&fleet), now())
            .filter_instances(&fleet)
            .iter()
            .map(|instance| {
                native_id(instance)
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
        let fleet = fleet_instances();
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
        let fleet = fleet_instances();
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
        let fleet = fleet_instances();
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
        let fleet = fleet_instances();
        let count =
            |query: &str| compile(query, &known_tag_keys(&fleet), now()).recognised_field_count();
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
    fn negated_empty_terms_are_ignored() {
        assert_eq!(ids("-owner:"), vec![1, 2, 3, 4, 5]);
        assert_eq!(ids("-tag:"), vec![1, 2, 3, 4, 5]);
        assert_eq!(ids("-state:"), vec![1, 2, 3, 4, 5]);
        assert_eq!(ids("-age:"), vec![1, 2, 3, 4, 5]);
        assert_eq!(ids("-has:"), vec![1, 2, 3, 4, 5]);
        assert_eq!(ids("-owner: web"), vec![1, 3]);
    }

    #[test]
    fn negated_blank_words_are_ignored() {
        assert_eq!(ids("-\"\""), vec![1, 2, 3, 4, 5]);
        assert_eq!(ids("-,"), vec![1, 2, 3, 4, 5]);
        assert_eq!(ids("-owner: ,"), vec![1, 2, 3, 4, 5]);
        assert_eq!(ids("-\",\" web"), vec![1, 3]);
    }

    #[test]
    fn blank_words_are_ignored_when_positive() {
        assert_eq!(ids("\"\""), vec![1, 2, 3, 4, 5]);
        assert_eq!(ids(","), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn tag_and_state_filters_combine() {
        assert_eq!(ids("state:running team:platform"), vec![1]);
    }

    #[test]
    fn recognised_field_count_counts_tags() {
        let fleet = fleet_instances();
        let count =
            |query: &str| compile(query, &known_tag_keys(&fleet), now()).recognised_field_count();
        assert_eq!(count("owner:esteban tag:Team web"), 2);
        assert_eq!(count("owner:"), 0);
        assert_eq!(count("-owner:"), 0);
    }

    #[test]
    fn has_filters() {
        assert_eq!(ids("has:public-ip"), vec![1, 3]);
        assert_eq!(ids("-has:public-ip"), vec![2, 4, 5]);
        assert_eq!(ids("has:name"), vec![1, 2, 3, 5]);
        assert_eq!(ids("has:tags"), vec![1, 2, 3, 5]);
        assert_eq!(ids("has:public-ip,name"), vec![1, 2, 3, 5]);
    }

    #[test]
    fn has_with_unknown_key_is_ignored_with_error() {
        let fleet = fleet_instances();
        let query = compile("has:ipv6", &known_tag_keys(&fleet), now());
        assert_eq!(ids("has:ipv6"), vec![1, 2, 3, 4, 5]);
        assert_eq!(
            query.errors(),
            &[QueryError {
                term_index: 0,
                message: "has: knows public-ip, tags and name, not “ipv6”.".into(),
            }]
        );
    }

    #[test]
    fn age_filters() {
        assert_eq!(ids("age:>30d"), vec![2]);
        assert_eq!(ids("age:<1d"), vec![3]);
        assert_eq!(ids("age:>2w"), vec![2, 4]);
        assert_eq!(ids("age:>36h"), vec![1, 2, 4]);
        assert_eq!(ids("age:<7d"), vec![3]);
        assert_eq!(ids("age:>7d"), vec![1, 2, 4]);
        assert_eq!(ids("-age:>30d"), vec![1, 3, 4, 5]);
    }

    #[test]
    fn invalid_age_is_ignored_with_error() {
        let fleet = fleet_instances();
        let help = "age needs > or <, a number and h, d or w. Example: age:>30d";
        let error = |query: &str| {
            compile(query, &known_tag_keys(&fleet), now())
                .errors()
                .to_vec()
        };
        assert_eq!(ids("age:30d"), vec![1, 2, 3, 4, 5]);
        assert_eq!(
            error("age:30d"),
            vec![QueryError {
                term_index: 0,
                message: help.into(),
            }]
        );
        assert_eq!(ids("age:>30m"), vec![1, 2, 3, 4, 5]);
        assert_eq!(error("age:>30m")[0].message, help);
        assert_eq!(ids("-age:bad"), vec![1, 2, 3, 4, 5]);
        assert_eq!(error("-age:bad").len(), 1);
    }

    #[test]
    fn recognised_field_count_skips_ignored_terms() {
        let fleet = fleet_instances();
        let query = compile(
            "state:stopped owner:esteban web colour:x age:bad -has:tags state:",
            &known_tag_keys(&fleet),
            now(),
        );
        assert_eq!(query.recognised_field_count(), 3);
        let indexes: Vec<usize> = query.errors().iter().map(|e| e.term_index).collect();
        assert_eq!(indexes, vec![3, 4]);
    }

    fn team_fleet() -> Vec<Instance> {
        [
            ("i-0000000000000011", "Data Platform"),
            ("i-0000000000000012", "platform"),
        ]
        .into_iter()
        .map(|(id, team)| Instance {
            id: id.to_string(),
            labels: vec![Label {
                key: "Team".to_string(),
                value: Some(team.to_string()),
            }],
            ..Instance::default()
        })
        .collect()
    }

    fn team_hits(query: &str) -> Vec<String> {
        let fleet = team_fleet();
        compile(query, &known_tag_keys(&fleet), now())
            .filter_instances(&fleet)
            .into_iter()
            .map(|instance| native_id(&instance).to_string())
            .collect()
    }

    #[test]
    fn tag_value_may_be_quoted_with_spaces() {
        let data = vec!["i-0000000000000011".to_string()];
        let both = vec![
            "i-0000000000000011".to_string(),
            "i-0000000000000012".to_string(),
        ];
        assert_eq!(team_hits("tag:team=\"Data Platform\""), data);
        assert_eq!(team_hits("tag:team=\"data plat\""), data);
        assert_eq!(team_hits("tag:\"Team\"=platform"), both);
        assert_eq!(team_hits("tag:team=platform"), both);
    }

    #[test]
    fn provider_term_matches_rows_by_prefix_and_negation() {
        let row = Instance::default();
        let aws = compile("provider:aws", &[], now());
        assert!(aws.matches_in(Some("aws"), &row, ""));
        assert!(compile("provider:AW", &[], now()).matches_in(Some("aws"), &row, ""));
        assert!(!compile("provider:gcp", &[], now()).matches_in(Some("aws"), &row, ""));
        assert!(!compile("-provider:aws", &[], now()).matches_in(Some("aws"), &row, ""));
        assert!(compile("-provider:gcp", &[], now()).matches_in(Some("aws"), &row, ""));
    }

    #[test]
    fn provider_term_is_ignored_without_a_provider_id() {
        let row = Instance::default();
        assert!(compile("provider:gcp", &[], now()).matches(&row));
        assert!(compile("-provider:aws", &[], now()).matches(&row));
    }
}
