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
    pub multi: bool,
    pub optional: bool,
    pub values: Vec<DimValue>,
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
