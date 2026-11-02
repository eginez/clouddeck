use serde::{Deserialize, Serialize};

/// Where an inspector row's value comes from on a neutral `Instance`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InspectorValue {
    State,
    Type,
    Location,
    Created,
    PrivateAddress,
    PublicAddress,
    Extra { section: String, key: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectorRowDef {
    pub label: String,
    pub value: InspectorValue,
    pub monospace: bool,
    pub copyable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectorSectionDef {
    pub title: String,
    pub rows: Vec<InspectorRowDef>,
}
