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
    let mut fields = vec![
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
