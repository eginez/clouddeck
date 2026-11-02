use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

use clouddeck_provider_api::{
    Call, CellFailure, Connection, ConnectionId, Dimension, ErrorKind, FieldDef, Host, INSTANCES,
    InspectorSectionDef, Instance, KindId, KindInfo, ListResult, LogLevel, Provider, ProviderError,
    ProviderInfo, Selection, ValueId,
};

use crate::console_url::console_url_for_instance;
use crate::error::CloudDeckError;
use crate::service::{BoxError, ComputeService};

pub(crate) const REGION: &str = "region";

pub fn service_provider(service: Box<dyn ComputeService + Send + Sync>) -> Arc<dyn Provider> {
    Arc::new(ServiceProvider { service })
}

pub fn join_failure(selection: &Selection, error: &tokio::task::JoinError) -> ListResult {
    whole_load_failure(
        &CloudDeckError::Unknown(format!("the load task stopped unexpectedly: {error}")),
        selection,
    )
}

pub fn whole_load_failure(error: &CloudDeckError, selection: &Selection) -> ListResult {
    ListResult {
        resources: Vec::new(),
        failures: vec![CellFailure {
            cell: selection.clone(),
            error: ProviderError {
                message: error.user_message(),
                ..ProviderError::from(error.clone())
            },
        }],
    }
}

pub fn cloud_deck_error(error: &ProviderError) -> CloudDeckError {
    match error.kind {
        ErrorKind::Auth => CloudDeckError::Auth,
        ErrorKind::Network => CloudDeckError::Network,
        ErrorKind::Timeout => CloudDeckError::Timeout,
        ErrorKind::Permission => CloudDeckError::Permission,
        ErrorKind::Throttled => CloudDeckError::Throttled,
        ErrorKind::NotFound => CloudDeckError::NotFound,
        ErrorKind::Other | ErrorKind::Unsupported => CloudDeckError::Unknown(error.message.clone()),
    }
}

pub(crate) fn classify(error: &BoxError) -> CloudDeckError {
    error
        .downcast_ref::<CloudDeckError>()
        .cloned()
        .unwrap_or_else(|| CloudDeckError::from_service_error(error.as_ref()))
}

pub(crate) fn info() -> ProviderInfo {
    ProviderInfo {
        id: "aws".to_string(),
        name: "AWS".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        connection_label: "Profile".to_string(),
    }
}

pub(crate) fn connections() -> Vec<Connection> {
    vec![Connection {
        id: "default".into(),
        display_name: "default".to_string(),
    }]
}

pub(crate) fn kinds() -> Vec<KindInfo> {
    vec![KindInfo {
        id: KindId::instances(),
        label: "Instances".to_string(),
    }]
}

pub(crate) fn console_url(kind: &KindId, resource: &Instance) -> Option<String> {
    if kind.as_str() != INSTANCES {
        return None;
    }
    let instance_id = resource.id.rsplit(':').next().unwrap_or_default();
    console_url_for_instance(resource.region.as_deref().unwrap_or(""), instance_id).ok()
}

pub(crate) fn fields(kind: &KindId) -> Vec<FieldDef> {
    if kind.as_str() == INSTANCES {
        crate::aws_fields()
    } else {
        Vec::new()
    }
}

pub(crate) fn inspector(kind: &KindId) -> Vec<InspectorSectionDef> {
    if kind.as_str() == INSTANCES {
        crate::aws_inspector::aws_inspector()
    } else {
        Vec::new()
    }
}

pub(crate) fn unsupported_kind(kind: &KindId, selection: &Selection) -> Option<ListResult> {
    (kind.as_str() != INSTANCES).then(|| ListResult {
        resources: Vec::new(),
        failures: vec![CellFailure {
            cell: selection.clone(),
            error: ProviderError::unsupported(),
        }],
    })
}

pub struct LoadHost;

impl Host for LoadHost {
    fn log(&self, _level: LogLevel, _message: &str) {}
    fn data_dir(&self) -> PathBuf {
        PathBuf::new()
    }
    fn setting(&self, _key: &str) -> Option<String> {
        None
    }
    fn secret(&self, _key: &str) -> Option<String> {
        None
    }
    fn set_secret(&self, _key: &str, _value: Option<&str>) -> Result<(), ProviderError> {
        Ok(())
    }
    fn open_url(&self, _url: &str) -> Result<(), ProviderError> {
        Ok(())
    }
    fn cancelled(&self) -> bool {
        false
    }
}

struct ServiceProvider {
    service: Box<dyn ComputeService + Send + Sync>,
}

fn run_blocking<F: Future>(future: F) -> Result<F::Output, CloudDeckError> {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => Ok(handle.block_on(future)),
        Err(_) => tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map(|runtime| runtime.block_on(future))
            .map_err(|error| {
                CloudDeckError::Unknown(format!("could not start a runtime: {error}"))
            }),
    }
}

pub(crate) fn region_cell(region: &str) -> Selection {
    Selection::new().with(REGION, vec![ValueId::from(region)])
}

impl Provider for ServiceProvider {
    fn info(&self) -> ProviderInfo {
        info()
    }

    fn connections(&self, _cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
        Ok(connections())
    }

    fn dimensions(
        &self,
        _cx: &Call<'_>,
        _connection: &ConnectionId,
    ) -> Result<Vec<Dimension>, ProviderError> {
        Ok(Vec::new())
    }

    fn kinds(&self) -> Vec<KindInfo> {
        kinds()
    }

    fn list(
        &self,
        _cx: &Call<'_>,
        _connection: &ConnectionId,
        kind: &KindId,
        selection: &Selection,
    ) -> ListResult {
        if let Some(unsupported) = unsupported_kind(kind, selection) {
            return unsupported;
        }

        match run_blocking(self.service.list_instances(None)) {
            Ok(Ok(result)) => ListResult {
                resources: result.instances.iter().map(Instance::from).collect(),
                failures: result
                    .partial_failures
                    .into_iter()
                    .map(|failure| CellFailure {
                        cell: region_cell(&failure.region),
                        error: ProviderError {
                            message: failure.message,
                            ..ProviderError::from(failure.error)
                        },
                    })
                    .collect(),
            },
            Ok(Err(error)) => whole_load_failure(&classify(&error), selection),
            Err(error) => whole_load_failure(&error, selection),
        }
    }

    fn console_url(
        &self,
        _connection: &ConnectionId,
        kind: &KindId,
        resource: &Instance,
    ) -> Option<String> {
        console_url(kind, resource)
    }

    fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
        fields(kind)
    }

    fn inspector(&self, kind: &KindId) -> Vec<InspectorSectionDef> {
        inspector(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockComputeService;
    use crate::query::fixtures::bare_ec2;
    use crate::{ComputeInstanceFilters, ComputeInstanceLoadResult, ComputeServicePartialFailure};

    fn with_call<T>(f: impl FnOnce(&Call<'_>) -> T) -> T {
        let host = LoadHost;
        f(&Call {
            deadline: None,
            host: &host,
        })
    }

    fn list(provider: &dyn Provider, kind: &KindId, selection: &Selection) -> ListResult {
        with_call(|cx| provider.list(cx, &ConnectionId::from("default"), kind, selection))
    }

    struct PartialService;

    #[async_trait::async_trait]
    impl ComputeService for PartialService {
        async fn list_instances(
            &self,
            _filters: Option<ComputeInstanceFilters>,
        ) -> Result<ComputeInstanceLoadResult, BoxError> {
            Ok(ComputeInstanceLoadResult {
                instances: vec![bare_ec2("us-east-1", "i-1")],
                partial_failures: vec![ComputeServicePartialFailure {
                    region: "eu-west-1".to_string(),
                    message: "eu-west-1 is throttled".to_string(),
                    error: CloudDeckError::Throttled,
                }],
            })
        }
    }

    struct FailingService(CloudDeckError);

    #[async_trait::async_trait]
    impl ComputeService for FailingService {
        async fn list_instances(
            &self,
            _filters: Option<ComputeInstanceFilters>,
        ) -> Result<ComputeInstanceLoadResult, BoxError> {
            Err(Box::new(self.0.clone()))
        }
    }

    #[test]
    fn cloud_deck_error_maps_every_kind() {
        let cases = [
            (ErrorKind::Auth, CloudDeckError::Auth),
            (ErrorKind::Network, CloudDeckError::Network),
            (ErrorKind::Timeout, CloudDeckError::Timeout),
            (ErrorKind::Permission, CloudDeckError::Permission),
            (ErrorKind::Throttled, CloudDeckError::Throttled),
            (ErrorKind::NotFound, CloudDeckError::NotFound),
            (
                ErrorKind::Other,
                CloudDeckError::Unknown("boom".to_string()),
            ),
            (
                ErrorKind::Unsupported,
                CloudDeckError::Unknown("boom".to_string()),
            ),
        ];
        for (kind, expected) in cases {
            assert_eq!(
                cloud_deck_error(&ProviderError::new(kind, "boom")),
                expected
            );
        }
    }

    #[test]
    fn whole_load_failure_carries_user_message_on_the_selection_cell() {
        let selection = region_cell("eu-west-1");
        let result = whole_load_failure(&CloudDeckError::Throttled, &selection);
        assert!(result.resources.is_empty());
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].cell, selection);
        assert_eq!(result.failures[0].error.kind, ErrorKind::Throttled);
        assert_eq!(
            result.failures[0].error.message,
            CloudDeckError::Throttled.user_message()
        );
    }

    #[test]
    fn list_keeps_instance_regions_and_maps_partial_failure() {
        let provider = service_provider(Box::new(PartialService));
        let result = list(provider.as_ref(), &KindId::instances(), &Selection::new());
        assert_eq!(result.resources.len(), 1);
        assert_eq!(result.resources[0].region.as_deref(), Some("us-east-1"));
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].cell, region_cell("eu-west-1"));
        assert_eq!(result.failures[0].error.kind, ErrorKind::Throttled);
        assert_eq!(result.failures[0].error.message, "eu-west-1 is throttled");
    }

    #[test]
    fn list_success_without_failures_has_no_cells() {
        let service = MockComputeService::new(vec![bare_ec2("us-east-1", "i-1")], None);
        let provider = service_provider(Box::new(service));
        let result = list(provider.as_ref(), &KindId::instances(), &Selection::new());
        assert_eq!(result.resources.len(), 1);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn list_error_becomes_whole_load_failure_on_the_selection() {
        let provider = service_provider(Box::new(FailingService(CloudDeckError::Permission)));
        let selection = Selection::new();
        let result = list(provider.as_ref(), &KindId::instances(), &selection);
        assert!(result.resources.is_empty());
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].cell, selection);
        assert_eq!(result.failures[0].error.kind, ErrorKind::Permission);
    }

    #[test]
    fn list_unclassified_error_is_classified_from_its_message() {
        let provider = service_provider(Box::new(FailingService(CloudDeckError::Unknown(
            "Could not run AWS CLI at /opt/auth/aws: boom".to_string(),
        ))));
        let result = list(provider.as_ref(), &KindId::instances(), &Selection::new());
        assert_eq!(
            result.failures[0].error.message,
            "Could not run AWS CLI at /opt/auth/aws: boom"
        );
    }

    #[test]
    fn unsupported_kind_fails_the_cell() {
        let provider = service_provider(Box::new(FailingService(CloudDeckError::Permission)));
        let selection = region_cell("us-east-1");
        let result = list(provider.as_ref(), &KindId::from("services"), &selection);
        assert!(result.resources.is_empty());
        assert_eq!(result.failures[0].error, ProviderError::unsupported());
    }

    #[test]
    fn join_failure_becomes_whole_load_failure_on_the_selection() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("runtime");
        let handle = runtime.spawn_blocking(|| panic!("boom"));
        let error = runtime.block_on(handle).expect_err("task panicked");
        let selection = Selection::new();
        let result = join_failure(&selection, &error);
        assert!(result.resources.is_empty());
        assert_eq!(result.failures[0].cell, selection);
    }

    #[test]
    fn load_host_is_a_quiet_no_op() {
        let host = LoadHost;
        host.log(LogLevel::Info, "ignored");
        assert_eq!(host.setting("k"), None);
        assert_eq!(host.secret("k"), None);
        assert_eq!(host.set_secret("k", Some("v")), Ok(()));
        assert_eq!(host.open_url("https://example.invalid"), Ok(()));
        assert!(!host.cancelled());
    }
}
