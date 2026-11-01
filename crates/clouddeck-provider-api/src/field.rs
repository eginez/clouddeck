use crate::instance::{ExtrasSection, Instance};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Enum,
    Tag,
    Address,
    Date,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    Contains,
    StartsWith,
    Glob,
    PrefixOrGlob,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreField {
    Id,
    Name,
    State,
    Type,
    Region,
    Location,
    Group,
    Addresses,
}

/// Where a field's values come from. A field may yield several values; a term matches when any one does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldSource {
    Core(CoreField),
    Extra {
        section: &'static str,
        key: &'static str,
    },
    Section {
        section: &'static str,
    },
    Labels,
    Concat(Vec<FieldSource>),
}

impl FieldSource {
    pub fn values(&self, instance: &Instance) -> Vec<String> {
        match self {
            FieldSource::Core(field) => core_values(*field, instance),
            FieldSource::Extra { section, key } => extra_value(&instance.extras, section, key)
                .into_iter()
                .collect(),
            FieldSource::Section { section } => section_entries(&instance.extras, section),
            FieldSource::Labels => instance
                .labels
                .iter()
                .flat_map(|label| [label.key.clone(), label.value.clone().unwrap_or_default()])
                .collect(),
            FieldSource::Concat(parts) => parts
                .iter()
                .flat_map(|part| part.values(instance))
                .collect(),
        }
    }
}

fn core_values(field: CoreField, instance: &Instance) -> Vec<String> {
    let single = |value: Option<&String>| value.cloned().into_iter().collect();
    match field {
        CoreField::Id => vec![instance.id.clone()],
        CoreField::Name => single(instance.name.as_ref()),
        CoreField::State => vec![instance.state.raw.clone()],
        CoreField::Type => single(instance.instance_type.as_ref()),
        CoreField::Region => single(instance.region.as_ref()),
        CoreField::Location => single(instance.location.as_ref()),
        CoreField::Group => single(instance.group.as_ref()),
        CoreField::Addresses => instance
            .addresses
            .iter()
            .map(|address| address.value.clone())
            .collect(),
    }
}

fn extra_value(sections: &[ExtrasSection], section: &str, key: &str) -> Option<String> {
    sections
        .iter()
        .filter(|s| s.title == section)
        .flat_map(|s| s.entries.iter())
        .find(|(entry_key, _)| entry_key == key)
        .map(|(_, value)| value.clone())
}

fn section_entries(sections: &[ExtrasSection], section: &str) -> Vec<String> {
    sections
        .iter()
        .find(|s| s.title == section)
        .map(|s| {
            s.entries
                .iter()
                .flat_map(|(key, value)| [key.clone(), value.clone()])
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldDef {
    pub name: String,
    pub label: String,
    /// Text shown next to the key in query suggestions.
    pub hint: String,
    pub kind: FieldKind,
    pub match_mode: MatchMode,
    /// Usable as a `name:value` query key.
    pub query: bool,
    /// Values are part of free-text search.
    pub search: bool,
    pub facet: bool,
    pub column: bool,
    pub source: FieldSource,
}
#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::instance::{Address, AddressKind, Label, State, StateKind};

    fn sample() -> Instance {
        Instance {
            id: "us-east-1:i-1".to_string(),
            name: Some("api".to_string()),
            group: Some("vpc-1".to_string()),
            state: State {
                kind: StateKind::Running,
                raw: "running".to_string(),
            },
            state_reason: None,
            instance_type: Some("t3.micro".to_string()),
            region: Some("us-east-1".to_string()),
            location: Some("us-east-1a".to_string()),
            created: Some(SystemTime::UNIX_EPOCH),
            addresses: vec![
                Address {
                    kind: AddressKind::Private,
                    value: "10.0.0.1".to_string(),
                },
                Address {
                    kind: AddressKind::Public,
                    value: "54.0.0.1".to_string(),
                },
            ],
            labels: vec![
                Label {
                    key: "team".to_string(),
                    value: Some("ops".to_string()),
                },
                Label {
                    key: "empty".to_string(),
                    value: None,
                },
            ],
            extras: vec![
                ExtrasSection {
                    title: "Network".to_string(),
                    entries: vec![
                        ("VPC".to_string(), "vpc-1".to_string()),
                        ("Public DNS".to_string(), String::new()),
                        ("Private DNS".to_string(), "ip-10-0-0-1".to_string()),
                    ],
                },
                ExtrasSection {
                    title: "Security groups".to_string(),
                    entries: vec![("web".to_string(), "sg-1".to_string())],
                },
            ],
        }
    }

    fn values(source: FieldSource) -> Vec<String> {
        source.values(&sample())
    }

    #[test]
    fn core_fields_yield_their_value() {
        assert_eq!(values(FieldSource::Core(CoreField::Id)), ["us-east-1:i-1"]);
        assert_eq!(values(FieldSource::Core(CoreField::Name)), ["api"]);
        assert_eq!(values(FieldSource::Core(CoreField::State)), ["running"]);
        assert_eq!(values(FieldSource::Core(CoreField::Type)), ["t3.micro"]);
        assert_eq!(values(FieldSource::Core(CoreField::Region)), ["us-east-1"]);
        assert_eq!(
            values(FieldSource::Core(CoreField::Location)),
            ["us-east-1a"]
        );
        assert_eq!(values(FieldSource::Core(CoreField::Group)), ["vpc-1"]);
    }

    #[test]
    fn absent_core_field_yields_nothing() {
        let mut instance = sample();
        instance.name = None;
        instance.group = None;
        assert!(
            FieldSource::Core(CoreField::Name)
                .values(&instance)
                .is_empty()
        );
        assert!(
            FieldSource::Core(CoreField::Group)
                .values(&instance)
                .is_empty()
        );
    }

    #[test]
    fn addresses_yield_all_values_in_order() {
        assert_eq!(
            values(FieldSource::Core(CoreField::Addresses)),
            ["10.0.0.1", "54.0.0.1"]
        );
    }

    #[test]
    fn extra_yields_first_matching_key_or_nothing() {
        assert_eq!(
            values(FieldSource::Extra {
                section: "Network",
                key: "VPC",
            }),
            ["vpc-1"]
        );
        assert_eq!(
            values(FieldSource::Extra {
                section: "Network",
                key: "Public DNS",
            }),
            [""]
        );
        assert!(
            values(FieldSource::Extra {
                section: "Network",
                key: "Missing",
            })
            .is_empty()
        );
    }

    #[test]
    fn section_flattens_key_value_pairs() {
        assert_eq!(
            values(FieldSource::Section {
                section: "Security groups",
            }),
            ["web", "sg-1"]
        );
        assert!(values(FieldSource::Section { section: "Nope" }).is_empty());
    }

    #[test]
    fn labels_yield_key_then_value_or_empty() {
        assert_eq!(values(FieldSource::Labels), ["team", "ops", "empty", ""]);
    }

    #[test]
    fn concat_joins_parts_in_order() {
        let dns = FieldSource::Concat(vec![
            FieldSource::Extra {
                section: "Network",
                key: "Private DNS",
            },
            FieldSource::Extra {
                section: "Network",
                key: "Public DNS",
            },
        ]);
        assert_eq!(values(dns), ["ip-10-0-0-1", ""]);
    }
}
