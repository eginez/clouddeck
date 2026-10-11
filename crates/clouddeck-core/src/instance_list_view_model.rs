use std::collections::{HashMap, HashSet};
use std::time::SystemTime;

use crate::{
    CloudDeckError, CompiledQuery, ComputeInstanceFilters, ComputeService,
    ComputeServicePartialFailure, EC2Instance, QueryError, merge_with_previous,
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
    service: std::sync::Arc<dyn ComputeService + Send + Sync>,
    instances: Vec<EC2Instance>,
    is_loading: bool,
    error: Option<CloudDeckError>,
    error_message: Option<String>,
    partial_failures: Vec<ComputeServicePartialFailure>,
    search_text: String,
    known_tag_keys: Vec<String>,
    selected_state: String,
    selected_instance_type: String,
    selected_instance_id: Option<String>,
    stale_regions: Vec<String>,
    last_success: Option<SystemTime>,
    region_loaded_at: HashMap<String, SystemTime>,
}

impl InstanceListViewModel {
    pub fn new(service: Box<dyn ComputeService + Send + Sync>) -> Self {
        Self {
            service: service.into(),
            instances: Vec::new(),
            is_loading: false,
            error: None,
            error_message: None,
            partial_failures: Vec::new(),
            search_text: String::new(),
            known_tag_keys: Vec::new(),
            selected_state: String::new(),
            selected_instance_type: String::new(),
            selected_instance_id: None,
            stale_regions: Vec::new(),
            last_success: None,
            region_loaded_at: HashMap::new(),
        }
    }

    pub fn instances(&self) -> &[EC2Instance] {
        &self.instances
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

    pub fn selected_state(&self) -> &str {
        &self.selected_state
    }

    /// Sets the state filter and reconciles the current selection (Swift:
    /// `selectedState`'s `didSet`).
    pub fn set_selected_state(&mut self, state: impl Into<String>) {
        self.selected_state = state.into();
        self.reconcile_selection_with_filtered_instances();
    }

    pub fn selected_instance_type(&self) -> &str {
        &self.selected_instance_type
    }

    /// Sets the instance-type filter and reconciles the current selection
    /// (Swift: `selectedInstanceType`'s `didSet`).
    pub fn set_selected_instance_type(&mut self, instance_type: impl Into<String>) {
        self.selected_instance_type = instance_type.into();
        self.reconcile_selection_with_filtered_instances();
    }

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
        Self::distinct_sorted(self.instances.iter().filter_map(|i| i.state.clone()))
    }

    /// Distinct, non-empty instance types, sorted case-insensitively.
    pub fn available_instance_types(&self) -> Vec<String> {
        Self::distinct_sorted(
            self.instances
                .iter()
                .filter_map(|i| i.instance_type.clone()),
        )
    }

    /// The current instance list with search/state/type filters applied,
    /// sorted by instance ID case-insensitively.
    pub fn filtered_instances(&self) -> Vec<EC2Instance> {
        let filters = ComputeInstanceFilters {
            name_contains: None,
            states: if self.selected_state.is_empty() {
                vec![]
            } else {
                vec![self.selected_state.clone()]
            },
            instance_types: if self.selected_instance_type.is_empty() {
                vec![]
            } else {
                vec![self.selected_instance_type.clone()]
            },
        };

        let matched = self.compile_search().filter_instances(&self.instances);
        let mut filtered = filters.apply(&matched);
        filtered.sort_by(|a, b| {
            a.instance_id
                .to_lowercase()
                .cmp(&b.instance_id.to_lowercase())
                .then_with(|| a.instance_id.cmp(&b.instance_id))
        });
        filtered
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
    pub fn selected_instance(&self) -> Option<&EC2Instance> {
        let selected_id = self.selected_instance_id.as_deref()?;
        self.instances.iter().find(|i| i.id() == selected_id)
    }

    /// A user-facing summary of partial failures, or `None` if there are
    /// none (Swift: `partialFailureSummary`).
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

    /// Loads instances from the compute service, updating state on success
    /// or failure (Swift: `loadInstances()`).
    pub fn begin_load(&mut self) -> Option<std::sync::Arc<dyn ComputeService + Send + Sync>> {
        if self.is_loading {
            return None;
        }
        self.is_loading = true;
        self.error = None;
        self.error_message = None;

        Some(self.service.clone())
    }

    pub fn finish_load(
        &mut self,
        result: Result<crate::ComputeInstanceLoadResult, crate::BoxError>,
    ) {
        self.finish_load_at(result, SystemTime::now());
    }

    /// Like `finish_load`, recording `now` as the load's finish time.
    pub fn finish_load_at(
        &mut self,
        result: Result<crate::ComputeInstanceLoadResult, crate::BoxError>,
        now: SystemTime,
    ) {
        match result {
            Ok(result) => {
                self.error = None;
                self.error_message = None;
                let merged = merge_with_previous(&self.instances, &result);
                self.instances = merged.instances;
                self.stale_regions = merged.stale_regions;
                self.known_tag_keys = crate::known_tag_keys(&self.instances);
                self.partial_failures = result.partial_failures;
                self.last_success = Some(now);
                for instance in &result.instances {
                    if let Some(region) = instance.region.as_deref() {
                        self.region_loaded_at.insert(region.to_string(), now);
                    }
                }
                self.reconcile_selected_filters();
                self.reconcile_selection_with_filtered_instances();
            }
            Err(error) => {
                let cloud_deck_error = CloudDeckError::from_service_error(error.as_ref());
                self.error_message = Some(cloud_deck_error.message_for("list instances"));
                self.error = Some(cloud_deck_error);
            }
        }

        self.is_loading = false;
    }

    pub async fn load_instances(&mut self) {
        if let Some(service) = self.begin_load() {
            let result = service.list_instances(None).await;
            self.finish_load(result);
        }
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
        let filtered_ids: HashSet<String> =
            self.filtered_instances().iter().map(|i| i.id()).collect();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DirectoryScenarioCatalog, ScenarioService, TestScenarioKind};
    use std::time::{Duration, UNIX_EPOCH};

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/ec2-scenarios");

    fn catalog() -> DirectoryScenarioCatalog {
        DirectoryScenarioCatalog::new(FIXTURES)
    }

    fn view_model_for(kind: TestScenarioKind) -> InstanceListViewModel {
        let resolved = ScenarioService::for_kind(kind, &catalog()).unwrap();
        InstanceListViewModel::new(Box::new(resolved.into_service()))
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
        let mut vm =
            InstanceListViewModel::new(Box::new(crate::MockComputeService::new(vec![], None)));
        vm.finish_load_at(loaded(vec![in_region("us-east-1", "a")], &[]), at(T1_SECS));
        vm.set_search_text("keep-me");
        vm.set_selected_instance_id(Some("us-east-1:a".into()));

        vm.begin_load();
        vm.finish_load_at(Err(Box::new(StubError)), at(T1_SECS + 60));

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
        let mut vm = InstanceListViewModel::new(Box::new(fleet));
        vm.load_instances().await;
        assert!(vm.instances().is_empty());
        assert!(vm.partial_failures().is_empty());
        assert!(
            vm.error_message()
                .unwrap()
                .starts_with("Could not list instances:")
        );
    }

    /// A service whose first call fails and whose subsequent calls succeed,
    /// used to exercise the failure-then-success load transition below.
    struct FailsOnceThenSucceeds {
        failed_once: std::sync::atomic::AtomicBool,
        instances: Vec<EC2Instance>,
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
            Ok(crate::ComputeInstanceLoadResult {
                instances: self.instances.clone(),
                partial_failures: vec![],
            })
        }
    }

    #[tokio::test]
    async fn a_successful_load_after_a_prior_failure_clears_the_stale_error() {
        let mut vm = InstanceListViewModel::new(Box::new(FailsOnceThenSucceeds {
            failed_once: std::sync::atomic::AtomicBool::new(false),
            instances: vec![EC2Instance {
                instance_id: "i-recovered".to_string(),
                region: None,
                instance_type: None,
                state: None,
                launch_time: None,
                public_ip_address: None,
                private_ip_address: None,
                vpc_id: None,
                subnet_id: None,
                security_groups: vec![],
                tags: vec![],
                availability_zone: None,
                image_id: None,
                key_name: None,
                architecture: None,
                state_transition_reason: None,
                iam_instance_profile_arn: None,
                private_dns_name: None,
                public_dns_name: None,
            }],
        }));

        vm.load_instances().await;
        assert!(vm.error_message().is_some(), "first load should fail");

        vm.load_instances().await;

        assert!(
            vm.error_message().is_none(),
            "a successful load must clear the stale error from the prior failure"
        );
        assert_eq!(vm.instances().len(), 1);
        assert_eq!(vm.instances()[0].instance_id, "i-recovered");
    }

    #[tokio::test]
    async fn refresh_reloads_instances() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);

        vm.refresh().await;

        assert_eq!(vm.instances().len(), 3);
    }

    #[tokio::test]
    async fn filtered_instances_applies_search_text_filter() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_search_text("bastion");
        let filtered = vm.filtered_instances();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name(), Some("bastion-east-1"));
    }

    #[tokio::test]
    async fn filtered_instances_applies_state_filter() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_selected_state("running");
        let filtered = vm.filtered_instances();
        let ids: Vec<&str> = filtered.iter().map(|i| i.instance_id.as_str()).collect();
        assert_eq!(ids, vec!["i-scenario-001", "i-scenario-003"]);
    }

    #[tokio::test]
    async fn filtered_instances_applies_instance_type_filter() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        vm.set_selected_instance_type("t3.medium");
        let filtered = vm.filtered_instances();
        let ids: Vec<&str> = filtered.iter().map(|i| i.instance_id.as_str()).collect();
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
        assert_eq!(filtered[0].instance_id, "i-scenario-003");

        // A constraint that matches nothing in combination excludes everything.
        vm.set_selected_instance_type("m6i.large");
        assert!(vm.filtered_instances().is_empty());
    }

    #[tokio::test]
    async fn filtered_instances_with_no_filters_returns_all_sorted_by_id() {
        let mut vm = view_model_for(TestScenarioKind::SmallHappyPath);
        vm.load_instances().await;

        let filtered = vm.filtered_instances();
        let ids: Vec<&str> = filtered.iter().map(|i| i.instance_id.as_str()).collect();
        let mut expected = ids.clone();
        expected.sort_by_key(|a| a.to_lowercase());
        assert_eq!(ids, expected);
        assert_eq!(filtered.len(), 3);
    }

    fn filtered_ids(vm: &InstanceListViewModel) -> Vec<String> {
        vm.filtered_instances()
            .into_iter()
            .map(|i| i.instance_id)
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
        let first_id = vm.instances()[0].id();
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
            .find(|i| i.instance_id == "i-scenario-002")
            .unwrap()
            .id();
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
            .find(|i| i.instance_id == "i-scenario-002")
            .unwrap()
            .id();
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
        vm.set_selected_instance_id(Some(target.id()));

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
        let vm = InstanceListViewModel::new(Box::new(crate::MockComputeService::new(vec![], None)));
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

    fn in_region(region: &str, instance_id: &str) -> EC2Instance {
        EC2Instance {
            instance_id: instance_id.to_string(),
            region: Some(region.to_string()),
            instance_type: None,
            state: None,
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![],
            tags: vec![],
            availability_zone: None,
            image_id: None,
            key_name: None,
            architecture: None,
            state_transition_reason: None,
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
        }
    }

    fn loaded(
        instances: Vec<EC2Instance>,
        failed_regions: &[&str],
    ) -> Result<crate::ComputeInstanceLoadResult, crate::BoxError> {
        Ok(crate::ComputeInstanceLoadResult {
            instances,
            partial_failures: failed_regions
                .iter()
                .map(|region| ComputeServicePartialFailure {
                    region: region.to_string(),
                    message: "request failed".to_string(),
                    error: CloudDeckError::Unknown("request failed".to_string()),
                })
                .collect(),
        })
    }

    fn loaded_ids(vm: &InstanceListViewModel) -> Vec<&str> {
        vm.instances()
            .iter()
            .map(|i| i.instance_id.as_str())
            .collect()
    }

    #[test]
    fn failed_region_keeps_previous_rows_and_is_marked_stale_with_load_times() {
        let mut vm =
            InstanceListViewModel::new(Box::new(crate::MockComputeService::new(vec![], None)));
        vm.finish_load_at(
            loaded(
                vec![in_region("us-east-1", "a"), in_region("eu-central-1", "b")],
                &[],
            ),
            at(T1_SECS),
        );

        vm.finish_load_at(
            loaded(vec![in_region("us-east-1", "a")], &["eu-central-1"]),
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
        let mut vm =
            InstanceListViewModel::new(Box::new(crate::MockComputeService::new(vec![], None)));
        vm.finish_load_at(
            loaded(
                vec![in_region("us-east-1", "a"), in_region("eu-central-1", "b")],
                &[],
            ),
            at(T1_SECS),
        );
        vm.finish_load_at(
            loaded(vec![in_region("us-east-1", "a")], &["eu-central-1"]),
            at(T2_SECS),
        );

        vm.finish_load_at(
            loaded(
                vec![in_region("us-east-1", "a"), in_region("eu-central-1", "b")],
                &[],
            ),
            at(T2_SECS),
        );

        assert!(vm.stale_regions().is_empty());
        assert!(vm.partial_failures().is_empty());
        assert_eq!(vm.region_loaded_at("eu-central-1"), Some(at(T2_SECS)));
    }

    #[test]
    fn failed_load_keeps_rows_stale_regions_and_last_success() {
        let mut vm =
            InstanceListViewModel::new(Box::new(crate::MockComputeService::new(vec![], None)));
        vm.finish_load_at(
            loaded(
                vec![in_region("us-east-1", "a"), in_region("eu-central-1", "b")],
                &[],
            ),
            at(T1_SECS),
        );
        vm.finish_load_at(
            loaded(vec![in_region("us-east-1", "a")], &["eu-central-1"]),
            at(T2_SECS),
        );

        vm.begin_load();
        vm.finish_load_at(Err(Box::new(StubError)), at(T2_SECS + 60));

        assert_eq!(loaded_ids(&vm), vec!["a", "b"]);
        assert_eq!(vm.stale_regions(), ["eu-central-1".to_string()]);
        assert_eq!(vm.last_success(), Some(at(T2_SECS)));
        assert_eq!(vm.partial_failures().len(), 1);
        assert_eq!(vm.partial_failures()[0].region, "eu-central-1");
    }

    #[test]
    fn finish_load_without_time_records_a_last_success() {
        let mut vm =
            InstanceListViewModel::new(Box::new(crate::MockComputeService::new(vec![], None)));

        vm.finish_load(loaded(vec![in_region("us-east-1", "a")], &[]));

        assert!(vm.last_success().is_some());
    }
}
