use std::fmt;

use crate::instance::Instance;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scope {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    Auth,
    Network,
    Timeout,
    Permission,
    Throttled,
    NotFound,
    Unsupported,
    Other(String),
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::Auth => f.write_str("authentication failed"),
            ProviderError::Network => f.write_str("network request failed"),
            ProviderError::Timeout => f.write_str("request timed out"),
            ProviderError::Permission => f.write_str("permission denied"),
            ProviderError::Throttled => f.write_str("request throttled"),
            ProviderError::NotFound => f.write_str("resource not found"),
            ProviderError::Unsupported => f.write_str("operation not supported"),
            ProviderError::Other(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ProviderError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeFailure {
    pub scope: String,
    pub message: String,
    pub error: ProviderError,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListResult {
    pub instances: Vec<Instance>,
    pub failures: Vec<ScopeFailure>,
}

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

#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    pub label: String,
    /// Text shown next to the key in query suggestions.
    pub hint: String,
    pub kind: FieldKind,
    pub match_mode: MatchMode,
    /// Usable as a `name:value` query key.
    pub query: bool,
    /// Getter values are part of free-text search.
    pub search: bool,
    pub facet: bool,
    pub column: bool,
    /// Returns several values; a term matches when any one of them matches.
    pub getter: fn(&Instance) -> Vec<String>,
}

pub trait Provider: Send + Sync {
    fn id(&self) -> String;
    fn connections(&self) -> Result<Vec<Connection>, ProviderError>;
    fn scopes(&self, connection: &str) -> Result<Vec<Scope>, ProviderError>;
    fn list(&self, connection: &str, scopes: &[String]) -> ListResult;

    fn sign_in(&self, _connection: &str) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported)
    }

    fn console_url(&self, instance: &Instance) -> Option<String>;
    fn fields(&self) -> Vec<FieldDef>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct EmptyProvider;

    impl Provider for EmptyProvider {
        fn id(&self) -> String {
            "empty".to_string()
        }

        fn connections(&self) -> Result<Vec<Connection>, ProviderError> {
            Ok(vec![])
        }

        fn scopes(&self, _connection: &str) -> Result<Vec<Scope>, ProviderError> {
            Ok(vec![])
        }

        fn list(&self, _connection: &str, _scopes: &[String]) -> ListResult {
            ListResult::default()
        }

        fn console_url(&self, _instance: &Instance) -> Option<String> {
            None
        }

        fn fields(&self) -> Vec<FieldDef> {
            vec![]
        }
    }

    fn assert_send_sync<T: Send + Sync + ?Sized>() {}

    #[test]
    fn provider_is_object_safe_behind_box_and_arc() {
        let boxed: Box<dyn Provider> = Box::new(EmptyProvider);
        assert_eq!(boxed.id(), "empty");
        assert_eq!(boxed.list("default", &[]), ListResult::default());

        let shared: Arc<dyn Provider> = Arc::new(EmptyProvider);
        assert_eq!(shared.connections(), Ok(vec![]));
    }

    #[test]
    fn dyn_provider_is_send_and_sync() {
        assert_send_sync::<dyn Provider>();
    }

    #[test]
    fn default_sign_in_is_unsupported() {
        assert_eq!(
            EmptyProvider.sign_in("default"),
            Err(ProviderError::Unsupported)
        );
    }
}
