use std::collections::BTreeMap;
use std::time::{Duration, SystemTime};

use crate::ids::{ConnectionId, FlowId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthMethod {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthInput {
    pub connection: ConnectionId,
    pub values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptField {
    pub id: String,
    pub label: String,
    pub secret: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthAnswer {
    Poll,
    Values(BTreeMap<String, String>),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    SignedOut,
    SignedIn { expires: Option<SystemTime> },
    Expired,
}
