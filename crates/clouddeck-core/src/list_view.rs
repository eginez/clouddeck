use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use clouddeck_provider_api::AuthStatus;
use clouddeck_provider_kit::scoped_provider::{ScopeSettings, scoped_provider};

use crate::service_provider::{LoadHost, REGION};
use crate::{
    Call, CloudDeckError, ComputeServicePartialFailure, DemoFleet, ErrorKind, Instance,
    InstanceListViewModel, ListResult, LoadRequest, MergedListViewModel, Provider, ProviderError,
    ProviderFacet, ProviderFailure, ProviderInfo, ProviderRow, QueryError, StateFacetCounts,
    default_selection,
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

/// The All-providers list: each provider that is signed in, on its default connection and selection.
///
/// A provider that is signed out is listed as needing sign-in, and one whose connections or
/// dimensions cannot be read becomes a failed member, so neither hides the other providers' rows.
pub fn all_providers_view(
    providers: &[Arc<dyn Provider>],
    demo: Option<DemoFleet>,
    request_timeout: Duration,
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
        if matches!(
            provider.auth_status(&call, None),
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
        let connections = match provider.connections(&call) {
            Ok(connections) => connections,
            Err(error) => {
                unavailable.push(setup_failure(info, error));
                continue;
            }
        };
        let Some(connection) = connections.first().map(|connection| connection.id.clone()) else {
            unavailable.push(ProviderFailure {
                message: format!("{} has no connections to list.", info.name),
                provider_id: info.id,
                provider_name: info.name,
                needs_sign_in: false,
            });
            continue;
        };
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

fn setup_failure(info: ProviderInfo, error: ProviderError) -> ProviderFailure {
    ProviderFailure {
        needs_sign_in: error.kind == ErrorKind::Auth,
        message: error.message,
        provider_id: info.id,
        provider_name: info.name,
    }
}
