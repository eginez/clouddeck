//! Rewrites a query's field terms as text and reads back the selected State and Type facets.

use super::matcher::glob;
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

/// Removes one value from the query's `field:` term, keeping its other comma-separated values.
pub fn remove_field_value(
    query: &str,
    field: QueryField,
    value: &str,
    known_tag_keys: &[String],
) -> String {
    let terms = parse(query, known_tag_keys);
    let value = value.trim();
    let remaining: Vec<String> = all_positive_values(&terms, field)
        .into_iter()
        .filter(|piece| !piece.eq_ignore_ascii_case(value))
        .collect();
    set_field_value(query, field, &remaining.join(","), known_tag_keys)
}

/// The State and Type values a query actually selects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FacetSelection {
    /// The values the query selects for state, as typed.
    pub state: Option<String>,
    /// The values the query selects for instance type, as typed.
    pub instance_type: Option<String>,
}

/// Reads the values the State and Type facets select, intersected across repeated terms.
pub fn facet_selection(query: &str, known_tag_keys: &[String]) -> FacetSelection {
    let terms = parse(query, known_tag_keys);
    FacetSelection {
        state: effective_facet_value(&terms, QueryField::State),
        instance_type: effective_facet_value(&terms, QueryField::Type),
    }
}

fn is_positive_field(term: &Term, field: QueryField) -> bool {
    !term.negated && matches!(&term.kind, TermKind::Field { field: f, .. } if *f == field)
}

fn all_positive_values(terms: &[Term], field: QueryField) -> Vec<String> {
    terms
        .iter()
        .filter(|t| is_positive_field(t, field))
        .flat_map(|t| match &t.kind {
            TermKind::Field { value, .. } => value
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

fn effective_facet_value(terms: &[Term], field: QueryField) -> Option<String> {
    let per_term: Vec<Vec<&str>> = terms
        .iter()
        .filter(|t| is_positive_field(t, field))
        .filter_map(|t| match &t.kind {
            TermKind::Field { value, .. } => {
                let pieces: Vec<&str> = value
                    .split(',')
                    .map(str::trim)
                    .filter(|piece| !piece.is_empty())
                    .collect();
                (!pieces.is_empty()).then_some(pieces)
            }
            _ => None,
        })
        .collect();
    if let [only] = per_term.as_slice() {
        return Some(only.join(","));
    }
    let mut candidates: Vec<&str> = Vec::new();
    for piece in per_term.iter().flatten().filter(|p| !p.contains('*')) {
        if !candidates.iter().any(|c| c.eq_ignore_ascii_case(piece)) {
            candidates.push(piece);
        }
    }
    let kept: Vec<&str> = candidates
        .into_iter()
        .filter(|candidate| {
            per_term
                .iter()
                .all(|term| term.iter().any(|pattern| covers(field, pattern, candidate)))
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join(","))
}

/// Mirrors the matcher's rules for State and Type, so highlights agree with the filter.
fn covers(field: QueryField, pattern: &str, value: &str) -> bool {
    let (value, pattern) = (value.to_lowercase(), pattern.to_lowercase());
    match field {
        QueryField::Type if pattern.contains('*') => glob(&value, &pattern),
        _ => value.starts_with(&pattern),
    }
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
    fn remove_field_value_peels_one_value_off_the_term() {
        let k = known();
        assert_eq!(
            remove_field_value("type:a,b", QueryField::Type, "a", &k),
            "type:b"
        );
        assert_eq!(
            remove_field_value("web type:a,b,c", QueryField::Type, "b", &k),
            "web type:a,c"
        );
        assert_eq!(remove_field_value("type:a", QueryField::Type, "a", &k), "");
        assert_eq!(
            remove_field_value("web type:A,b", QueryField::Type, "a", &k),
            "web type:b"
        );
        assert_eq!(
            remove_field_value(
                "state:running,stopped web",
                QueryField::State,
                "stopped",
                &k
            ),
            "web state:running"
        );
        assert_eq!(
            remove_field_value("-type:a type:a,b", QueryField::Type, "a", &k),
            "-type:a type:b"
        );
        assert_eq!(remove_field_value("web", QueryField::Type, "a", &k), "web");
        assert_eq!(
            remove_field_value("type:a type:b", QueryField::Type, "b", &k),
            "type:a"
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
            None
        );
        assert_eq!(
            facet_selection("state:running,stopped state:stopped", &k).state,
            Some("stopped".into())
        );
        assert_eq!(
            facet_selection("state:run state:running", &k).state,
            Some("running".into())
        );
        assert_eq!(facet_selection("type:a type:b", &k).instance_type, None);
        assert_eq!(
            facet_selection("type:a,b type:b", &k).instance_type,
            Some("b".into())
        );
        assert_eq!(
            facet_selection("type:b,a type:a,b", &k).instance_type,
            Some("b,a".into())
        );
        assert_eq!(
            facet_selection("type:A,b type:a", &k).instance_type,
            Some("A".into())
        );
        assert_eq!(
            facet_selection("type:a type:", &k).instance_type,
            Some("a".into())
        );
        assert_eq!(
            facet_selection("type:a -type:b", &k).instance_type,
            Some("a".into())
        );
        assert_eq!(
            facet_selection("type:t3 type:t3.micro", &k).instance_type,
            Some("t3.micro".into())
        );
        assert_eq!(
            facet_selection("type:t3.micro type:t3", &k).instance_type,
            Some("t3.micro".into())
        );
        assert_eq!(
            facet_selection("type:t3* type:t3.micro", &k).instance_type,
            Some("t3.micro".into())
        );
        assert_eq!(
            facet_selection("type:t3.micro,m5.large type:t3", &k).instance_type,
            Some("t3.micro".into())
        );
        assert_eq!(
            facet_selection("type:t3*", &k).instance_type,
            Some("t3*".into())
        );
        assert_eq!(
            facet_selection("type:a,b", &k).instance_type,
            Some("a,b".into())
        );
        assert_eq!(
            facet_selection("type:T3.Micro type:t3", &k).instance_type,
            Some("T3.Micro".into())
        );
        assert_eq!(
            facet_selection("-type:t3 type:t3.micro", &k).instance_type,
            Some("t3.micro".into())
        );
        assert_eq!(
            facet_selection("state: state:running", &k).state,
            Some("running".into())
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
