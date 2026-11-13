use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use clouddeck_provider_api::{AuthStatus, NoAuth};
use clouddeck_provider_kit::scoped_provider::{ScopeSettings, scoped_provider};

use crate::service_provider::{LoadHost, REGION};
use crate::table::Column;
use crate::{
    Call, CloudDeckError, ComputeServicePartialFailure, Connection, ConnectionId, DemoFleet,
    ErrorKind, Instance, InstanceListViewModel, ListResult, LoadRequest, MergedListViewModel,
    Provider, ProviderError, ProviderFacet, ProviderFailure, ProviderInfo, ProviderRow, QueryError,
    StateFacetCounts, WorkspaceContext, default_selection,
};

/// The instance list for the active provider, or for every signed-in provider at once.
pub enum ListView {
    Single(Box<InstanceListViewModel>),
    All(MergedListViewModel),
}

impl From<InstanceListViewModel> for ListView {
    fn from(view_model: InstanceListViewModel) -> Self {
        Self::Single(Box::new(view_model))
    }
}

impl ListView {
    pub fn is_all(&self) -> bool {
        matches!(self, Self::All(_))
    }

    /// The single-provider list, for operations that load one provider directly.
    pub fn single_mut(&mut self) -> Option<&mut InstanceListViewModel> {
        match self {
            Self::Single(view_model) => Some(&mut **view_model),
            Self::All(_) => None,
        }
    }

    pub fn instances(&self) -> Cow<'_, [Instance]> {
        match self {
            Self::Single(view_model) => Cow::Borrowed(view_model.instances()),
            Self::All(merged) => Cow::Owned(merged.instances()),
        }
    }

    pub fn is_loading(&self) -> bool {
        match self {
            Self::Single(view_model) => view_model.is_loading(),
            Self::All(merged) => merged.is_loading(),
        }
    }

    /// The whole-load error of a single provider; an All view reports failures per provider instead.
    pub fn error(&self) -> Option<&CloudDeckError> {
        match self {
            Self::Single(view_model) => view_model.error(),
            Self::All(_) => None,
        }
    }

    pub fn header(&self, column: Column) -> String {
        match self {
            Self::Single(view_model) => view_model.header(column),
            Self::All(merged) => merged.header(column).to_string(),
        }
    }

    pub fn kind_label(&self) -> String {
        match self {
            Self::Single(view_model) => view_model.kind_label(),
            Self::All(_) => "Instances".to_string(),
        }
    }

    pub fn load_failure(&self) -> Option<&ProviderError> {
        match self {
            Self::Single(view_model) => view_model.load_failure(),
            Self::All(_) => None,
        }
    }

    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Single(view_model) => view_model.error_message(),
            Self::All(_) => None,
        }
    }

    pub fn query_errors(&self) -> Vec<QueryError> {
        match self {
            Self::Single(view_model) => view_model.query_errors(),
            Self::All(merged) => merged.query_errors(),
        }
    }

    pub fn last_result(&self) -> Option<&ListResult> {
        match self {
            Self::Single(view_model) => view_model.last_result(),
            Self::All(_) => None,
        }
    }

    pub fn last_success(&self) -> Option<SystemTime> {
        match self {
            Self::Single(view_model) => view_model.last_success(),
            Self::All(merged) => merged.last_success(),
        }
    }

    pub fn load_generation(&self) -> u64 {
        match self {
            Self::Single(view_model) => view_model.load_generation(),
            Self::All(merged) => merged.load_generation(),
        }
    }

    pub fn partial_failures(&self) -> Vec<ComputeServicePartialFailure> {
        match self {
            Self::Single(view_model) => view_model.partial_failures().to_vec(),
            Self::All(merged) => merged.partial_failures(),
        }
    }

    pub fn stale_regions(&self) -> Vec<String> {
        match self {
            Self::Single(view_model) => view_model.stale_regions().to_vec(),
            Self::All(merged) => merged.stale_regions(),
        }
    }

    pub fn known_tag_keys(&self) -> Cow<'_, [String]> {
        match self {
            Self::Single(view_model) => Cow::Borrowed(view_model.known_tag_keys()),
            Self::All(merged) => Cow::Owned(merged.known_tag_keys()),
        }
    }

    pub fn search_text(&self) -> &str {
        match self {
            Self::Single(view_model) => view_model.search_text(),
            Self::All(merged) => merged.search_text(),
        }
    }

    pub fn set_search_text(&mut self, text: &str) {
        match self {
            Self::Single(view_model) => view_model.set_search_text(text),
            Self::All(merged) => merged.set_search_text(text),
        }
    }

    pub fn set_selected_instance_id(&mut self, instance_id: Option<String>) {
        match self {
            Self::Single(view_model) => view_model.set_selected_instance_id(instance_id),
            Self::All(merged) => merged.set_selected_instance_id(instance_id),
        }
    }

    pub fn recognised_filter_count(&self) -> usize {
        match self {
            Self::Single(view_model) => view_model.recognised_filter_count(),
            Self::All(merged) => merged.recognised_filter_count(),
        }
    }

    pub fn state_facet_counts(&self, now: SystemTime) -> StateFacetCounts {
        match self {
            Self::Single(view_model) => view_model.state_facet_counts(now),
            Self::All(merged) => merged.state_facet_counts(now),
        }
    }

    pub fn type_facet_counts(&self, now: SystemTime) -> Vec<(String, usize)> {
        match self {
            Self::Single(view_model) => view_model.type_facet_counts(now),
            Self::All(merged) => merged.type_facet_counts(now),
        }
    }

    /// Filtered rows, each stamped with the id of the provider it was listed from.
    pub fn rows(&self) -> Vec<ProviderRow> {
        match self {
            Self::Single(view_model) => {
                let provider_id = view_model.row_provider_id();
                view_model
                    .filtered_instances()
                    .into_iter()
                    .map(|instance| (provider_id.clone(), instance))
                    .collect()
            }
            Self::All(merged) => merged.rows(),
        }
    }

    pub fn provider_failures(&self) -> Vec<ProviderFailure> {
        match self {
            Self::Single(view_model) => {
                let Some(message) = view_model.error_message() else {
                    return Vec::new();
                };
                let info = view_model.provider_info();
                vec![ProviderFailure {
                    provider_id: info.id,
                    provider_name: info.name,
                    message: message.to_string(),
                    needs_sign_in: matches!(view_model.error(), Some(CloudDeckError::Auth)),
                }]
            }
            Self::All(merged) => merged.provider_failures(),
        }
    }

    pub fn provider_facet(&self) -> Vec<ProviderFacet> {
        match self {
            Self::Single(view_model) => view_model.provider_facet(),
            Self::All(merged) => merged.provider_facet(),
        }
    }

    pub fn provider_names(&self) -> BTreeMap<String, String> {
        match self {
            Self::Single(view_model) => {
                let info: ProviderInfo = view_model.provider_info();
                BTreeMap::from([(info.id, info.name)])
            }
            Self::All(merged) => merged.provider_names(),
        }
    }

    /// Starts the loads this view needs, keyed by provider id; empty when they are already running.
    pub fn begin_loads(&mut self) -> Vec<(String, LoadRequest)> {
        match self {
            Self::Single(view_model) => {
                let id = view_model.provider_info().id;
                view_model
                    .begin_load()
                    .map(|request| (id, request))
                    .into_iter()
                    .collect()
            }
            Self::All(merged) => merged.begin_loads(),
        }
    }

    pub fn finish_load_for(&mut self, provider_id: &str, result: ListResult, now: SystemTime) {
        match self {
            Self::Single(view_model) => view_model.finish_load_at(result, now),
            Self::All(merged) => merged.finish_load_for(provider_id, result, now),
        }
    }
}

/// The All-providers list: each provider that is signed in, on its saved or first connection.
///
/// The saved context's connection is used for the provider it belongs to. A provider that is
/// signed out is listed as needing sign-in, and one whose connections or dimensions cannot be
/// read becomes a failed member, so neither hides the other providers' rows.
pub fn all_providers_view(
    providers: &[Arc<dyn Provider>],
    demo: Option<DemoFleet>,
    request_timeout: Duration,
    saved: &WorkspaceContext,
) -> ListView {
    let call = Call {
        deadline: None,
        host: &LoadHost,
    };
    let mut members = Vec::new();
    let mut unavailable = Vec::new();
    for provider in providers
        .iter()
        .filter(|provider| (provider.info().id == "demo") == demo.is_some())
    {
        let info = provider.info();
        let connections = match provider.connections(&call) {
            Ok(connections) => connections,
            Err(error) => {
                unavailable.push(setup_failure(info, error));
                continue;
            }
        };
        let Some(connection) = preferred_connection(&info.id, saved, &connections) else {
            unavailable.push(ProviderFailure {
                message: format!("{} has no connections to list.", info.name),
                provider_id: info.id,
                provider_name: info.name,
                needs_sign_in: false,
            });
            continue;
        };
        if matches!(
            provider
                .auth()
                .unwrap_or(&NoAuth)
                .auth_status(&call, Some(&connection)),
            AuthStatus::SignedOut | AuthStatus::Expired
        ) {
            unavailable.push(ProviderFailure {
                message: format!("Sign in to {} to list its instances.", info.name),
                provider_id: info.id,
                provider_name: info.name,
                needs_sign_in: true,
            });
            continue;
        }
        let selection = match provider.dimensions(&call, &connection) {
            Ok(dimensions) => default_selection(&dimensions),
            Err(error) => {
                unavailable.push(setup_failure(info, error));
                continue;
            }
        };
        let settings = ScopeSettings {
            request_timeout,
            fleet: selection.values(REGION).len() > 1,
            demo,
        };
        members.push(InstanceListViewModel::new(scoped_provider(
            provider.clone(),
            connection,
            selection,
            settings,
        )));
    }
    ListView::All(MergedListViewModel::new(members).with_unavailable(unavailable))
}

/// Whether the provider reports a usable session, as the picker and sign-in state see it.
pub fn provider_signed_in(provider: &dyn Provider, connection: Option<&ConnectionId>) -> bool {
    let call = Call {
        deadline: None,
        host: &LoadHost,
    };
    matches!(
        provider
            .auth()
            .unwrap_or(&NoAuth)
            .auth_status(&call, connection),
        AuthStatus::SignedIn { .. }
    )
}

fn preferred_connection(
    provider_id: &str,
    saved: &WorkspaceContext,
    connections: &[Connection],
) -> Option<ConnectionId> {
    let saved_match = (saved.provider == provider_id)
        .then(|| {
            connections
                .iter()
                .find(|connection| connection.id.as_str() == saved.profile)
        })
        .flatten();
    saved_match
        .or_else(|| connections.first())
        .map(|connection| connection.id.clone())
}

fn setup_failure(info: ProviderInfo, error: ProviderError) -> ProviderFailure {
    ProviderFailure {
        needs_sign_in: error.kind == ErrorKind::Auth,
        message: error.message,
        provider_id: info.id,
        provider_name: info.name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct SignedInOnConnection {
        inner: crate::ComputeServiceProvider,
        connection: &'static str,
    }

    impl Provider for SignedInOnConnection {
        fn info(&self) -> ProviderInfo {
            self.inner.info()
        }

        fn auth(&self) -> Option<&dyn clouddeck_provider_api::Auth> {
            Some(self)
        }
    }

    impl clouddeck_provider_api::Auth for SignedInOnConnection {
        fn auth_status(&self, _cx: &Call<'_>, connection: Option<&ConnectionId>) -> AuthStatus {
            if connection.is_some_and(|id| id.as_str() == self.connection) {
                AuthStatus::SignedIn { expires: None }
            } else {
                AuthStatus::SignedOut
            }
        }
    }

    impl clouddeck_provider_api::Browse for SignedInOnConnection {
        fn connections(&self, cx: &Call<'_>) -> Result<Vec<crate::Connection>, ProviderError> {
            self.inner.connections(cx)
        }

        fn dimensions(
            &self,
            cx: &Call<'_>,
            connection: &ConnectionId,
        ) -> Result<Vec<crate::Dimension>, ProviderError> {
            self.inner.dimensions(cx, connection)
        }

        fn kinds(&self) -> Vec<crate::KindInfo> {
            self.inner.kinds()
        }

        fn list(
            &self,
            cx: &Call<'_>,
            connection: &ConnectionId,
            kind: &clouddeck_provider_api::KindId,
            selection: &clouddeck_provider_api::Selection,
        ) -> ListResult {
            self.inner.list(cx, connection, kind, selection)
        }

        fn console_url(
            &self,
            connection: &ConnectionId,
            kind: &clouddeck_provider_api::KindId,
            resource: &Instance,
        ) -> Option<String> {
            self.inner.console_url(connection, kind, resource)
        }

        fn fields(&self, kind: &clouddeck_provider_api::KindId) -> Vec<crate::FieldDef> {
            self.inner.fields(kind)
        }
    }

    fn connection(id: &str) -> crate::Connection {
        crate::Connection {
            id: id.into(),
            display_name: id.to_string(),
        }
    }

    fn service_provider_with(
        id: &str,
        connections: Vec<crate::Connection>,
    ) -> crate::ComputeServiceProvider {
        crate::ComputeServiceProvider::new(
            id,
            connections,
            vec!["us-east-1".to_string()],
            |_, _| Box::new(crate::MockComputeService::new(vec![], None)),
        )
    }

    #[test]
    fn all_view_checks_each_provider_on_the_saved_connection() {
        let aws = SignedInOnConnection {
            inner: service_provider_with(
                "aws",
                vec![connection("demo-a"), connection("local-dev")],
            ),
            connection: "local-dev",
        };
        let gcp =
            service_provider_with("gcp", vec![connection("demo-data"), connection("demo-web")]);
        let providers: Vec<Arc<dyn Provider>> = vec![Arc::new(aws), Arc::new(gcp)];
        let saved = WorkspaceContext {
            provider: "aws".into(),
            profile: "local-dev".into(),
        };

        let view = all_providers_view(&providers, None, Duration::from_secs(5), &saved);

        assert!(view.provider_failures().is_empty());
        let names: Vec<String> = view.provider_names().into_keys().collect();
        assert_eq!(names, ["aws", "gcp"]);
    }

    #[test]
    fn provider_signed_out_on_its_saved_connection_needs_sign_in() {
        let aws = SignedInOnConnection {
            inner: service_provider_with("aws", vec![connection("demo-a")]),
            connection: "local-dev",
        };
        let providers: Vec<Arc<dyn Provider>> = vec![Arc::new(aws)];
        let saved = WorkspaceContext {
            provider: "aws".into(),
            profile: "missing".into(),
        };

        let view = all_providers_view(&providers, None, Duration::from_secs(5), &saved);

        let failures = view.provider_failures();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].provider_id, "aws");
        assert!(failures[0].needs_sign_in);
    }

    #[test]
    fn provider_without_auth_counts_as_signed_in() {
        let provider = crate::ComputeServiceProvider::new("aws", vec![], vec![], |_, _| {
            Box::new(crate::MockComputeService::new(vec![], None))
        });
        assert!(provider_signed_in(&provider, None));
    }
}
