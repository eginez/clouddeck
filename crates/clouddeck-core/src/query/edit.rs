//! Rewrites a query's field terms as text and reads back the selected State and Type facets.

use super::term::{Term, TermKind, parse};
use super::tokenize::QueryField;

/// Removes every non-negated `field:` term from the query, keeping everything else in order.
pub fn query_without_field(query: &str, field: QueryField, known_tag_keys: &[String]) -> String {
    parse(query, known_tag_keys)
        .into_iter()
        .filter(|t| !is_positive_field(t, field))
        .map(|t| t.token.raw)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Replaces the query's `field:` terms with one `field:value`; a blank value only removes them.
pub fn set_field_value(
    query: &str,
    field: QueryField,
    value: &str,
    known_tag_keys: &[String],
) -> String {
    let base = query_without_field(query, field, known_tag_keys);
    let value = value.trim();
    if value.is_empty() {
        return base;
    }
    format!("{base} {}:{value}", field.name())
        .trim()
        .to_string()
}

/// The State and Type facet values selected by a query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FacetSelection {
    /// The selected state value as typed.
    pub state: Option<String>,
    /// The selected instance type value as typed.
    pub instance_type: Option<String>,
}

/// Reads the State and Type facets back out of a query.
pub fn facet_selection(query: &str, known_tag_keys: &[String]) -> FacetSelection {
    let terms = parse(query, known_tag_keys);
    FacetSelection {
        state: last_facet_value(&terms, QueryField::State),
        instance_type: last_facet_value(&terms, QueryField::Type),
    }
}

fn is_positive_field(term: &Term, field: QueryField) -> bool {
    !term.negated && matches!(&term.kind, TermKind::Field { field: f, .. } if *f == field)
}

fn last_facet_value(terms: &[Term], field: QueryField) -> Option<String> {
    terms.iter().rev().find_map(|t| match &t.kind {
        TermKind::Field {
            field: f, value, ..
        } if *f == field && !t.negated => {
            let pieces: Vec<&str> = value
                .split(',')
                .map(str::trim)
                .filter(|piece| !piece.is_empty())
                .collect();
            (!pieces.is_empty()).then(|| pieces.join(","))
        }
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{fixtures, known_tag_keys};

    fn known() -> Vec<String> {
        known_tag_keys(&fixtures::fleet())
    }

    #[test]
    fn query_without_field_removes_positive_field_terms() {
        let k = known();
        assert_eq!(
            query_without_field("web state:stopped type:t3.small", QueryField::State, &k),
            "web type:t3.small"
        );
        assert_eq!(
            query_without_field("-state:stopped state:running", QueryField::State, &k),
            "-state:stopped"
        );
        assert_eq!(
            query_without_field("state:a state:b", QueryField::State, &k),
            ""
        );
        assert_eq!(
            query_without_field("  web   owner:esteban ", QueryField::State, &k),
            "web owner:esteban"
        );
        assert_eq!(
            query_without_field("zone:us-east-1a web", QueryField::Az, &k),
            "web"
        );
        assert_eq!(query_without_field("", QueryField::State, &k), "");
        assert_eq!(
            query_without_field("owner:esteban state:stopped", QueryField::State, &k),
            "owner:esteban"
        );
    }

    #[test]
    fn set_field_value_replaces_or_adds_field() {
        let k = known();
        assert_eq!(
            set_field_value("web state:stopped", QueryField::State, "running", &k),
            "web state:running"
        );
        assert_eq!(
            set_field_value("web state:stopped", QueryField::State, "", &k),
            "web"
        );
        assert_eq!(
            set_field_value("", QueryField::Type, "t3.small", &k),
            "type:t3.small"
        );
        assert_eq!(
            set_field_value("  web  ", QueryField::State, "running", &k),
            "web state:running"
        );
        assert_eq!(
            set_field_value("state:stopped type:t3", QueryField::State, "pending", &k),
            "type:t3 state:pending"
        );
        assert_eq!(
            set_field_value("-state:stopped", QueryField::State, "running", &k),
            "-state:stopped state:running"
        );
        assert_eq!(
            set_field_value("state:running,stopped web", QueryField::State, "", &k),
            "web"
        );
    }

    #[test]
    fn facet_selection_reads_state_and_type() {
        let k = known();
        assert_eq!(
            facet_selection("state:stopped type:t3.small", &k),
            FacetSelection {
                state: Some("stopped".into()),
                instance_type: Some("t3.small".into()),
            }
        );
        assert_eq!(
            facet_selection("state:running,pending", &k),
            FacetSelection {
                state: Some("running,pending".into()),
                instance_type: None,
            }
        );
        assert_eq!(facet_selection("-state:stopped", &k).state, None);
        assert_eq!(facet_selection("state:", &k).state, None);
        assert_eq!(facet_selection("state:,", &k).state, None);
        assert_eq!(
            facet_selection("state:running,", &k).state,
            Some("running".into())
        );
        assert_eq!(
            facet_selection("state:running,,stopped", &k).state,
            Some("running,stopped".into())
        );
        assert_eq!(
            facet_selection("state:running state:stopped", &k).state,
            Some("stopped".into())
        );
        assert_eq!(facet_selection("web", &k), FacetSelection::default());
        assert_eq!(
            facet_selection("STATE:Running", &k).state,
            Some("Running".into())
        );
    }

    #[test]
    fn set_then_read_state_round_trips() {
        let k = known();
        for v in ["running", "stopped", "pending"] {
            let q = set_field_value("web type:t3", QueryField::State, v, &k);
            assert_eq!(facet_selection(&q, &k).state, Some(v.to_string()));
            assert!(q.contains("web"));
            assert!(q.contains("type:t3"));
        }
    }
}
