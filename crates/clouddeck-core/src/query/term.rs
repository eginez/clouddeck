//! Classifies tokens into search terms (words, field filters, tag shorthands).

use super::tokenize::{QueryField, Token, tokenize};

/// The meaning of a classified query token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermKind {
    /// A plain search word.
    Word {
        /// The word text.
        text: String,
    },
    /// A `field:value` filter on a known query field.
    Field {
        /// The matched query field.
        field: QueryField,
        /// The field name exactly as typed.
        name: String,
        /// The value part of the token.
        value: String,
    },
    /// A `key:value` filter shorthanded through a known tag key.
    TagShorthand {
        /// The matching known tag key in its canonical case.
        key: String,
        /// The key exactly as typed.
        name: String,
        /// The value part of the token.
        value: String,
    },
    /// A `name:value` token whose name is neither a field nor a tag key.
    Unknown {
        /// The name part of the token.
        name: String,
        /// The full body with every quote removed.
        text: String,
    },
}

/// A classified token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Term {
    /// The source token.
    pub token: Token,
    /// Whether a leading `-` negated the term.
    pub negated: bool,
    /// What the token means.
    pub kind: TermKind,
}

impl Term {
    /// Error message for unknown fields; `None` for every other kind.
    pub fn error(&self) -> Option<String> {
        match &self.kind {
            TermKind::Unknown { name, .. } => {
                Some(format!("No field named “{name}”. Searching for the text instead."))
            }
            _ => None,
        }
    }
}

/// Classifies a single token into a term.
pub fn classify(token: &Token, known_tag_keys: &[String]) -> Term {
    let (negated, body) = split_negation(&token.raw);
    field_name_end(body).map_or_else(
        || word_term(token, negated, body),
        |name_end| field_term(token, negated, body, name_end, known_tag_keys),
    )
}

fn split_negation(raw: &str) -> (bool, &str) {
    if raw.len() > 1 && raw.starts_with('-') {
        (true, &raw[1..])
    } else {
        (false, raw)
    }
}

/// Byte index of the `:` that ends a field name: one ASCII letter, then ASCII letters, digits, `_` or `-`.
fn field_name_end(body: &str) -> Option<usize> {
    let bytes = body.as_bytes();
    let first = bytes.first()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let colon = bytes.iter().position(|b| *b == b':')?;
    bytes[1..colon]
        .iter()
        .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
        .then_some(colon)
}

fn word_term(token: &Token, negated: bool, body: &str) -> Term {
    Term {
        token: token.clone(),
        negated,
        kind: TermKind::Word {
            text: body.replace('"', ""),
        },
    }
}

fn field_term(
    token: &Token,
    negated: bool,
    body: &str,
    name_end: usize,
    known_tag_keys: &[String],
) -> Term {
    let name = &body[..name_end];
    let value = body[name_end + 1..].trim_matches(|c| c == '"');
    Term {
        token: token.clone(),
        negated,
        kind: field_kind(name, value, body, known_tag_keys),
    }
}

fn field_kind(name: &str, value: &str, body: &str, known_tag_keys: &[String]) -> TermKind {
    QueryField::from_name(name).map_or_else(
        || tag_or_unknown_kind(name, value, body, known_tag_keys),
        |field| TermKind::Field {
            field,
            name: name.to_string(),
            value: value.to_string(),
        },
    )
}

fn tag_or_unknown_kind(name: &str, value: &str, body: &str, known_tag_keys: &[String]) -> TermKind {
    known_tag_keys
        .iter()
        .find(|k| k.eq_ignore_ascii_case(name))
        .map_or_else(
            || TermKind::Unknown {
                name: name.to_string(),
                text: body.replace('"', ""),
            },
            |key| TermKind::TagShorthand {
                key: key.clone(),
                name: name.to_string(),
                value: value.to_string(),
            },
        )
}

/// Tokenizes a query and classifies each token into a term.
pub fn parse(query: &str, known_tag_keys: &[String]) -> Vec<Term> {
    tokenize(query)
        .iter()
        .map(|t| classify(t, known_tag_keys))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::QueryField;

    fn keys() -> Vec<String> {
        vec!["CostCenter", "Environment", "Owner", "Team"]
            .into_iter()
            .map(String::from)
            .collect()
    }

    fn k(raw: &str) -> Term {
        classify(&tokenize(raw)[0], &keys())
    }

    #[test]
    fn plain_word() {
        assert_eq!(
            k("web"),
            Term {
                token: Token {
                    raw: "web".into(),
                    start: 0,
                    end: 3
                },
                negated: false,
                kind: TermKind::Word {
                    text: "web".into()
                }
            }
        );
    }

    #[test]
    fn negated_field() {
        let t = k("-state:stopped");
        assert!(t.negated);
        assert_eq!(
            t.kind,
            TermKind::Field {
                field: QueryField::State,
                name: "state".into(),
                value: "stopped".into()
            }
        );
        assert_eq!(t.error(), None);
    }

    #[test]
    fn lone_dash_is_word() {
        let t = k("-");
        assert!(!t.negated);
        assert_eq!(t.kind, TermKind::Word {
            text: "-".into()
        });
    }

    #[test]
    fn zone_alias() {
        assert_eq!(
            k("zone:us-east-1a").kind,
            TermKind::Field {
                field: QueryField::Az,
                name: "zone".into(),
                value: "us-east-1a".into()
            }
        );
    }

    #[test]
    fn field_name_case_insensitive() {
        assert_eq!(
            k("STATE:Running").kind,
            TermKind::Field {
                field: QueryField::State,
                name: "STATE".into(),
                value: "Running".into()
            }
        );
    }

    #[test]
    fn tag_shorthand_lower() {
        assert_eq!(
            k("owner:esteban").kind,
            TermKind::TagShorthand {
                key: "Owner".into(),
                name: "owner".into(),
                value: "esteban".into()
            }
        );
    }

    #[test]
    fn tag_shorthand_upper() {
        assert_eq!(
            k("OWNER:x").kind,
            TermKind::TagShorthand {
                key: "Owner".into(),
                name: "OWNER".into(),
                value: "x".into()
            }
        );
    }

    #[test]
    fn tag_field_wins_over_tag_key() {
        assert_eq!(
            k("tag:Team=data").kind,
            TermKind::Field {
                field: QueryField::Tag,
                name: "tag".into(),
                value: "Team=data".into()
            }
        );
    }

    #[test]
    fn quoted_field_value() {
        assert_eq!(
            k("name:\"api gw\"").kind,
            TermKind::Field {
                field: QueryField::Name,
                name: "name".into(),
                value: "api gw".into()
            }
        );
        assert_eq!(k("name:\"api gw\"").error(), None);
    }

    #[test]
    fn quoted_word() {
        assert_eq!(
            k("\"api gateway\"").kind,
            TermKind::Word {
                text: "api gateway".into()
            }
        );
    }

    #[test]
    fn unknown_field_with_error() {
        let t = k("colour:red");
        assert_eq!(
            t.kind,
            TermKind::Unknown {
                name: "colour".into(),
                text: "colour:red".into()
            }
        );
        assert_eq!(
            t.error(),
            Some("No field named “colour”. Searching for the text instead.".into())
        );
    }

    #[test]
    fn empty_value() {
        assert_eq!(
            k("state:").kind,
            TermKind::Field {
                field: QueryField::State,
                name: "state".into(),
                value: "".into()
            }
        );
    }

    #[test]
    fn name_must_start_with_letter() {
        assert_eq!(k("1x:y").kind, TermKind::Word {
            text: "1x:y".into()
        });
    }

    #[test]
    fn value_may_contain_colons() {
        let t = k("a:b:c");
        assert_eq!(
            t.kind,
            TermKind::Unknown {
                name: "a".into(),
                text: "a:b:c".into()
            }
        );
    }

    #[test]
    fn word_and_field_errors_none() {
        assert_eq!(k("web").error(), None);
        assert_eq!(k("state:stopped").error(), None);
        assert_eq!(k("-").error(), None);
        assert_eq!(k("1x:y").error(), None);
        assert_eq!(k("\"api gateway\"").error(), None);
    }

    #[test]
    fn parse_splits_and_classifies() {
        assert_eq!(parse("web -state:stopped", &keys()).len(), 2);
    }
}
