/// Where an inspector row's value comes from on a neutral `Instance`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InspectorValue {
    State,
    Type,
    Location,
    Created,
    PrivateAddress,
    PublicAddress,
    Extra {
        section: &'static str,
        key: &'static str,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorRowDef {
    pub label: &'static str,
    pub value: InspectorValue,
    pub monospace: bool,
    pub copyable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectorSectionDef {
    pub title: &'static str,
    pub rows: Vec<InspectorRowDef>,
}
