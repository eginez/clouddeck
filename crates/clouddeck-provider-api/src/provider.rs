use crate::error::ProviderError;
use crate::field::FieldDef;
use crate::inspector::InspectorSectionDef;
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

pub trait Provider: Send + Sync {
    fn id(&self) -> String;
    fn connections(&self) -> Result<Vec<Connection>, ProviderError>;
    fn scopes(&self, connection: &str) -> Result<Vec<Scope>, ProviderError>;
    fn list(&self, connection: &str, scopes: &[String]) -> ListResult;

    fn sign_in(&self, _connection: &str) -> Result<(), ProviderError> {
        Err(ProviderError::unsupported())
    }

    fn console_url(&self, instance: &Instance) -> Option<String>;
    fn fields(&self) -> Vec<FieldDef>;

    fn inspector(&self) -> Vec<InspectorSectionDef> {
        Vec::new()
    }
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
    fn default_inspector_is_empty() {
        assert!(EmptyProvider.inspector().is_empty());
    }

    #[test]
    fn default_sign_in_is_unsupported() {
        assert_eq!(
            EmptyProvider.sign_in("default"),
            Err(ProviderError::unsupported())
        );
    }
}
