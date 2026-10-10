//! Autocomplete for the word being typed in the search field, and applying a chosen suggestion.

use super::term::{TermKind, classify};
use super::tokenize::{QueryField, tokenize};
use crate::models::EC2Instance;
use std::collections::{BTreeMap, BTreeSet};

const MAX_FIELD_ITEMS: usize = 6;
const MAX_VALUE_ITEMS: usize = 8;

/// Whether a suggestion names a field or a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    /// A field name such as `state:`.
    Field,
    /// A value for a field such as `running`.
    Value,
}

/// One autocomplete entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Suggestion {
    /// Text shown in the list.
    pub label: String,
    /// Text that replaces the word being typed.
    pub insert: String,
    /// Short explanation shown beside the label.
    pub hint: Option<String>,
    /// Number of matching instances, when known.
    pub count: Option<usize>,
    /// Whether this entry is a field or a value.
    pub kind: SuggestionKind,
}

/// The autocomplete list for the end of a query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suggestions {
    /// Entries to show, best first.
    pub items: Vec<Suggestion>,
    /// Byte offset in the query where the replaced text starts.
    pub replace_start: usize,
    /// The name before a colon when it is neither a field nor a tag key.
    pub unknown_field: Option<String>,
}

/// Suggests fields and values for the word at the end of `query`.
pub fn suggest(query: &str, instances: &[EC2Instance], known_tag_keys: &[String]) -> Suggestions {
    if query.is_empty() {
        return Suggestions::default();
    }
    if query.ends_with(char::is_whitespace) {
        let items = [
            QueryField::State,
            QueryField::Type,
            QueryField::Tag,
            QueryField::Has,
            QueryField::Age,
        ]
        .into_iter()
        .map(|field| field_item("", field.name(), field_hint(field)))
        .collect();
        return Suggestions {
            items,
            replace_start: query.len(),
            unknown_field: None,
        };
    }

    let tokens = tokenize(query);
    let Some(current) = tokens.last() else {
        return Suggestions::default();
    };
    let replace_start = current.start;

    match classify(current, known_tag_keys).kind {
        TermKind::Word { .. } => {}
        TermKind::Field {
            field: QueryField::Tag,
            ..
        } => {
            return Suggestions {
                items: tag_items(&current.raw, known_tag_keys, instances),
                replace_start,
                unknown_field: None,
            };
        }
        TermKind::Field { field, .. } => {
            return Suggestions {
                items: value_items(&current.raw, field, instances),
                replace_start,
                unknown_field: None,
            };
        }
        TermKind::TagShorthand { key, .. } => {
            return Suggestions {
                items: shorthand_items(&current.raw, &key, instances),
                replace_start,
                unknown_field: None,
            };
        }
        TermKind::Unknown { name, .. } => {
            return Suggestions {
                items: Vec::new(),
                replace_start,
                unknown_field: Some(name),
            };
        }
    }

    let (neg, rest) = match current.raw.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", current.raw.as_str()),
    };
    let prefix = rest.to_lowercase();
    if prefix.is_empty() {
        return Suggestions {
            items: Vec::new(),
            replace_start,
            unknown_field: None,
        };
    }

    let candidates = QueryField::ALL
        .iter()
        .map(|field| (field.name().to_string(), field_hint(*field)))
        .chain(
            known_tag_keys
                .iter()
                .filter(|key| is_name_like(key))
                .map(|key| (key.to_lowercase(), "tag key")),
        );

    let mut items: Vec<Suggestion> = Vec::new();
    for (candidate, hint) in candidates {
        if items.len() == MAX_FIELD_ITEMS {
            break;
        }
        if !candidate.starts_with(&prefix) || candidate == prefix {
            continue;
        }
        let item = field_item(neg, &candidate, hint);
        if items.iter().any(|kept| kept.label == item.label) {
            continue;
        }
        items.push(item);
    }

    Suggestions {
        items,
        replace_start,
        unknown_field: None,
    }
}

/// Returns `query` with the suggestion at `index` inserted over its replaced text, or `None` if `index` is out of range.
pub fn apply_suggestion(query: &str, suggestions: &Suggestions, index: usize) -> Option<String> {
    let item = suggestions.items.get(index)?;
    let head = query.get(..suggestions.replace_start)?;
    let mut applied = format!("{head}{}", item.insert);
    if !item.insert.ends_with([':', '=']) {
        applied.push(' ');
    }
    Some(applied)
}

fn split_value(current: &str) -> Option<(String, &str)> {
    let (neg, body) = match current.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", current),
    };
    let (name, value) = body.split_once(':')?;
    let seg_start = value.rfind(',').map_or(0, |comma| comma + 1);
    Some((
        format!("{neg}{name}:{}", &value[..seg_start]),
        &value[seg_start..],
    ))
}

fn tag_items(
    current: &str,
    known_tag_keys: &[String],
    instances: &[EC2Instance],
) -> Vec<Suggestion> {
    let Some((base, seg)) = split_value(current) else {
        return Vec::new();
    };
    match seg.split_once('=') {
        None => known_tag_keys
            .iter()
            .filter(|key| starts_with_ignore_case(key, seg))
            .take(MAX_VALUE_ITEMS)
            .map(|key| Suggestion {
                label: key.clone(),
                insert: format!("{base}{key}="),
                hint: None,
                count: Some(
                    instances
                        .iter()
                        .filter(|i| i.tags.iter().any(|t| t.key.eq_ignore_ascii_case(key)))
                        .count(),
                ),
                kind: SuggestionKind::Value,
            })
            .collect(),
        Some((kpart, vpart)) => {
            match known_tag_keys
                .iter()
                .find(|key| key.eq_ignore_ascii_case(kpart))
            {
                Some(key) => tag_value_items(
                    instances,
                    key,
                    vpart,
                    &format!("{key}="),
                    &format!("{base}{key}="),
                ),
                None => Vec::new(),
            }
        }
    }
}

fn shorthand_items(current: &str, key: &str, instances: &[EC2Instance]) -> Vec<Suggestion> {
    let Some((base, seg)) = split_value(current) else {
        return Vec::new();
    };
    tag_value_items(instances, key, seg, "", &base)
}

fn tag_value_items(
    instances: &[EC2Instance],
    key: &str,
    filter: &str,
    label_prefix: &str,
    insert_prefix: &str,
) -> Vec<Suggestion> {
    tag_value_counts(instances, key)
        .into_iter()
        .filter(|(value, _)| starts_with_ignore_case(value, filter))
        .take(MAX_VALUE_ITEMS)
        .map(|(value, count)| Suggestion {
            label: format!("{label_prefix}{value}"),
            insert: format!("{insert_prefix}{value}"),
            hint: None,
            count: Some(count),
            kind: SuggestionKind::Value,
        })
        .collect()
}

fn tag_value_counts(instances: &[EC2Instance], key: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for instance in instances {
        let values: BTreeSet<&str> = instance
            .tags
            .iter()
            .filter(|tag| tag.key.eq_ignore_ascii_case(key) && !tag.value.is_empty())
            .map(|tag| tag.value.as_str())
            .collect();
        for value in values {
            *counts.entry(value.to_string()).or_insert(0) += 1;
        }
    }
    counts
}

fn value_items(current: &str, field: QueryField, instances: &[EC2Instance]) -> Vec<Suggestion> {
    let Some((base, seg)) = split_value(current) else {
        return Vec::new();
    };
    let seg = seg.to_ascii_lowercase();

    value_candidates(field, instances)
        .into_iter()
        .filter(|(label, _)| label.to_ascii_lowercase().starts_with(&seg))
        .take(MAX_VALUE_ITEMS)
        .map(|(label, count)| Suggestion {
            insert: format!("{base}{label}"),
            label,
            hint: None,
            count,
            kind: SuggestionKind::Value,
        })
        .collect()
}

fn value_candidates(field: QueryField, instances: &[EC2Instance]) -> Vec<(String, Option<usize>)> {
    match field {
        QueryField::State => ["running", "stopped", "pending"]
            .into_iter()
            .map(|state| {
                let count = instances
                    .iter()
                    .filter(|i| is_text(i.state.as_deref(), state))
                    .count();
                (state.to_string(), Some(count))
            })
            .collect(),
        QueryField::Type => distinct_values(instances, |i| i.instance_type.as_deref()),
        QueryField::Az => distinct_values(instances, |i| i.availability_zone.as_deref()),
        QueryField::Region => distinct_values(instances, |i| i.region.as_deref()),
        QueryField::Has => vec![
            (
                "public-ip".to_string(),
                Some(
                    instances
                        .iter()
                        .filter(|i| non_empty(i.public_ip_address.as_deref()))
                        .count(),
                ),
            ),
            (
                "tags".to_string(),
                Some(instances.iter().filter(|i| !i.tags.is_empty()).count()),
            ),
            (
                "name".to_string(),
                Some(instances.iter().filter(|i| non_empty(i.name())).count()),
            ),
        ],
        QueryField::Age => [">30d", ">7d", "<1d", "<7d"]
            .into_iter()
            .map(|label| (label.to_string(), None))
            .collect(),
        _ => Vec::new(),
    }
}

fn distinct_values(
    instances: &[EC2Instance],
    text_of: impl Fn(&EC2Instance) -> Option<&str>,
) -> Vec<(String, Option<usize>)> {
    let mut counts: BTreeMap<String, (String, usize)> = BTreeMap::new();
    for text in instances
        .iter()
        .filter_map(text_of)
        .filter(|t| !t.is_empty())
    {
        counts
            .entry(text.to_ascii_lowercase())
            .or_insert_with(|| (text.to_string(), 0))
            .1 += 1;
    }
    counts
        .into_values()
        .map(|(value, count)| (value, Some(count)))
        .collect()
}

fn is_text(value: Option<&str>, expected: &str) -> bool {
    value.is_some_and(|v| v.eq_ignore_ascii_case(expected))
}

fn non_empty(value: Option<&str>) -> bool {
    value.is_some_and(|v| !v.is_empty())
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.to_ascii_lowercase()
        .starts_with(&prefix.to_ascii_lowercase())
}

fn field_item(neg: &str, name: &str, hint: &str) -> Suggestion {
    let insert = format!("{neg}{name}:");
    Suggestion {
        label: insert.clone(),
        insert,
        hint: Some(hint.to_string()),
        count: None,
        kind: SuggestionKind::Field,
    }
}

fn is_name_like(key: &str) -> bool {
    let bytes = key.as_bytes();
    bytes.first().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
}

fn field_hint(field: QueryField) -> &'static str {
    match field {
        QueryField::Name => "name contains",
        QueryField::Id => "instance id",
        QueryField::State => "running, stopped, pending",
        QueryField::Type => "instance type, t3.* works",
        QueryField::Az => "availability zone",
        QueryField::Region => "region",
        QueryField::Ip => "private or public",
        QueryField::Private => "private IP",
        QueryField::Public => "public IP",
        QueryField::Vpc => "VPC id",
        QueryField::Subnet => "subnet id",
        QueryField::Sg => "security group",
        QueryField::Ami => "image id",
        QueryField::Key => "key pair",
        QueryField::Age => "time since launch, e.g. >30d",
        QueryField::Has => "public-ip, tags, name",
        QueryField::Tag => "tag key or key=value",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::fixtures::fleet;
    use crate::query::known_tag_keys;

    fn labels(q: &str) -> Vec<String> {
        suggest(q, &fleet(), &known_tag_keys(&fleet()))
            .items
            .into_iter()
            .map(|s| s.label)
            .collect()
    }

    fn with_keys(q: &str, keys: &[&str]) -> Suggestions {
        let keys: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        suggest(q, &fleet(), &keys)
    }

    #[test]
    fn empty_query_has_no_items() {
        let s = suggest("", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(s, Suggestions::default());
        assert!(s.items.is_empty());
        assert_eq!(s.replace_start, 0);
    }

    #[test]
    fn trailing_space_offers_leading_fields() {
        let s = suggest("web ", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(
            s.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["state:", "type:", "tag:", "has:", "age:"]
        );
        assert_eq!(
            s.items
                .iter()
                .map(|i| i.hint.clone().unwrap())
                .collect::<Vec<_>>(),
            [
                "running, stopped, pending",
                "instance type, t3.* works",
                "tag key or key=value",
                "public-ip, tags, name",
                "time since launch, e.g. >30d",
            ]
        );
        assert!(
            s.items
                .iter()
                .all(|i| i.kind == SuggestionKind::Field && i.count.is_none())
        );
        assert_eq!(s.replace_start, 4);
    }

    #[test]
    fn prefix_suggests_state() {
        assert_eq!(labels("st"), ["state:"]);
    }

    #[test]
    fn negated_prefix_keeps_dash_in_insert() {
        let s = suggest("-st", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(s.items.len(), 1);
        assert_eq!(s.items[0].label, "-state:");
        assert_eq!(s.items[0].insert, "-state:");
        assert_eq!(s.replace_start, 0);
    }

    #[test]
    fn tag_key_suggested_with_tag_hint() {
        let s = suggest("o", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(
            s.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["owner:"]
        );
        assert_eq!(s.items[0].hint.as_deref(), Some("tag key"));
    }

    #[test]
    fn letter_a_suggests_az_ami_age() {
        assert_eq!(labels("a"), ["az:", "ami:", "age:"]);
    }

    #[test]
    fn letter_p_suggests_private_public() {
        assert_eq!(labels("p"), ["private:", "public:"]);
    }

    #[test]
    fn letter_t_suggests_fields_then_tag_keys() {
        assert_eq!(labels("t"), ["type:", "tag:", "team:"]);
    }

    #[test]
    fn exact_field_name_has_no_items() {
        assert!(labels("state").is_empty());
    }

    #[test]
    fn lone_dash_has_no_items() {
        assert!(labels("-").is_empty());
    }

    #[test]
    fn prefix_after_space_replaces_only_last_word() {
        let s = suggest("web st", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(
            s.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["state:"]
        );
        assert_eq!(s.replace_start, 4);
    }

    #[test]
    fn state_value_prefix_suggests_running() {
        assert_eq!(labels("state:r"), ["running"]);
    }

    fn counted(q: &str) -> Vec<(String, Option<usize>)> {
        suggest(q, &fleet(), &known_tag_keys(&fleet()))
            .items
            .into_iter()
            .map(|s| (s.label, s.count))
            .collect()
    }

    fn pairs(items: &[(&str, usize)]) -> Vec<(String, Option<usize>)> {
        items
            .iter()
            .map(|(label, count)| (label.to_string(), Some(*count)))
            .collect()
    }

    #[test]
    fn state_values_with_counts() {
        assert_eq!(
            counted("state:"),
            pairs(&[("running", 2), ("stopped", 2), ("pending", 1)])
        );
    }

    #[test]
    fn state_value_prefix_replaces_only_value() {
        let s = suggest("web state:r", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(counted("web state:r"), pairs(&[("running", 2)]));
        assert_eq!(s.replace_start, 4);
        assert_eq!(s.items[0].insert, "state:running");
        assert_eq!(s.items[0].kind, SuggestionKind::Value);
        assert_eq!(s.items[0].hint, None);
    }

    #[test]
    fn comma_list_completes_last_segment() {
        let s = suggest("state:running,st", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(
            s.items.iter().map(|i| i.count).collect::<Vec<_>>(),
            [Some(2)]
        );
        assert_eq!(s.items[0].label, "stopped");
        assert_eq!(s.items[0].insert, "state:running,stopped");
    }

    #[test]
    fn negated_value_keeps_dash_in_insert() {
        let s = suggest("-state:s", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(s.items[0].insert, "-state:stopped");
        assert_eq!(s.items.len(), 1);
    }

    #[test]
    fn value_match_ignores_case_and_keeps_typed_name() {
        let s = suggest("STATE:R", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(counted("STATE:R"), pairs(&[("running", 2)]));
        assert_eq!(s.items[0].insert, "STATE:running");
    }

    #[test]
    fn type_values_are_distinct_sorted_and_counted() {
        assert_eq!(
            counted("type:t3"),
            pairs(&[("t3.medium", 1), ("t3.micro", 1), ("t3.small", 1)])
        );
    }

    #[test]
    fn distinct_values_keep_first_seen_text_and_merge_case() {
        let mut instances = fleet();
        instances[0].availability_zone = Some("US-EAST-1A".to_string());
        let s = suggest("zone:us-e", &instances, &known_tag_keys(&instances));
        assert_eq!(
            s.items
                .iter()
                .map(|i| (i.label.as_str(), i.count))
                .collect::<Vec<_>>(),
            [("US-EAST-1A", Some(1)), ("us-east-1b", Some(1))]
        );
        assert_eq!(s.items[0].insert, "zone:US-EAST-1A");
    }

    #[test]
    fn az_values_filtered_by_prefix() {
        assert_eq!(
            counted("zone:us-w"),
            pairs(&[("us-west-2a", 1), ("us-west-2c", 1)])
        );
        let s = suggest("zone:us-w", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(s.items[0].insert, "zone:us-west-2a");
    }

    #[test]
    fn region_values_sorted_and_counted() {
        assert_eq!(
            counted("region:"),
            pairs(&[("eu-west-1", 1), ("us-east-1", 2), ("us-west-2", 2)])
        );
    }

    #[test]
    fn has_values_counted() {
        assert_eq!(
            counted("has:"),
            pairs(&[("public-ip", 2), ("tags", 4), ("name", 4)])
        );
    }

    #[test]
    fn age_values_have_no_count() {
        assert_eq!(
            counted("age:"),
            vec![
                (">30d".to_string(), None),
                (">7d".to_string(), None),
                ("<1d".to_string(), None),
                ("<7d".to_string(), None),
            ]
        );
        assert_eq!(labels("age:>"), [">30d", ">7d"]);
    }

    #[test]
    fn name_id_ip_and_tag_values_have_no_items() {
        assert!(labels("name:").is_empty());
        assert!(labels("ip:1").is_empty());
        assert!(labels("colour:").is_empty());
    }

    #[test]
    fn tag_field_lists_keys_with_counts() {
        let s = suggest("tag:", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(s.items[0].insert, "tag:CostCenter=");
        assert_eq!(
            counted("tag:"),
            pairs(&[
                ("CostCenter", 1),
                ("Environment", 3),
                ("Owner", 4),
                ("Team", 4)
            ])
        );
    }

    #[test]
    fn tag_field_key_prefix_applies_without_trailing_space() {
        let s = suggest("tag:te", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(counted("tag:te"), pairs(&[("Team", 4)]));
        assert_eq!(
            apply_suggestion("tag:te", &s, 0).as_deref(),
            Some("tag:Team=")
        );
    }

    #[test]
    fn negated_tag_field_keeps_dash_in_insert() {
        let s = suggest("-tag:o", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(counted("-tag:o"), pairs(&[("Owner", 4)]));
        assert_eq!(s.items[0].insert, "-tag:Owner=");
    }

    #[test]
    fn tag_equals_lists_distinct_values_with_counts() {
        let s = suggest("tag:team=", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(
            counted("tag:team="),
            pairs(&[
                ("Team=data", 1),
                ("Team=payments", 1),
                ("Team=platform", 1),
                ("Team=search", 1),
            ])
        );
        assert_eq!(s.items[0].insert, "tag:Team=data");
    }

    #[test]
    fn tag_equals_value_prefix_filters_values() {
        assert_eq!(
            counted("tag:team=p"),
            pairs(&[("Team=payments", 1), ("Team=platform", 1)])
        );
    }

    #[test]
    fn tag_equals_unknown_key_has_no_items() {
        assert!(labels("tag:nope=").is_empty());
    }

    #[test]
    fn tag_shorthand_lists_values_sorted_with_counts() {
        let s = suggest("owner:", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(
            counted("owner:"),
            pairs(&[("esteban", 2), ("jonas", 1), ("mira", 1)])
        );
        assert_eq!(s.items[0].insert, "owner:esteban");
    }

    #[test]
    fn tag_shorthand_value_prefix_filters_values() {
        assert_eq!(counted("owner:j"), pairs(&[("jonas", 1)]));
    }

    #[test]
    fn tag_shorthand_keeps_typed_key_case_in_insert() {
        let s = suggest("Team:pa", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(counted("Team:pa"), pairs(&[("payments", 1)]));
        assert_eq!(
            apply_suggestion("Team:pa", &s, 0).as_deref(),
            Some("Team:payments ")
        );
    }

    #[test]
    fn tag_values_compare_exactly_and_skip_empty() {
        let mut instances = fleet();
        instances[0].tags[2].value = "Platform".to_string();
        instances[4].tags[1].value = String::new();
        let s = suggest("team:", &instances, &known_tag_keys(&instances));
        assert_eq!(
            s.items
                .iter()
                .map(|i| (i.label.as_str(), i.count))
                .collect::<Vec<_>>(),
            [
                ("Platform", Some(1)),
                ("payments", Some(1)),
                ("search", Some(1)),
            ]
        );
    }

    #[test]
    fn unknown_field_name_is_reported_as_typed() {
        let s = suggest("Colour:", &fleet(), &known_tag_keys(&fleet()));
        assert!(s.items.is_empty());
        assert_eq!(s.unknown_field.as_deref(), Some("Colour"));
        let s = suggest("state:", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(s.unknown_field, None);
    }

    #[test]
    fn apply_state_value_adds_trailing_space() {
        let s = suggest("web state:r", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(
            apply_suggestion("web state:r", &s, 0).as_deref(),
            Some("web state:running ")
        );
    }

    #[test]
    fn unknown_name_value_reports_unknown_field() {
        let s = suggest("colour:r", &fleet(), &known_tag_keys(&fleet()));
        assert!(s.items.is_empty());
        assert_eq!(s.unknown_field.as_deref(), Some("colour"));
        assert_eq!(s.replace_start, 0);
    }

    #[test]
    fn at_most_six_field_items() {
        let s = with_keys("t", &["Ta", "Tb", "Tc", "Td", "Te"]);
        assert_eq!(
            s.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["type:", "tag:", "ta:", "tb:", "tc:", "td:"]
        );
    }

    #[test]
    fn tag_keys_the_parser_cannot_read_as_names_are_not_suggested() {
        let s = with_keys("c", &["cost.center", "Cost Center", "cc-1"]);
        assert_eq!(
            s.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["cc-1:"]
        );
    }

    #[test]
    fn tag_key_matching_a_field_name_is_listed_once() {
        let s = with_keys("reg", &["Region"]);
        assert_eq!(
            s.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(),
            ["region:"]
        );
        assert_eq!(s.items[0].hint.as_deref(), Some("region"));
    }

    #[test]
    fn apply_field_name_adds_no_trailing_space() {
        let s = suggest("sta", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(apply_suggestion("sta", &s, 0).as_deref(), Some("state:"));
    }

    #[test]
    fn apply_replaces_only_last_word() {
        let s = suggest("web st", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(
            apply_suggestion("web st", &s, 0).as_deref(),
            Some("web state:")
        );
    }

    #[test]
    fn apply_keeps_negation() {
        let s = suggest("-st", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(apply_suggestion("-st", &s, 0).as_deref(), Some("-state:"));
    }

    #[test]
    fn apply_tag_key() {
        let s = suggest("o", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(apply_suggestion("o", &s, 0).as_deref(), Some("owner:"));
    }

    #[test]
    fn apply_out_of_range_is_none() {
        let s = suggest("st", &fleet(), &known_tag_keys(&fleet()));
        assert_eq!(apply_suggestion("st", &s, 99), None);
    }

    #[test]
    fn apply_value_adds_trailing_space_unless_colon_or_equals() {
        let s = Suggestions {
            replace_start: 0,
            unknown_field: None,
            items: vec![Suggestion {
                label: "state:running".into(),
                insert: "state:running".into(),
                hint: None,
                count: None,
                kind: SuggestionKind::Value,
            }],
        };
        assert_eq!(
            apply_suggestion("state:r", &s, 0).as_deref(),
            Some("state:running ")
        );
    }
}
