//! Searchable/columnar field definitions for instances.

use clouddeck_extension_api::{CoreField, FieldDef, FieldKind, FieldSource, MatchMode};

fn field(name: &str, label: &str, kind: FieldKind, core: CoreField, facet: bool) -> FieldDef {
    FieldDef {
        name: name.to_string(),
        label: label.to_string(),
        hint: format!("match on the instance {label}"),
        kind,
        match_mode: MatchMode::PrefixOrGlob,
        query: true,
        search: true,
        facet,
        column: true,
        source: FieldSource::Core(core),
    }
}

pub fn instance_fields() -> Vec<FieldDef> {
    use CoreField as C;
    use FieldKind as K;
    let mut id = field("id", "ID", K::Text, C::Id, false);
    id.query = false;
    id.search = false;
    let mut fields = vec![
        id,
        field("name", "Name", K::Text, C::Name, false),
        field("state", "State", K::Enum, C::State, true),
        field("type", "Machine type", K::Text, C::Type, true),
        field("zone", "Zone", K::Text, C::Location, true),
        field("region", "Region", K::Text, C::Region, true),
        field("ip", "IP address", K::Address, C::Addresses, false),
    ];
    let mut labels = field("label", "Labels", K::Tag, C::Group, false);
    labels.source = FieldSource::Labels;
    labels.column = false;
    fields.push(labels);
    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_headers_use_zone_and_id() {
        let fields = instance_fields();
        let label = |core: CoreField| {
            fields
                .iter()
                .find(|field| field.column && field.source == FieldSource::Core(core))
                .map(|field| field.label.clone())
        };
        assert_eq!(label(CoreField::Location).as_deref(), Some("Zone"));
        assert_eq!(label(CoreField::Id).as_deref(), Some("ID"));
    }

    #[test]
    fn id_is_a_header_only_field() {
        let fields = instance_fields();
        let id = fields.iter().find(|field| field.name == "id").unwrap();
        assert!(!id.query && !id.search);
    }
}
