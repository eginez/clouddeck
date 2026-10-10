//! Autocomplete for the word being typed in the search field, and applying a chosen suggestion.

use super::term::{TermKind, classify};
use super::tokenize::{QueryField, tokenize};
use crate::models::EC2Instance;

const MAX_FIELD_ITEMS: usize = 6;

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

/// Suggests fields for the word at the end of `query`.
pub fn suggest(query: &str, _instances: &[EC2Instance], known_tag_keys: &[String]) -> Suggestions {
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
        TermKind::Unknown { name, .. } => {
            return Suggestions {
                items: Vec::new(),
                replace_start,
                unknown_field: Some(name),
            };
        }
        // value suggestions: query engine beads 11-12
        _ => {
            return Suggestions {
                items: Vec::new(),
                replace_start,
                unknown_field: None,
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
    fn name_value_has_no_items_until_value_suggestions_exist() {
        assert!(labels("state:r").is_empty());
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
