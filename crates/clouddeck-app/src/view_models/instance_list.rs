use std::collections::HashSet;

use clouddeck_core::{
    ComputeInstanceFilters, ComputeService, ComputeServicePartialFailure, EC2Instance,
};

/// Rust port of the Swift oracle's `InstanceListViewModel`
/// (`ViewModels/EC2/InstanceListViewModel.swift`).
///
/// Plain state + explicit update methods rather than `@Observable`: a gpui
/// view owns one of these and calls the mutating methods, re-rendering
/// itself after `load_instances`/`refresh` resolve. No gpui types are
/// referenced here.
pub struct InstanceListViewModel {
    service: Box<dyn ComputeService + Send + Sync>,
    instances: Vec<EC2Instance>,
    is_loading: bool,
    error_message: Option<String>,
    partial_failures: Vec<ComputeServicePartialFailure>,
    search_text: String,
    selected_state: String,
    selected_instance_type: String,
    selected_instance_id: Option<String>,
}

impl InstanceListViewModel {
    pub fn new(service: Box<dyn ComputeService + Send + Sync>) -> Self {
        Self {
            service,
            instances: Vec::new(),
            is_loading: false,
            error_message: None,
            partial_failures: Vec::new(),
            search_text: String::new(),
            selected_state: String::new(),
            selected_instance_type: String::new(),
            selected_instance_id: None,
        }
    }

    pub fn instances(&self) -> &[EC2Instance] {
        &self.instances
    }

    pub fn is_loading(&self) -> bool {
        self.is_loading
    }

    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
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
    /// newly filtered list (Swift: `searchText`'s `didSet`). Not yet called
    /// anywhere: no editable text-input widget exists in this codebase yet
    /// for a real search box (see `instance_list.rs`'s doc comment on the
    /// same limitation).
    #[allow(dead_code)]
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
            name_contains: Some(self.search_text.clone()),
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

        let mut filtered = filters.apply(&self.instances);
        filtered.sort_by(|a, b| {
            a.instance_id
                .to_lowercase()
                .cmp(&b.instance_id.to_lowercase())
                .then_with(|| a.instance_id.cmp(&b.instance_id))
        });
        filtered
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
    pub async fn load_instances(&mut self) {
        self.is_loading = true;
        self.error_message = None;
        self.partial_failures = vec![];

        match self.service.list_instances(None).await {
            Ok(result) => {
                self.instances = result.instances;
                self.partial_failures = result.partial_failures;
                self.reconcile_selected_filters();
                self.reconcile_selection_with_filtered_instances();
            }
            Err(error) => {
                self.instances = vec![];
                self.partial_failures = vec![];
                let cloud_deck_error =
                    clouddeck_core::CloudDeckError::from_service_error(error.as_ref());
                self.error_message = Some(cloud_deck_error.message_for("list instances"));
            }
        }

        self.is_loading = false;
    }

    /// Reloads instances (Swift: `refresh()`, which just calls
    /// `loadInstances()`). Not yet called anywhere outside tests: wiring the
    /// refresh button's click handler to this async call needs an
    /// established click-to-async-spawn pattern, which is a follow-on bead
    /// (see `instance_list.rs`'s doc comment).
    #[allow(dead_code)]
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
    use clouddeck_core::{DirectoryScenarioCatalog, ScenarioService, TestScenarioKind};

    const FIXTURES: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../clouddeck-core/testdata/ec2-scenarios"
    );

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

    #[tokio::test]
    async fn load_failure_clears_instances_and_sets_error_message() {
        let mut vm = view_model_for(TestScenarioKind::Failure);
        vm.set_search_text("keep-me");

        vm.load_instances().await;

        assert!(!vm.is_loading());
        assert!(vm.instances().is_empty());
        assert!(vm.partial_failures().is_empty());
        let message = vm.error_message().expect("error message should be set");
        assert!(message.starts_with("Could not list instances:"));
        // Filter state itself is untouched by a failed load.
        assert_eq!(vm.search_text(), "keep-me");
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
        let vm = InstanceListViewModel::new(Box::new(clouddeck_core::MockComputeService::new(
            vec![],
            None,
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
        }];
        assert_eq!(
            vm.partial_failure_summary(),
            Some("1 region failed: us-west-2".to_string())
        );

        vm.partial_failures = vec![
            ComputeServicePartialFailure {
                region: "us-west-2".to_string(),
                message: "boom".to_string(),
            },
            ComputeServicePartialFailure {
                region: "eu-west-1".to_string(),
                message: "boom".to_string(),
            },
        ];
        assert_eq!(
            vm.partial_failure_summary(),
            Some("2 regions failed: eu-west-1, us-west-2".to_string())
        );
    }
}
