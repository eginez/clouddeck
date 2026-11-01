use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StateKind {
    Running,
    Stopped,
    Starting,
    Stopping,
    Terminated,
    #[default]
    Unknown,
}

/// `raw` is the provider's own state text, shown as-is.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct State {
    pub kind: StateKind,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Label {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AddressKind {
    Private,
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Address {
    pub kind: AddressKind,
    pub value: String,
}

/// Provider-specific key/value lines shown in order. An empty value means the attribute is absent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtrasSection {
    pub title: String,
    pub entries: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Instance {
    pub id: String,
    pub name: Option<String>,
    pub group: Option<String>,
    pub state: State,
    /// Provider's explanation of the last state change.
    pub state_reason: Option<String>,
    pub instance_type: Option<String>,
    pub region: Option<String>,
    pub location: Option<String>,
    pub created: Option<SystemTime>,
    pub addresses: Vec<Address>,
    pub labels: Vec<Label>,
    pub extras: Vec<ExtrasSection>,
}

impl Instance {
    pub fn address(&self, kind: AddressKind) -> Option<&str> {
        self.addresses
            .iter()
            .find(|address| address.kind == kind)
            .map(|address| address.value.as_str())
    }
}
