use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::ids::{ConnectionId, FlowId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthMethod {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Keys of the provider's declared settings that this method's connect form shows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub settings: Vec<String>,
}

impl AuthMethod {
    pub fn new(id: String, label: String) -> Self {
        Self {
            id,
            label,
            description: None,
            settings: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthInput {
    pub connection: Option<ConnectionId>,
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptField {
    pub id: String,
    pub label: String,
    pub secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthAnswer {
    Poll,
    Values(BTreeMap<String, String>),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthStep {
    Done,
    Prompt {
        flow: FlowId,
        fields: Vec<PromptField>,
    },
    OpenUrl {
        flow: FlowId,
        url: String,
        then_poll: bool,
    },
    DeviceCode {
        flow: FlowId,
        code: String,
        url: String,
        complete_url: Option<String>,
        expires_in: Duration,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuthStatus {
    SignedOut,
    SignedIn {
        expires: Option<SystemTime>,
    },
    Expired,
    /// The check itself failed (for example a network error), so the sign-in state is not known.
    Unknown,
}
