use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorKind {
    Auth,
    Network,
    Timeout,
    Permission,
    Throttled,
    NotFound,
    Unsupported,
    Other,
}

impl ErrorKind {
    pub fn default_message(self) -> &'static str {
        match self {
            ErrorKind::Auth => "authentication failed",
            ErrorKind::Network => "network request failed",
            ErrorKind::Timeout => "request timed out",
            ErrorKind::Permission => "permission denied",
            ErrorKind::Throttled => "request throttled",
            ErrorKind::NotFound => "resource not found",
            ErrorKind::Unsupported => "operation not supported",
            ErrorKind::Other => "error",
        }
    }
}

/// A recovery panel: a heading, an optional note and an optional command the user can copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorHint {
    pub title: String,
    pub detail: Option<String>,
    pub command: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderError {
    pub kind: ErrorKind,
    pub message: String,
    pub advice: Option<String>,
    pub retry_after: Option<Duration>,
    #[serde(default)]
    pub hint: Option<Box<ErrorHint>>,
}

impl ProviderError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            advice: None,
            retry_after: None,
            hint: None,
        }
    }

    pub fn of(kind: ErrorKind) -> Self {
        Self::new(kind, kind.default_message())
    }

    pub fn unsupported() -> Self {
        Self::of(ErrorKind::Unsupported)
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ProviderError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn of_uses_default_message() {
        let error = ProviderError::of(ErrorKind::Network);
        assert_eq!(error.kind, ErrorKind::Network);
        assert_eq!(error.message, "network request failed");
        assert_eq!(error.advice, None);
        assert_eq!(error.retry_after, None);
    }

    #[test]
    fn new_keeps_message_and_display_shows_it() {
        let error = ProviderError::new(ErrorKind::Other, "boom");
        assert_eq!(error.kind, ErrorKind::Other);
        assert_eq!(error.to_string(), "boom");
    }

    #[test]
    fn unsupported_is_unsupported_kind() {
        let error = ProviderError::unsupported();
        assert_eq!(error.kind, ErrorKind::Unsupported);
        assert_eq!(error.to_string(), "operation not supported");
    }

    #[test]
    fn other_default_message_is_error() {
        assert_eq!(ErrorKind::Other.default_message(), "error");
    }
}
