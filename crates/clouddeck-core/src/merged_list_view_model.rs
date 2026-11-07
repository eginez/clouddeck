//! The All-providers list: one `InstanceListViewModel` per provider, merged into one row set.

use std::collections::{BTreeMap, BTreeSet};
use std::time::SystemTime;

use crate::table::{Column, column_header, compare_provider};
use crate::{
    CloudDeckError, ComputeServicePartialFailure, ErrorKind, Instance, InstanceListViewModel,
    ListResult, LoadRequest, ProviderInfo, QueryError, StateFacetCounts,
};

/// A merged row: the instance and the id of the provider it was listed from.
pub type ProviderRow = (String, Instance);

/// A provider whose rows are missing: its last load failed, or it needs a sign-in first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderFailure {
    pub provider_id: String,
    pub provider_name: String,
    pub message: String,
    pub needs_sign_in: bool,
}

pub struct MergedListViewModel {
    members: Vec<InstanceListViewModel>,
    unavailable: Vec<ProviderFailure>,
    search_text: String,
}

impl MergedListViewModel {
    pub fn new(members: Vec<InstanceListViewModel>) -> Self {
        Self {
            members,
            unavailable: Vec::new(),
            search_text: String::new(),
        }
    }

    /// Providers with no member to load, such as ones that are signed out or could not be set up.
    pub fn with_unavailable(mut self, failures: Vec<ProviderFailure>) -> Self {
        self.unavailable = failures;
        self
    }

    /// Whether more than one provider is in the view, which switches the headers to the shared titles.
    pub fn is_mixed(&self) -> bool {
        self.members.len() > 1
    }

    pub fn header(&self, column: Column) -> &'static str {
        column_header(column, self.is_mixed())
    }

    pub fn search_text(&self) -> &str {
        &self.search_text
    }

    pub fn query_errors(&self) -> Vec<QueryError> {
        crate::compile(&self.search_text, &self.known_tag_keys(), SystemTime::now())
            .errors()
            .to_vec()
    }

    pub fn set_search_text(&mut self, text: &str) {
        self.search_text = text.to_string();
        for member in &mut self.members {
            member.set_search_text(text);
        }
    }

    pub fn set_selected_instance_id(&mut self, instance_id: Option<String>) {
        for member in &mut self.members {
            member.set_selected_instance_id(instance_id.clone());
        }
    }

    pub fn provider_failures(&self) -> Vec<ProviderFailure> {
        let mut failures: Vec<ProviderFailure> = self
            .members
            .iter()
            .filter_map(|member| {
                let (message, needs_sign_in) = member_failure(member)?;
                let info = member.provider_info();
                Some(ProviderFailure {
                    provider_id: info.id,
                    provider_name: info.name,
                    message,
                    needs_sign_in,
                })
            })
            .collect();
        failures.extend(self.unavailable.iter().cloned());
        failures
    }

    /// Display names of every provider in the view, including ones that are signed out or failed.
    pub fn provider_names(&self) -> BTreeMap<String, String> {
        self.members
            .iter()
            .map(|member| {
                let info = member.provider_info();
                (info.id, info.name)
            })
            .chain(
                self.unavailable
                    .iter()
                    .map(|failure| (failure.provider_id.clone(), failure.provider_name.clone())),
            )
            .collect()
    }

    /// Filtered rows of every provider, ordered by provider name and then by instance ID.
    /// A provider whose latest load failed still contributes the rows from its last successful load.
    pub fn rows(&self) -> Vec<ProviderRow> {
        let mut members: Vec<(ProviderInfo, &InstanceListViewModel)> = self
            .members
            .iter()
            .map(|member| (member.provider_info(), member))
            .collect();
        members.sort_by(|(a, _), (b, _)| compare_provider(a, b).then_with(|| a.id.cmp(&b.id)));
        members
            .into_iter()
            .flat_map(|(info, member)| {
                member
                    .filtered_instances()
                    .into_iter()
                    .map(move |instance| (info.id.clone(), instance))
            })
            .collect()
    }

    /// Every loaded instance of every provider, unfiltered.
    pub fn instances(&self) -> Vec<Instance> {
        self.members
            .iter()
            .flat_map(|member| member.instances().iter().cloned())
            .collect()
    }

    pub fn is_loading(&self) -> bool {
        self.members.iter().any(|member| member.is_loading())
    }

    pub fn last_success(&self) -> Option<SystemTime> {
        self.members
            .iter()
            .filter_map(|member| member.last_success())
            .max()
    }

    pub fn load_generation(&self) -> u64 {
        self.members
            .iter()
            .map(|member| member.load_generation())
            .sum()
    }

    pub fn partial_failures(&self) -> Vec<ComputeServicePartialFailure> {
        self.members
            .iter()
            .flat_map(|member| member.partial_failures().iter().cloned())
            .collect()
    }

    pub fn stale_regions(&self) -> Vec<String> {
        self.members
            .iter()
            .flat_map(|member| member.stale_regions().iter().cloned())
            .collect()
    }

    pub fn known_tag_keys(&self) -> Vec<String> {
        self.members
            .iter()
            .flat_map(|member| member.known_tag_keys().iter().cloned())
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect()
    }

    pub fn recognised_filter_count(&self) -> usize {
        self.members
            .first()
            .map_or(0, |member| member.recognised_filter_count())
    }

    pub fn state_facet_counts(&self, now: SystemTime) -> StateFacetCounts {
        self.members
            .iter()
            .map(|member| member.state_facet_counts(now))
            .fold(StateFacetCounts::default(), |total, counts| {
                StateFacetCounts {
                    all: total.all + counts.all,
                    running: total.running + counts.running,
                    stopped: total.stopped + counts.stopped,
                    pending: total.pending + counts.pending,
                }
            })
    }

    pub fn type_facet_counts(&self, now: SystemTime) -> Vec<(String, usize)> {
        let mut totals: BTreeMap<String, usize> = BTreeMap::new();
        for member in &self.members {
            for (name, count) in member.type_facet_counts(now) {
                *totals.entry(name).or_default() += count;
            }
        }
        totals.into_iter().collect()
    }

    /// Starts a load for every member that is not already loading, keyed by provider id.
    pub fn begin_loads(&mut self) -> Vec<(String, LoadRequest)> {
        self.members
            .iter_mut()
            .filter_map(|member| {
                let id = member.provider_info().id;
                member.begin_load().map(|request| (id, request))
            })
            .collect()
    }

    pub fn finish_load_for(&mut self, provider_id: &str, result: ListResult, now: SystemTime) {
        if let Some(member) = self
            .members
            .iter_mut()
            .find(|member| member.provider_info().id == provider_id)
        {
            member.finish_load_at(result, now);
        }
    }
}

/// A member's failure: its whole-load error, or a last load that returned only failures.
fn member_failure(member: &InstanceListViewModel) -> Option<(String, bool)> {
    if let Some(message) = member.error_message() {
        return Some((
            message.to_string(),
            matches!(member.error(), Some(CloudDeckError::Auth)),
        ));
    }
    let result = member.last_result()?;
    if !result.resources.is_empty() {
        return None;
    }
    let failure = result.failures.first()?;
    Some((
        failure.error.message.clone(),
        failure.error.kind == ErrorKind::Auth,
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::query::fixtures::bare_ec2;
    use crate::service_provider::{service_provider, whole_load_failure};
    use crate::{
        BoxError, Call, CloudDeckError, Connection, ConnectionId, Dimension, FieldDef, KindId,
        KindInfo, ListResult, MockComputeService, Provider, ProviderError, Selection, native_id,
    };

    /// Wraps a provider so it reports a different id, since the mock service always reports "aws".
    struct Named {
        id: &'static str,
        inner: Arc<dyn Provider>,
    }

    impl Provider for Named {
        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: self.id.to_string(),
                name: self.id.to_uppercase(),
                version: "1.0.0".to_string(),
                connection_label: "Profile".to_string(),
            }
        }

        fn connections(&self, cx: &Call<'_>) -> Result<Vec<Connection>, ProviderError> {
            self.inner.connections(cx)
        }

        fn dimensions(
            &self,
            cx: &Call<'_>,
            connection: &ConnectionId,
        ) -> Result<Vec<Dimension>, ProviderError> {
            self.inner.dimensions(cx, connection)
        }

        fn kinds(&self) -> Vec<KindInfo> {
            self.inner.kinds()
        }

        fn list(
            &self,
            cx: &Call<'_>,
            connection: &ConnectionId,
            kind: &KindId,
            selection: &Selection,
        ) -> ListResult {
            self.inner.list(cx, connection, kind, selection)
        }

        fn console_url(
            &self,
            connection: &ConnectionId,
            kind: &KindId,
            resource: &Instance,
        ) -> Option<String> {
            self.inner.console_url(connection, kind, resource)
        }

        fn fields(&self, kind: &KindId) -> Vec<FieldDef> {
            self.inner.fields(kind)
        }
    }

    fn member_with(id: &'static str, service: MockComputeService) -> InstanceListViewModel {
        let mut vm = InstanceListViewModel::new(Arc::new(Named {
            id,
            inner: service_provider(Box::new(service)),
        }));
        let result = vm.begin_load().expect("load starts").run();
        vm.finish_load(result);
        vm
    }

    fn member(id: &'static str, records: &[(&str, &str)]) -> InstanceListViewModel {
        let service = MockComputeService::new(
            records
                .iter()
                .map(|(region, id)| bare_ec2(region, id))
                .collect(),
            None,
        );
        member_with(id, service)
    }

    fn failing(id: &'static str) -> InstanceListViewModel {
        let error: BoxError = Box::new(std::io::Error::other("provider is down"));
        member_with(id, MockComputeService::new(Vec::new(), Some(error)))
    }

    fn row_ids(merged: &MergedListViewModel) -> Vec<(String, String)> {
        merged
            .rows()
            .into_iter()
            .map(|(provider, instance)| (provider, native_id(&instance).to_string()))
            .collect()
    }

    fn pairs(values: &[(&str, &str)]) -> Vec<(String, String)> {
        values
            .iter()
            .map(|(provider, id)| (provider.to_string(), id.to_string()))
            .collect()
    }

    #[test]
    fn a_failing_provider_keeps_its_error_and_does_not_hide_other_rows() {
        let merged = MergedListViewModel::new(vec![
            member("aws", &[("us-east-1", "i-1"), ("us-east-1", "i-2")]),
            failing("gcp"),
        ]);

        assert_eq!(row_ids(&merged), pairs(&[("aws", "i-1"), ("aws", "i-2")]));
        let failures = merged.provider_failures();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].provider_id, "gcp");
        assert_eq!(failures[0].provider_name, "GCP");
        assert!(failures[0].message.starts_with("Could not list instances:"));
    }

    #[test]
    fn a_provider_that_fails_after_loading_keeps_its_last_rows_and_reports_the_error() {
        let mut gcp = member("gcp", &[("us-central1", "i-9")]);
        gcp.finish_load(whole_load_failure(
            &CloudDeckError::Throttled,
            &Selection::new(),
        ));
        let merged = MergedListViewModel::new(vec![member("aws", &[("us-east-1", "i-1")]), gcp]);

        assert_eq!(row_ids(&merged), pairs(&[("aws", "i-1"), ("gcp", "i-9")]));
        let failures = merged.provider_failures();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].provider_id, "gcp");
    }

    #[test]
    fn rows_are_ordered_by_provider_then_instance_id_and_stamped_with_their_provider() {
        let merged = MergedListViewModel::new(vec![
            member("gcp", &[("us-central1", "i-0")]),
            member("aws", &[("us-east-1", "i-2"), ("us-east-1", "i-1")]),
        ]);

        assert_eq!(
            row_ids(&merged),
            pairs(&[("aws", "i-1"), ("aws", "i-2"), ("gcp", "i-0")])
        );
    }

    #[test]
    fn provider_term_filters_rows_by_their_provider_id() {
        let mut merged = MergedListViewModel::new(vec![
            member("aws", &[("us-east-1", "i-1")]),
            member("gcp", &[("us-central1", "i-2")]),
        ]);

        merged.set_search_text("provider:gcp");
        assert_eq!(row_ids(&merged), pairs(&[("gcp", "i-2")]));

        merged.set_search_text("-provider:gcp");
        assert_eq!(row_ids(&merged), pairs(&[("aws", "i-1")]));
    }

    #[test]
    fn mixed_view_uses_shared_headers_and_single_provider_view_uses_its_own() {
        let mixed = MergedListViewModel::new(vec![member("aws", &[]), member("gcp", &[])]);
        assert!(mixed.is_mixed());
        let single = MergedListViewModel::new(vec![member("aws", &[])]);
        assert!(!single.is_mixed());

        for column in Column::ALL {
            assert_eq!(mixed.header(column), column_header(column, true));
            assert_eq!(single.header(column), column_header(column, false));
        }
    }
}
