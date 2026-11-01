use crate::FieldDef;
use std::sync::OnceLock;

/// Phase 1 has one provider; the app injects `provider.fields()` later (clouddeck-rs-ju62).
pub fn provider_fields() -> &'static [FieldDef] {
    static FIELDS: OnceLock<Vec<FieldDef>> = OnceLock::new();
    FIELDS.get_or_init(crate::aws_fields)
}

pub fn provider_field(name: &str) -> Option<&'static FieldDef> {
    provider_fields().iter().find(|def| def.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_field_finds_registry_entries_by_name() {
        assert!(provider_field("sg").is_some());
        assert!(provider_field("nope").is_none());
    }
}
