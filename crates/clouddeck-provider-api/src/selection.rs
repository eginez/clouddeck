use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{DimensionId, ValueId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DimValue {
    pub id: ValueId,
    pub label: String,
    /// `None` marks a root value.
    pub parent: Option<ValueId>,
    pub default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dimension {
    pub id: DimensionId,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plural_label: Option<String>,
    pub multi: bool,
    pub optional: bool,
    pub values: Vec<DimValue>,
}

impl Dimension {
    pub fn plural(&self) -> String {
        match &self.plural_label {
            Some(plural) => plural.clone(),
            None => format!("{}s", self.label),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection(pub BTreeMap<DimensionId, Vec<ValueId>>);

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, dimension: impl Into<DimensionId>, values: Vec<ValueId>) -> Self {
        self.0.insert(dimension.into(), values);
        self
    }

    pub fn values(&self, dimension: &str) -> &[ValueId] {
        self.0.get(dimension).map_or(&[], Vec::as_slice)
    }

    pub fn is_empty(&self) -> bool {
        self.0.values().all(Vec::is_empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dimension(label: &str, plural_label: Option<&str>) -> Dimension {
        Dimension {
            id: "d".into(),
            label: label.into(),
            plural_label: plural_label.map(str::to_string),
            multi: true,
            optional: false,
            values: Vec::new(),
        }
    }

    #[test]
    fn plural_falls_back_to_label_plus_s() {
        assert_eq!(dimension("Region", None).plural(), "Regions");
    }

    #[test]
    fn plural_uses_override() {
        assert_eq!(
            dimension("Compartment", Some("Compartments")).plural(),
            "Compartments"
        );
        assert_eq!(dimension("Child", Some("Children")).plural(), "Children");
    }

    #[test]
    fn json_without_plural_label_deserializes() {
        let json = r#"{"id":"d","label":"Region","multi":true,"optional":false,"values":[]}"#;
        let parsed: Dimension = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.plural_label, None);
        assert_eq!(parsed.plural(), "Regions");
    }

    #[test]
    fn none_plural_label_is_not_serialized() {
        let json = serde_json::to_string(&dimension("Region", None)).unwrap();
        assert!(!json.contains("plural_label"), "{json}");
    }

    #[test]
    fn some_plural_label_round_trips() {
        let original = dimension("Zone", Some("Zones"));
        let json = serde_json::to_string(&original).unwrap();
        assert!(json.contains("\"plural_label\":\"Zones\""), "{json}");
        let parsed: Dimension = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, original);
    }
}
