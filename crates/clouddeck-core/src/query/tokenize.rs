//! Splits a query string into whitespace-separated tokens and names query fields.

/// A single token with its byte offsets in the original query string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The raw token text; quotes are preserved.
    pub raw: String,
    /// Byte offset of the first character of the token.
    pub start: usize,
    /// Byte offset one past the last character of the token.
    pub end: usize,
}

/// Field names recognised in `field:value` query terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueryField {
    /// `name:` instance name.
    Name,
    /// `id:` instance id.
    Id,
    /// `state:` instance state.
    State,
    /// `type:` instance type.
    Type,
    /// `az:` availability zone (also `zone:`).
    Az,
    /// `region:` region.
    Region,
    /// `ip:` any IP address.
    Ip,
    /// `private:` private IP.
    Private,
    /// `public:` public IP.
    Public,
    /// `age:` launch age.
    Age,
    /// `has:` attribute presence.
    Has,
    /// `tag:` tag filters.
    Tag,
    /// A provider field from `provider_fields()`, by name.
    Provider(&'static str),
}

impl QueryField {
    /// All query fields: neutral fields, then provider fields in registry order, then age, has and tag.
    pub fn all() -> Vec<QueryField> {
        let mut fields = vec![
            QueryField::Name,
            QueryField::Id,
            QueryField::State,
            QueryField::Type,
            QueryField::Az,
            QueryField::Region,
            QueryField::Ip,
            QueryField::Private,
            QueryField::Public,
        ];
        fields.extend(
            super::fields::provider_fields()
                .iter()
                .filter(|def| def.query)
                .map(|def| QueryField::Provider(def.name.as_str())),
        );
        fields.extend([QueryField::Age, QueryField::Has, QueryField::Tag]);
        fields
    }

    /// Lowercase name of the field.
    pub fn name(self) -> &'static str {
        match self {
            QueryField::Name => "name",
            QueryField::Id => "id",
            QueryField::State => "state",
            QueryField::Type => "type",
            QueryField::Az => "az",
            QueryField::Region => "region",
            QueryField::Ip => "ip",
            QueryField::Private => "private",
            QueryField::Public => "public",
            QueryField::Age => "age",
            QueryField::Has => "has",
            QueryField::Tag => "tag",
            QueryField::Provider(name) => name,
        }
    }

    /// Case-insensitive lookup of a field by name; `zone` is an alias for `az`.
    pub fn from_name(s: &str) -> Option<QueryField> {
        let lower = s.to_ascii_lowercase();
        if lower == "zone" {
            return Some(QueryField::Az);
        }
        Self::all().into_iter().find(|f| f.name() == lower)
    }
}

/// Splits a query into tokens on whitespace; a double quote toggles quoted
/// mode where whitespace does not split, and quotes stay in the raw text.
pub fn tokenize(query: &str) -> Vec<Token> {
    let bytes = query.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        // Skip whitespace outside quotes.
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let start = i;
        let mut in_quotes = false;
        while i < bytes.len() {
            match bytes[i] {
                b'"' => in_quotes = !in_quotes,
                b if b.is_ascii_whitespace() && !in_quotes => break,
                _ => {}
            }
            i += 1;
        }
        tokens.push(Token {
            raw: query[start..i].to_string(),
            start,
            end: i,
        });
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_whitespace_only_queries_yield_no_tokens() {
        assert_eq!(tokenize(""), Vec::<Token>::new());
        assert_eq!(tokenize("   "), Vec::<Token>::new());
    }

    #[test]
    fn plain_words_with_offsets() {
        let tokens = tokenize("web  state:stopped");
        assert_eq!(
            tokens
                .iter()
                .map(|t| t.raw.as_str())
                .collect::<Vec<_>>(),
            ["web", "state:stopped"]
        );
        assert_eq!((tokens[0].start, tokens[0].end), (0, 3));
        assert_eq!((tokens[1].start, tokens[1].end), (5, 18));
        assert_eq!(&"web  state:stopped"[tokens[1].start..tokens[1].end], tokens[1].raw);
    }

    #[test]
    fn quoted_text_does_not_split_on_whitespace() {
        let tokens = tokenize(r#"state:stopped "api gateway" x"#);
        assert_eq!(
            tokens
                .iter()
                .map(|t| t.raw.as_str())
                .collect::<Vec<_>>(),
            ["state:stopped", "\"api gateway\"", "x"]
        );
        for t in &tokens {
            assert_eq!(&"state:stopped \"api gateway\" x"[t.start..t.end], t.raw);
        }
    }

    #[test]
    fn unclosed_quote_runs_to_end_of_string() {
        let tokens = tokenize(r#"name:"a b"#);
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].raw, r#"name:"a b"#);
    }

    #[test]
    fn leading_and_trailing_whitespace_is_trimmed() {
        let tokens = tokenize("  -x  ");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].raw, "-x");
        assert_eq!(tokens[0].start, 2);
        assert_eq!(tokens[0].end, 4);
    }

    #[test]
    fn all_fields_have_expected_names() {
        assert_eq!(QueryField::all().len(), 17);
        assert_eq!(
            QueryField::all()
                .iter()
                .map(|f| f.name())
                .collect::<Vec<_>>(),
            [
                "name", "id", "state", "type", "az", "region", "ip", "private", "public", "vpc",
                "subnet", "sg", "ami", "key", "age", "has", "tag",
            ]
        );
    }

    #[test]
    fn from_name_is_case_insensitive_with_zone_alias() {
        assert_eq!(QueryField::from_name("Zone"), Some(QueryField::Az));
        assert_eq!(QueryField::from_name("STATE"), Some(QueryField::State));
        assert_eq!(QueryField::from_name("nope"), None);
        assert_eq!(QueryField::from_name(""), None);
    }

    #[test]
    fn from_name_unknown_field_is_none() {
        assert_eq!(QueryField::from_name("nope"), None);
    }
}
