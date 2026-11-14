use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

use crate::instance::{Instance, native_id};
use crate::service_provider::{LoadHost, REGION, cloud_deck_error, join_failure};
use crate::table::{Column, header_field};
use crate::{
    Call, CellFailure, CloudDeckError, CompiledQuery, ComputeServicePartialFailure, ConnectionId,
    FieldSource, KindId, ListResult, Provider, ProviderError, ProviderFacet, ProviderInfo,
    QueryError, SearchIndex, Selection, StateFacetCounts, merge_with_previous,
    provider_facet_counts, state_facet_counts_indexed, type_facet_counts_indexed,
};

/// Rust port of the Swift oracle's `InstanceListViewModel`
/// (`ViewModels/EC2/InstanceListViewModel.swift`).
///
/// Plain state + explicit update methods rather than `@Observable`: a gpui
/// view owns one of these and calls the mutating methods, re-rendering
/// itself after `load_instances`/`refresh` resolve. No gpui types are
/// referenced here. The search text is a query in the field language (see
/// `crate::compile`).
pub struct InstanceListViewModel {
    provider: Arc<dyn Provider>,
    connection: ConnectionId,
    selection: Selection,
    instances: Vec<Instance>,
    is_loading: bool,
    error: Option<CloudDeckError>,
    error_message: Option<String>,
    partial_failures: Vec<ComputeServicePartialFailure>,
    search_text: String,
    known_tag_keys: Vec<String>,
    search_index: SearchIndex,
    load_generation: u64,
    selected_state: String,
    selected_instance_type: String,
    selected_instance_id: Option<String>,
    stale_regions: Vec<String>,
    last_success: Option<SystemTime>,
    region_loaded_at: HashMap<String, SystemTime>,
    last_result: Option<ListResult>,
}

impl InstanceListViewModel {
    pub fn new(provider: Arc<dyn Provider>) -> Self {
        Self {
            provider,
            connection: ConnectionId::from("default"),
            selection: Selection::new(),
            instances: Vec::new(),
            is_loading: false,
            error: None,
            error_message: None,
            partial_failures: Vec::new(),
            search_text: String::new(),
            known_tag_keys: Vec::new(),
            search_index: SearchIndex::new(&[]),
            load_generation: 0,
            selected_state: String::new(),
            selected_instance_type: String::new(),
            selected_instance_id: None,
            stale_regions: Vec::new(),
            last_success: None,
            region_loaded_at: HashMap::new(),
            last_result: None,
        }
    }

    pub fn last_result(&self) -> Option<&ListResult> {
        self.last_result.as_ref()
    }

    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    /// Tag keys seen on the loaded instances.
    pub fn known_tag_keys(&self) -> &[String] {
        &self.known_tag_keys
    }

    /// Count of finished loads, changing whenever the loaded instances may have changed.
    pub fn load_generation(&self) -> u64 {
        self.load_generation
    }

    /// Regions whose rows are carried over from a previous load because they failed.
    pub fn stale_regions(&self) -> &[String] {
        &self.stale_regions
    }

    /// When the most recent successful load finished.
    pub fn last_success(&self) -> Option<SystemTime> {
        self.last_success
    }

    /// When the given region last returned rows in a successful load.
    #[cfg(test)]
    pub fn region_loaded_at(&self, region: &str) -> Option<SystemTime> {
        self.region_loaded_at.get(region).copied()
    }

    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// The classified error from the last failed load, if it has not been cleared.
    pub fn error(&self) -> Option<&CloudDeckError> {
        self.error.as_ref()
    }

    /// The first cell failure of the last load, when that load returned no rows.
    pub fn load_failure(&self) -> Option<&ProviderError> {
        let result = self.last_result.as_ref()?;
        if !result.resources.is_empty() {
            return None;
        }
        let cells = self.selection.values(REGION).len().max(1);
        if result.failures.len() < cells {
            return None;
        }
        result.failures.first().map(|failure| &failure.error)
    }

    // Not yet read by any view: the partial-failure banner in
    // `instance_list.rs` is not wired to a live per-region failure feed
    // yet (this bead's `default_compute_service()` never reports partial
    // failures). Kept as public ViewModel API for that follow-on wiring.
    #[allow(dead_code)]
    pub fn partial_failures(&self) -> &[ComputeServicePartialFailure] {
        &self.partial_failures
    }

    pub fn search_text(&self) -> &str {
        &self.search_text
    }

    /// Sets the search text and reconciles the current selection against the
    /// newly filtered list (Swift: `searchText`'s `didSet`). Called from
    /// `instance_list.rs`'s search `Input`'s change handler.
    pub fn set_search_text(&mut self, text: impl Into<String>) {
        self.search_text = text.into();
        self.reconcile_selection_with_filtered_instances();
    }

    #[cfg(test)]
    pub fn selected_state(&self) -> &str {
        &self.selected_state
    }

    /// Sets the state filter and reconciles the current selection (Swift:
    /// `selectedState`'s `didSet`).
    #[cfg(test)]
    pub fn set_selected_state(&mut self, state: impl Into<String>) {
        self.selected_state = state.into();
        self.reconcile_selection_with_filtered_instances();
    }

    #[cfg(test)]
    pub fn selected_instance_type(&self) -> &str {
        &self.selected_instance_type
    }

    /// Sets the instance-type filter and reconciles the current selection
    /// (Swift: `selectedInstanceType`'s `didSet`).
    #[cfg(test)]
    pub fn set_selected_instance_type(&mut self, instance_type: impl Into<String>) {
        self.selected_instance_type = instance_type.into();
        self.reconcile_selection_with_filtered_instances();
    }

    #[cfg(test)]
    pub fn selected_instance_id(&self) -> Option<&str> {
        self.selected_instance_id.as_deref()
    }

    /// Sets the selected instance ID directly; unlike the filter setters,
    /// Swift's `selectedInstanceID` has no `didSet` reconciliation.
    pub fn set_selected_instance_id(&mut self, instance_id: Option<String>) {
        self.selected_instance_id = instance_id;
    }

    /// Distinct, non-empty instance states, sorted case-insensitively.
    pub fn available_states(&self) -> Vec<String> {
        Self::distinct_sorted(self.instances.iter().map(|i| i.state.raw.clone()))
    }

    /// Distinct, non-empty instance types, sorted case-insensitively.
    pub fn available_instance_types(&self) -> Vec<String> {
        Self::distinct_sorted(
            self.instances
                .iter()
                .filter_map(|i| i.instance_type.clone()),
        )
    }

    pub fn provider_info(&self) -> ProviderInfo {
        self.provider.info()
    }

    pub fn kind_label(&self) -> String {
        self.provider
            .kinds()
            .into_iter()
            .find(|kind| kind.id == KindId::instances())
            .map_or_else(|| "Instances".to_string(), |kind| kind.label)
    }

    pub fn header(&self, column: Column) -> String {
        header_field(column)
            .and_then(|core| {
                self.provider
                    .fields(&KindId::instances())
                    .into_iter()
                    .find(|field| field.column && field.source == FieldSource::Core(core))
            })
            .map_or_else(|| column.title().to_string(), |field| field.label)
    }

    pub fn row_provider_id(&self) -> String {
        self.provider.info().id
    }

    pub fn provider_facet(&self) -> Vec<ProviderFacet> {
        let info = self.provider.info();
        let rows: Vec<(String, String)> = self
            .instances
            .iter()
            .map(|_| (info.id.clone(), info.name.clone()))
            .collect();
        provider_facet_counts(&rows)
    }

    /// The current instance list with search/state/type filters applied,
    /// sorted by instance ID case-insensitively.
    pub fn filtered_instances(&self) -> Vec<Instance> {
        let states = normalized_selection(&self.selected_state);
        let instance_types = normalized_selection(&self.selected_instance_type);
        let provider_id = self.row_provider_id();

        let mut filtered: Vec<Instance> = self
            .search_index
            .matching(
                Some(provider_id.as_str()),
                &self.compile_search(),
                &self.instances,
            )
            .into_iter()
            .filter(|instance| {
                states.is_empty() || states.contains(&instance.state.raw.to_lowercase())
            })
            .filter(|instance| {
                instance_types.is_empty()
                    || instance
                        .instance_type
                        .as_deref()
                        .is_some_and(|value| instance_types.contains(&value.to_lowercase()))
            })
            .cloned()
            .collect();
        filtered.sort_by(|a, b| {
            native_id(a)
                .to_lowercase()
                .cmp(&native_id(b).to_lowercase())
                .then_with(|| native_id(a).cmp(native_id(b)))
        });
        filtered
    }

    /// State facet counts for the search text over the loaded instances.
    pub fn state_facet_counts(&self, now: SystemTime) -> StateFacetCounts {
        state_facet_counts_indexed(
            &self.instances,
            &self.search_index,
            &self.search_text,
            &self.known_tag_keys,
            now,
        )
    }

    /// Type facet counts for the search text over the loaded instances.
    pub fn type_facet_counts(&self, now: SystemTime) -> Vec<(String, usize)> {
        type_facet_counts_indexed(
            &self.instances,
            &self.search_index,
            &self.search_text,
            &self.known_tag_keys,
            now,
        )
    }

    /// Errors for the terms of the search text that could not be interpreted.
    pub fn query_errors(&self) -> Vec<QueryError> {
        self.compile_search().errors().to_vec()
    }

    /// Number of search terms naming a field or tag with a value.
    pub fn recognised_filter_count(&self) -> usize {
        self.compile_search().recognised_field_count()
    }

    fn compile_search(&self) -> CompiledQuery {
        crate::compile(&self.search_text, &self.known_tag_keys, SystemTime::now())
    }

    /// The instance matching `selected_instance_id`, if any.
    pub fn selected_instance(&self) -> Option<&Instance> {
        let selected_id = self.selected_instance_id.as_deref()?;
        self.instances.iter().find(|i| i.id == selected_id)
    }

    /// A user-facing summary of partial failures, or `None` if there are
    /// none (Swift: `partialFailureSummary`).
    #[cfg(test)]
    pub fn partial_failure_summary(&self) -> Option<String> {
        if self.partial_failures.is_empty() {
            return None;
        }

        if self.partial_failures.len() == 1 {
            let failure = &self.partial_failures[0];
            return Some(format!("1 region failed: {}", failure.region));
        }

        let mut regions: Vec<&str> = self
            .partial_failures
            .iter()
            .map(|f| f.region.as_str())
            .collect();
        regions.sort();
        Some(format!(
            "{} regions failed: {}",
            self.partial_failures.len(),
            regions.join(", ")
        ))
    }

    /// Starts a load (Swift: `loadInstances()`). The returned request must be
    /// run off the async runtime, because `Provider::list` blocks.
    pub fn begin_load(&mut self) -> Option<LoadRequest> {
        if self.is_loading {
            return None;
        }
        self.is_loading = true;
        self.error = None;
        self.error_message = None;

        Some(LoadRequest {
            provider: self.provider.clone(),
            connection: self.connection.clone(),
            selection: self.selection.clone(),
        })
    }

    pub fn finish_load(&mut self, result: ListResult) {
        self.finish_load_at(result, SystemTime::now());
    }

    /// Like `finish_load`, recording `now` as the load's finish time.
    pub fn finish_load_at(&mut self, result: ListResult, now: SystemTime) {
        self.last_result = Some(result.clone());
        if let Some(error) = whole_load_error(&result, &self.selection) {
            let classified = cloud_deck_error(error);
            self.error_message = Some(classified.message_for("list instances"));
            self.error = Some(classified);
        } else {
            self.error = None;
            self.error_message = None;
            let partial_failures: Vec<ComputeServicePartialFailure> =
                result.failures.iter().map(partial_failure).collect();
            for instance in &result.resources {
                if let Some(region) = instance.region.as_deref() {
                    self.region_loaded_at.insert(region.to_string(), now);
                }
            }
            let merged = merge_with_previous(&self.instances, result.resources, &partial_failures);
            self.instances = merged.instances;
            self.search_index = SearchIndex::new(&self.instances);
            self.stale_regions = merged.stale_regions;
            self.known_tag_keys = crate::known_tag_keys(&self.instances);
            self.partial_failures = partial_failures;
            self.last_success = Some(now);
            self.reconcile_selected_filters();
            self.reconcile_selection_with_filtered_instances();
        }

        self.load_generation += 1;
        self.is_loading = false;
    }

    pub async fn load_instances(&mut self) {
        let Some(request) = self.begin_load() else {
            return;
        };
        let selection = request.selection.clone();
        let result = match tokio::task::spawn_blocking(move || request.run()).await {
            Ok(result) => result,
            Err(error) => join_failure(&selection, &error),
        };
        self.finish_load(result);
    }

    /// Reloads instances (Swift: `refresh()`, which just calls
    /// `loadInstances()`). Only called from tests: gpui `Entity` updates
    /// can't hold `&mut self` across an `.await`, so the click-to-async-spawn
    /// pattern used by the refresh button and `AppShell`'s startup load
    /// (`InstanceListView::refresh` in `instance_list.rs`) decomposes this
    /// into `begin_load()` (sync, before the spawn) + the service call
    /// (inside the spawned task) + `finish_load()` (sync, back on the
    /// entity) instead of awaiting this method directly. Kept as public
    /// ViewModel API for non-gpui/test callers that want the whole
    /// load in one `.await`.
    pub async fn refresh(&mut self) {
        self.load_instances().await;
    }

    /// Clears a filter that no longer matches any available value in the
    /// freshly loaded instance list (Swift:
    /// `reconcileSelectedFilters`).
    fn reconcile_selected_filters(&mut self) {
        if !self.selected_state.is_empty()
            && !self.available_states().contains(&self.selected_state)
        {
            self.selected_state = String::new();
        }

        if !self.selected_instance_type.is_empty()
            && !self
                .available_instance_types()
                .contains(&self.selected_instance_type)
        {
            self.selected_instance_type = String::new();
        }
    }

    /// Clears the selection if it no longer appears in the filtered list
    /// (Swift: `reconcileSelectionWithFilteredInstances`).
    fn reconcile_selection_with_filtered_instances(&mut self) {
        let Some(selected_id) = self.selected_instance_id.as_deref() else {
            return;
        };
        let filtered_ids: HashSet<String> = self
            .filtered_instances()
            .into_iter()
            .map(|i| i.id)
            .collect();
        if !filtered_ids.contains(selected_id) {
            self.selected_instance_id = None;
        }
    }

    /// Distinct, non-empty values sorted case-insensitively (Swift:
    /// `Set(...).sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }`).
    fn distinct_sorted(values: impl Iterator<Item = String>) -> Vec<String> {
        let distinct: HashSet<String> = values.filter(|v| !v.is_empty()).collect();
        let mut sorted: Vec<String> = distinct.into_iter().collect();
        sorted.sort_by(|a, b| {
            a.to_lowercase()
                .cmp(&b.to_lowercase())
                .then_with(|| a.cmp(b))
        });
        sorted
    }
}

pub struct LoadRequest {
    provider: Arc<dyn Provider>,
    connection: ConnectionId,
    selection: Selection,
}

impl LoadRequest {
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn connection(&self) -> &ConnectionId {
        &self.connection
    }

    pub fn run(self) -> ListResult {
        let host = LoadHost;
        let cx = Call {
            deadline: None,
            host: &host,
        };
        self.provider
            .list(&cx, &self.connection, &KindId::instances(), &self.selection)
    }
}

fn whole_load_error<'a>(
    result: &'a ListResult,
    selection: &Selection,
) -> Option<&'a ProviderError> {
    result
        .failures
        .iter()
        .find(|failure| {
            failure.cell.is_empty() || (&failure.cell == selection && result.resources.is_empty())
        })
        .map(|failure| &failure.error)
}

fn partial_failure(failure: &CellFailure) -> ComputeServicePartialFailure {
    ComputeServicePartialFailure {
        region: failure
            .cell
            .values(REGION)
            .first()
            .map(|value| value.as_str().to_string())
            .unwrap_or_default(),
        message: failure.error.message.clone(),
        error: cloud_deck_error(&failure.error),
    }
}

/// The selected filter value as a lowercase set; empty when nothing is selected.
fn normalized_selection(selected: &str) -> HashSet<String> {
    let value = selected.trim().to_lowercase();
    if value.is_empty() {
        HashSet::new()
    } else {
        HashSet::from([value])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComputeService;
    use crate::center_state::{CenterInput, CenterState, center_state};
    use crate::query::fixtures::bare_ec2;
    use crate::service_provider::{service_provider, whole_load_failure};
    use crate::{
        ComputeInstanceFilters, DirectoryScenarioCatalog, ScenarioService, TestScenarioKind,
    };
    use clouddeck_provider_api::{ErrorKind, ValueId};
    use std::time::{Duration, UNIX_EPOCH};

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../clouddeck-provider-kit/testdata/ec2-scenarios"
    );

    fn catalog() -> DirectoryScenarioCatalog {
        DirectoryScenarioCatalog::new(FIXTURES)
    }

    fn view_model_for(kind: TestScenarioKind) -> InstanceListViewModel {
        let resolved = ScenarioService::for_kind(kind, &catalog()).unwrap();
        InstanceListViewModel::new(service_provider(Box::new(resolved.into_service())))
    }

    #[tokio::test]
    async fn load_success_populates_instances_and_clears_loading() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);

        vm.load_instances().await;

        assert!(!vm.is_loading());
        assert!(vm.error_message().is_none());
        assert_eq!(vm.instances().len(), 3);
        assert!(vm.partial_failures().is_empty());
    }

    #[tokio::test]
    async fn load_success_with_empty_fleet_yields_empty_instances() {
        let mut vm = view_model_for(TestScenarioKind::EmptyFleet);

        vm.load_instances().await;

        assert!(!vm.is_loading());
        assert!(vm.error_message().is_none());
        assert!(vm.instances().is_empty());
    }

    #[test]
    fn load_failure_keeps_last_good_instances_and_sets_error_message() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        vm.finish_load_at(loaded(vec![("us-east-1", "a")], &[]), at(T1_SECS));
        vm.set_search_text("keep-me");
        vm.set_selected_instance_id(Some("us-east-1:a".into()));

        vm.begin_load();
        vm.finish_load_at(stub_failure(), at(T1_SECS + 60));

        assert!(!vm.is_loading());
        assert_eq!(loaded_ids(&vm), vec!["a"]);
        assert_eq!(vm.selected_instance_id(), Some("us-east-1:a"));
        let message = vm.error_message().expect("error message should be set");
        assert!(message.starts_with("Could not list instances:"));
        // Filter state itself is untouched by a failed load.
        assert_eq!(vm.search_text(), "keep-me");
    }

    #[tokio::test]
    async fn throttled_load_shows_throttled_message_and_no_instances() {
        let mut vm = view_model_for(TestScenarioKind::ThrottledFailure);
        vm.load_instances().await;
        assert!(vm.instances().is_empty());
        assert_eq!(vm.error(), Some(&CloudDeckError::Throttled));
        assert_eq!(
            vm.error_message(),
            Some(
                crate::CloudDeckError::Throttled
                    .message_for("list instances")
                    .as_str()
            )
        );
    }

    #[tokio::test]
    async fn permission_load_shows_permission_message_and_no_instances() {
        let mut vm = view_model_for(TestScenarioKind::PermissionFailure);
        vm.load_instances().await;
        assert!(vm.instances().is_empty());
        assert_eq!(vm.error(), Some(&CloudDeckError::Permission));
        assert_eq!(
            vm.error_message(),
            Some(
                crate::CloudDeckError::Permission
                    .message_for("list instances")
                    .as_str()
            )
        );
    }

    #[tokio::test]
    async fn multi_region_partial_failure_shows_healthy_regions_and_names_failed_region() {
        let mut vm = view_model_for(TestScenarioKind::MultiRegionPartialFailure);
        vm.load_instances().await;
        assert!(vm.error_message().is_none());
        assert_eq!(vm.instances().len(), 3);
        assert_eq!(vm.partial_failures().len(), 1);
        assert_eq!(
            vm.partial_failures()[0].region,
            crate::scenario_service::MULTI_REGION_FAILING_REGION
        );
        assert_eq!(
            vm.partial_failure_summary().as_deref(),
            Some("1 region failed: eu-west-1")
        );
    }

    #[tokio::test]
    async fn multi_region_all_throttled_surfaces_throttled_error_on_view_model() {
        let regions = vec!["us-east-1".to_string(), "eu-west-1".to_string()];
        let fleet = crate::FleetComputeService::new(regions, |_| {
            let resolved = ScenarioService::for_kind(
                TestScenarioKind::ThrottledFailure,
                &DirectoryScenarioCatalog::new(FIXTURES),
            )
            .unwrap();
            Box::new(resolved.into_service())
        });
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(fleet)));
        vm.load_instances().await;
        assert!(vm.instances().is_empty());
        assert_eq!(vm.error(), Some(&CloudDeckError::Throttled));
    }

    #[tokio::test]
    async fn multi_region_all_failing_errors_the_whole_load() {
        let regions = vec!["us-east-1".to_string(), "eu-west-1".to_string()];
        let fleet = crate::FleetComputeService::new(regions, |_| {
            let resolved = ScenarioService::for_kind(
                TestScenarioKind::ThrottledFailure,
                &DirectoryScenarioCatalog::new(FIXTURES),
            )
            .unwrap();
            Box::new(resolved.into_service())
        });
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(fleet)));
        vm.load_instances().await;
        assert!(vm.instances().is_empty());
        assert!(vm.partial_failures().is_empty());
        assert!(
            vm.error_message()
                .unwrap()
                .starts_with("Could not list instances:")
        );
    }

    #[tokio::test]
    async fn classified_load_error_is_not_reclassified_from_its_message() {
        struct AlreadyClassified;

        #[async_trait::async_trait]
        impl ComputeService for AlreadyClassified {
            async fn list_instances(
                &self,
                _filters: Option<ComputeInstanceFilters>,
            ) -> Result<crate::ComputeInstanceLoadResult, crate::BoxError> {
                Err(Box::new(CloudDeckError::Unknown(
                    "Could not run AWS CLI at /opt/auth/aws: boom".to_string(),
                )))
            }
        }

        let mut vm = InstanceListViewModel::new(service_provider(Box::new(AlreadyClassified)));
        vm.load_instances().await;
        assert_eq!(
            vm.error(),
            Some(&CloudDeckError::Unknown(
                "Could not run AWS CLI at /opt/auth/aws: boom".to_string()
            ))
        );
    }

    /// A service whose first call fails and whose subsequent calls succeed,
    /// used to exercise the failure-then-success load transition below.
    struct FailsOnceThenSucceeds {
        failed_once: std::sync::atomic::AtomicBool,
        result: crate::ComputeInstanceLoadResult,
    }

    #[derive(Debug)]
    struct StubError;

    impl std::fmt::Display for StubError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("stub failure")
        }
    }

    impl std::error::Error for StubError {}

    #[async_trait::async_trait]
    impl ComputeService for FailsOnceThenSucceeds {
        async fn list_instances(
            &self,
            _filters: Option<ComputeInstanceFilters>,
        ) -> Result<crate::ComputeInstanceLoadResult, crate::BoxError> {
            if !self
                .failed_once
                .swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                return Err(Box::new(StubError));
            }
            Ok(self.result.clone())
        }
    }

    #[tokio::test]
    async fn a_successful_load_after_a_prior_failure_clears_the_stale_error() {
        let mut vm =
            InstanceListViewModel::new(service_provider(Box::new(FailsOnceThenSucceeds {
                failed_once: std::sync::atomic::AtomicBool::new(false),
                result: load_result(vec![("us-east-1", "i-recovered")], &[]),
            })));

        vm.load_instances().await;
        assert!(vm.error_message().is_some(), "first load should fail");

        vm.load_instances().await;

        assert!(
            vm.error_message().is_none(),
            "a successful load must clear the stale error from the prior failure"
        );
        assert_eq!(vm.instances().len(), 1);
        assert_eq!(native_id(&vm.instances()[0]), "i-recovered");
    }

    #[tokio::test]
    async fn refresh_reloads_instances() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);

        vm.refresh().await;

        assert_eq!(vm.instances().len(), 3);
    }

    #[tokio::test]
    async fn provider_term_filters_rows_by_their_provider() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        assert_eq!(vm.row_provider_id(), "aws");
        vm.load_instances().await;
        let total = vm.instances().len();

        vm.set_search_text("provider:aws");
        assert_eq!(vm.filtered_instances().len(), total);
        vm.set_search_text("provider:AW");
        assert_eq!(vm.filtered_instances().len(), total);
        vm.set_search_text("provider:gcp");
        assert!(vm.filtered_instances().is_empty());
        vm.set_search_text("-provider:aws");
        assert!(vm.filtered_instances().is_empty());
    }

    #[tokio::test]
    async fn provider_facet_has_one_entry_once_rows_load() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        assert!(vm.provider_facet().is_empty());
        vm.load_instances().await;

        let facet = vm.provider_facet();
        assert_eq!(facet.len(), 1);
        assert_eq!(facet[0].id, "aws");
        assert_eq!(facet[0].name, "AWS");
        assert_eq!(facet[0].count, vm.instances().len());
    }

    #[tokio::test]
    async fn filtered_instances_applies_search_text_filter() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_search_text("bastion");
        let filtered = vm.filtered_instances();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name.as_deref(), Some("bastion-east-1"));
    }

    #[tokio::test]
    async fn filtered_instances_applies_state_filter() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_selected_state("running");
        let filtered = vm.filtered_instances();
        let ids: Vec<&str> = filtered.iter().map(native_id).collect();
        assert_eq!(ids, vec!["i-scenario-001", "i-scenario-003"]);
    }

    #[tokio::test]
    async fn filtered_instances_applies_instance_type_filter() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_selected_instance_type("t3.medium");
        let filtered = vm.filtered_instances();
        let ids: Vec<&str> = filtered.iter().map(native_id).collect();
        assert_eq!(ids, vec!["i-scenario-001"]);
    }

    #[tokio::test]
    async fn filtered_instances_ands_search_state_and_type_filters_together() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_search_text("bastion");
        vm.set_selected_state("running");
        vm.set_selected_instance_type("t3.small");
        let filtered = vm.filtered_instances();
        assert_eq!(filtered.len(), 1);
        assert_eq!(native_id(&filtered[0]), "i-scenario-003");

        // A constraint that matches nothing in combination excludes everything.
        vm.set_selected_instance_type("m6i.large");
        assert!(vm.filtered_instances().is_empty());
    }

    #[tokio::test]
    async fn filtered_instances_with_no_filters_returns_all_sorted_by_id() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        let filtered = vm.filtered_instances();
        let ids: Vec<&str> = filtered.iter().map(native_id).collect();
        let mut expected = ids.clone();
        expected.sort_by_key(|a| a.to_lowercase());
        assert_eq!(ids, expected);
        assert_eq!(filtered.len(), 3);
    }

    fn filtered_ids(vm: &InstanceListViewModel) -> Vec<String> {
        vm.filtered_instances()
            .into_iter()
            .map(|i| native_id(&i).to_string())
            .collect()
    }

    #[tokio::test]
    async fn search_text_field_filters_match_instances() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_search_text("state:stopped");
        assert_eq!(filtered_ids(&vm), vec!["i-scenario-002"]);

        vm.set_search_text("-state:stopped");
        assert_eq!(filtered_ids(&vm), vec!["i-scenario-001", "i-scenario-003"]);

        vm.set_search_text("service:api");
        assert_eq!(filtered_ids(&vm), vec!["i-scenario-001"]);

        vm.set_search_text("environment:prod");
        assert_eq!(filtered_ids(&vm), vec!["i-scenario-001", "i-scenario-002"]);

        vm.set_search_text("type:t3");
        assert_eq!(filtered_ids(&vm), vec!["i-scenario-001", "i-scenario-003"]);
    }

    #[tokio::test]
    async fn search_text_plain_words_match_name_and_security_groups() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_search_text("web");
        assert_eq!(filtered_ids(&vm), vec!["i-scenario-001"]);

        vm.set_search_text("edge");
        assert_eq!(filtered_ids(&vm), vec!["i-scenario-003"]);
    }

    #[tokio::test]
    async fn search_text_unknown_field_matches_nothing_and_reports_one_error() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_search_text("colour:x");
        assert!(filtered_ids(&vm).is_empty());
        assert_eq!(vm.query_errors().len(), 1);
    }

    #[tokio::test]
    async fn search_text_combines_with_state_and_type_filters() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_search_text("state:running");
        vm.set_selected_instance_type("t3.small");
        assert_eq!(filtered_ids(&vm), vec!["i-scenario-003"]);
    }

    #[tokio::test]
    async fn recognised_filter_count_and_query_errors_describe_search_text() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_search_text("state:running service:api web");
        assert_eq!(vm.recognised_filter_count(), 2);
        assert!(vm.query_errors().is_empty());

        vm.set_search_text("");
        assert_eq!(vm.recognised_filter_count(), 0);
        assert!(vm.query_errors().is_empty());
    }

    #[tokio::test]
    async fn available_states_and_types_are_distinct_and_sorted() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        let states = vm.available_states();
        let mut sorted_states = states.clone();
        sorted_states.sort_by_key(|a| a.to_lowercase());
        assert_eq!(states, sorted_states);
        assert_eq!(
            states.iter().collect::<HashSet<_>>().len(),
            states.len(),
            "available_states should be distinct"
        );

        let types = vm.available_instance_types();
        assert_eq!(
            types.iter().collect::<HashSet<_>>().len(),
            types.len(),
            "available_instance_types should be distinct"
        );
    }

    #[tokio::test]
    async fn selecting_a_state_not_in_available_states_after_reload_is_cleared() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;
        vm.set_selected_state("this-state-does-not-exist");

        // A subsequent load reconciles filters against the freshly loaded
        // instance list (Swift: reconcileSelectedFilters runs post-load, not
        // on the setter itself).
        vm.load_instances().await;

        assert_eq!(vm.selected_state(), "");
    }

    #[tokio::test]
    async fn selecting_an_instance_type_not_in_available_types_after_reload_is_cleared() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;
        vm.set_selected_instance_type("this-type-does-not-exist");

        vm.load_instances().await;

        assert_eq!(vm.selected_instance_type(), "");
    }

    #[tokio::test]
    async fn setting_search_text_clears_selection_no_longer_in_filtered_list() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;
        let first_id = vm.instances()[0].id.clone();
        vm.set_selected_instance_id(Some(first_id.clone()));
        assert_eq!(vm.selected_instance_id(), Some(first_id.as_str()));

        vm.set_search_text("no-instance-matches-this-query");

        assert_eq!(vm.selected_instance_id(), None);
    }

    #[tokio::test]
    async fn setting_selected_state_clears_selection_no_longer_in_filtered_list() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;
        // i-scenario-002 is "stopped"; filtering to "running" should exclude it.
        let stopped_id = vm
            .instances()
            .iter()
            .find(|i| native_id(i) == "i-scenario-002")
            .unwrap()
            .id
            .clone();
        vm.set_selected_instance_id(Some(stopped_id.clone()));
        assert_eq!(vm.selected_instance_id(), Some(stopped_id.as_str()));

        vm.set_selected_state("running");

        assert_eq!(vm.selected_instance_id(), None);
    }

    #[tokio::test]
    async fn setting_selected_instance_type_clears_selection_no_longer_in_filtered_list() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;
        // i-scenario-002 is "m6i.large"; filtering to "t3.medium" should exclude it.
        let large_id = vm
            .instances()
            .iter()
            .find(|i| native_id(i) == "i-scenario-002")
            .unwrap()
            .id
            .clone();
        vm.set_selected_instance_id(Some(large_id.clone()));
        assert_eq!(vm.selected_instance_id(), Some(large_id.as_str()));

        vm.set_selected_instance_type("t3.medium");

        assert_eq!(vm.selected_instance_id(), None);
    }

    #[tokio::test]
    async fn selected_instance_resolves_by_id() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;
        let target = vm.instances()[0].clone();
        vm.set_selected_instance_id(Some(target.id.clone()));

        assert_eq!(vm.selected_instance(), Some(&target));
    }

    #[tokio::test]
    async fn selected_instance_is_none_without_selection() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        assert_eq!(vm.selected_instance(), None);
    }

    #[test]
    fn partial_failure_summary_is_none_when_empty() {
        let vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        assert_eq!(vm.partial_failure_summary(), None);
    }

    #[tokio::test]
    async fn partial_failure_summary_singular_and_plural() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;
        vm.partial_failures = vec![ComputeServicePartialFailure {
            region: "us-west-2".to_string(),
            message: "boom".to_string(),
            error: CloudDeckError::Unknown("boom".to_string()),
        }];
        assert_eq!(
            vm.partial_failure_summary(),
            Some("1 region failed: us-west-2".to_string())
        );

        vm.partial_failures = vec![
            ComputeServicePartialFailure {
                region: "us-west-2".to_string(),
                message: "boom".to_string(),
                error: CloudDeckError::Unknown("boom".to_string()),
            },
            ComputeServicePartialFailure {
                region: "eu-west-1".to_string(),
                message: "boom".to_string(),
                error: CloudDeckError::Unknown("boom".to_string()),
            },
        ];
        assert_eq!(
            vm.partial_failure_summary(),
            Some("2 regions failed: eu-west-1, us-west-2".to_string())
        );
    }

    /// 2026-10-08T12:01:00Z as seconds since the Unix epoch.
    const T1_SECS: u64 = 1_791_460_860;
    /// 2026-10-08T12:04:00Z as seconds since the Unix epoch.
    const T2_SECS: u64 = 1_791_461_040;

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    struct Canned(crate::ComputeInstanceLoadResult);

    #[async_trait::async_trait]
    impl ComputeService for Canned {
        async fn list_instances(
            &self,
            _filters: Option<ComputeInstanceFilters>,
        ) -> Result<crate::ComputeInstanceLoadResult, crate::BoxError> {
            Ok(self.0.clone())
        }
    }

    fn loaded(records: Vec<(&str, &str)>, failed_regions: &[&str]) -> ListResult {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(Canned(load_result(
            records,
            failed_regions,
        )))));
        vm.begin_load().expect("load starts").run()
    }

    fn stub_failure() -> ListResult {
        whole_load_failure(
            &CloudDeckError::from_service_error(&StubError),
            &Selection::new(),
        )
    }

    fn load_result(
        records: Vec<(&str, &str)>,
        failed_regions: &[&str],
    ) -> crate::ComputeInstanceLoadResult {
        crate::ComputeInstanceLoadResult {
            instances: records
                .into_iter()
                .map(|(region, instance_id)| bare_ec2(region, instance_id))
                .collect(),
            partial_failures: failed_regions
                .iter()
                .map(|region| ComputeServicePartialFailure {
                    region: region.to_string(),
                    message: "request failed".to_string(),
                    error: CloudDeckError::Unknown("request failed".to_string()),
                })
                .collect(),
        }
    }

    fn loaded_ids(vm: &InstanceListViewModel) -> Vec<&str> {
        vm.instances().iter().map(native_id).collect()
    }

    #[test]
    fn failed_region_keeps_previous_rows_and_is_marked_stale_with_load_times() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        vm.finish_load_at(
            loaded(vec![("us-east-1", "a"), ("eu-central-1", "b")], &[]),
            at(T1_SECS),
        );

        vm.finish_load_at(
            loaded(vec![("us-east-1", "a")], &["eu-central-1"]),
            at(T2_SECS),
        );

        assert_eq!(loaded_ids(&vm), vec!["a", "b"]);
        assert_eq!(vm.stale_regions(), ["eu-central-1".to_string()]);
        assert_eq!(vm.last_success(), Some(at(T2_SECS)));
        assert_eq!(vm.region_loaded_at("us-east-1"), Some(at(T2_SECS)));
        assert_eq!(vm.region_loaded_at("eu-central-1"), Some(at(T1_SECS)));
    }

    #[test]
    fn recovered_region_clears_stale_marker_and_refreshes_its_load_time() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        vm.finish_load_at(
            loaded(vec![("us-east-1", "a"), ("eu-central-1", "b")], &[]),
            at(T1_SECS),
        );
        vm.finish_load_at(
            loaded(vec![("us-east-1", "a")], &["eu-central-1"]),
            at(T2_SECS),
        );

        vm.finish_load_at(
            loaded(vec![("us-east-1", "a"), ("eu-central-1", "b")], &[]),
            at(T2_SECS),
        );

        assert!(vm.stale_regions().is_empty());
        assert!(vm.partial_failures().is_empty());
        assert_eq!(vm.region_loaded_at("eu-central-1"), Some(at(T2_SECS)));
    }

    #[test]
    fn failed_load_keeps_rows_stale_regions_and_last_success() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        vm.finish_load_at(
            loaded(vec![("us-east-1", "a"), ("eu-central-1", "b")], &[]),
            at(T1_SECS),
        );
        vm.finish_load_at(
            loaded(vec![("us-east-1", "a")], &["eu-central-1"]),
            at(T2_SECS),
        );

        vm.begin_load();
        vm.finish_load_at(stub_failure(), at(T2_SECS + 60));

        assert_eq!(loaded_ids(&vm), vec!["a", "b"]);
        assert_eq!(vm.stale_regions(), ["eu-central-1".to_string()]);
        assert_eq!(vm.last_success(), Some(at(T2_SECS)));
        assert_eq!(vm.partial_failures().len(), 1);
        assert_eq!(vm.partial_failures()[0].region, "eu-central-1");
    }

    #[test]
    fn finish_load_without_time_records_a_last_success() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));

        vm.finish_load(loaded(vec![("us-east-1", "a")], &[]));

        assert!(vm.last_success().is_some());
    }

    #[test]
    fn whole_load_rule_other_region_failure_with_zero_resources_is_partial() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        let result = ListResult {
            resources: Vec::new(),
            failures: vec![CellFailure {
                cell: Selection::new().with("region", vec![ValueId::from("eu-west-1")]),
                error: ProviderError::new(ErrorKind::Throttled, "eu-west-1 is throttled"),
            }],
        };

        vm.finish_load(result);

        assert_eq!(vm.error_message(), None);
        assert_eq!(vm.partial_failures().len(), 1);
        assert_eq!(vm.partial_failures()[0].region, "eu-west-1");
    }

    #[test]
    fn kind_label_comes_from_the_provider() {
        let aws = InstanceListViewModel::new(Arc::new(crate::ComputeServiceProvider::new(
            "aws",
            vec![],
            vec![],
            |_, _| Box::new(crate::MockComputeService::new(vec![], None)),
        )));
        assert_eq!(aws.kind_label(), "EC2 Instances");

        let generic = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        assert_eq!(generic.kind_label(), "Instances");
    }

    #[test]
    fn single_provider_headers_without_field_labels_keep_column_titles() {
        let vm = InstanceListViewModel::new(Arc::new(crate::ComputeServiceProvider::new(
            "aws",
            vec![],
            vec![],
            |_, _| Box::new(crate::MockComputeService::new(vec![], None)),
        )));
        assert_eq!(vm.header(Column::InstanceId), "Instance ID");
        assert_eq!(vm.header(Column::Az), "AZ");
        assert_eq!(vm.header(Column::PublicIp), "Public IP");
    }

    #[test]
    fn load_failure_is_the_first_cell_failure_when_no_rows_came_back() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        let failure = ProviderError::new(ErrorKind::Throttled, "eu-west-1 is throttled");
        vm.finish_load(ListResult {
            resources: Vec::new(),
            failures: vec![CellFailure {
                cell: Selection::new().with("region", vec![ValueId::from("eu-west-1")]),
                error: failure.clone(),
            }],
        });

        assert_eq!(vm.load_failure(), Some(&failure));
    }

    #[test]
    fn load_failure_is_none_when_a_cell_returned_rows() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        vm.finish_load(ListResult {
            resources: vec![Instance::from(&bare_ec2("us-east-1", "i-1"))],
            failures: vec![CellFailure {
                cell: Selection::new().with("region", vec![ValueId::from("eu-west-1")]),
                error: ProviderError::new(ErrorKind::Throttled, "eu-west-1 is throttled"),
            }],
        });

        assert_eq!(vm.load_failure(), None);
    }

    #[test]
    fn load_failure_is_none_when_another_selected_cell_loaded_empty() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        vm.selection = Selection::new().with(
            "region",
            vec![ValueId::from("us-east-1"), ValueId::from("eu-west-1")],
        );
        vm.finish_load(ListResult {
            resources: Vec::new(),
            failures: vec![CellFailure {
                cell: Selection::new().with("region", vec![ValueId::from("eu-west-1")]),
                error: ProviderError::new(ErrorKind::Throttled, "eu-west-1 is throttled"),
            }],
        });

        assert_eq!(vm.load_failure(), None);
    }

    #[test]
    fn partial_cell_failure_with_no_rows_shows_the_failure_state() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        vm.finish_load(ListResult {
            resources: Vec::new(),
            failures: vec![CellFailure {
                cell: Selection::new().with("region", vec![ValueId::from("eu-west-1")]),
                error: ProviderError {
                    advice: Some("Try another region.".to_string()),
                    ..ProviderError::new(ErrorKind::Throttled, "eu-west-1 is throttled")
                },
            }],
        });

        let input = CenterInput {
            is_loading: false,
            loaded_count: vm.instances().len(),
            shown_count: 0,
            error: vm.error().cloned(),
            failure: vm.load_failure().cloned(),
        };
        assert_eq!(
            center_state(&input),
            Some(CenterState::Error {
                message: "eu-west-1 is throttled".to_string(),
                advice: Some("Try another region.".to_string()),
            })
        );
    }

    #[test]
    fn whole_load_rule_failure_on_selection_cell_is_whole_load() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        vm.finish_load(whole_load_failure(
            &CloudDeckError::Throttled,
            &Selection::new(),
        ));

        assert!(vm.error_message().is_some());
        assert!(vm.partial_failures().is_empty());
    }

    #[test]
    fn whole_call_failure_with_empty_cell_is_whole_load_not_a_blank_region() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        vm.selection = Selection::new().with("region", vec![ValueId::from("eu-west-1")]);
        let result = ListResult {
            resources: Vec::new(),
            failures: vec![CellFailure {
                cell: Selection::new(),
                error: ProviderError::new(ErrorKind::Network, "API unreachable"),
            }],
        };

        vm.finish_load(result);

        assert!(vm.error_message().is_some());
        assert!(vm.partial_failures().is_empty());
        assert_eq!(vm.partial_failure_summary(), None);
    }

    #[test]
    fn selected_region_failure_alongside_rows_stays_a_partial_failure() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        let selection = Selection::new().with("region", vec![ValueId::from("eu-west-1")]);
        vm.selection = selection.clone();
        let mut result = loaded(vec![("eu-west-1", "a")], &[]);
        result.failures.push(CellFailure {
            cell: selection,
            error: ProviderError::new(ErrorKind::Throttled, "eu-west-1 is throttled"),
        });

        vm.finish_load(result);

        assert_eq!(vm.error_message(), None);
        assert_eq!(vm.partial_failures().len(), 1);
        assert_eq!(vm.partial_failures()[0].region, "eu-west-1");
    }

    #[test]
    fn whole_call_failure_alongside_resources_keeps_previous_rows_and_sets_error() {
        let mut vm = InstanceListViewModel::new(service_provider(Box::new(
            crate::MockComputeService::new(vec![], None),
        )));
        vm.finish_load_at(loaded(vec![("us-east-1", "a")], &[]), at(T1_SECS));

        let mut result = loaded(vec![("us-east-1", "b")], &[]);
        result.failures.push(CellFailure {
            cell: Selection::new(),
            error: ProviderError::new(ErrorKind::Auth, "credentials expired"),
        });
        vm.finish_load_at(result, at(T2_SECS));

        assert!(vm.error_message().is_some());
        assert!(vm.partial_failures().is_empty());
        assert_eq!(loaded_ids(&vm), vec!["a"]);
        assert_eq!(vm.last_success(), Some(at(T1_SECS)));
    }
}
