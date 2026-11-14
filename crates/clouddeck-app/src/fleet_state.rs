//! Shared state entity the new views observe: instances, query, sort, selection, panes.

#[cfg(test)]
use std::cell::OnceCell;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
#[cfg(test)]
use std::time::Instant;
use std::time::{Duration, SystemTime};

use clouddeck_core::{
    CloudDeckError, Column, ConnectionId, DisplayRow, GroupBy, Instance, ListResult, ListView,
    PROVIDER_COLUMN_WIDTH, ProviderRow, RECENT_INSTANCE_LIMIT, REGION, RegionCounts, SortState,
    StateFacetCounts, ValueId, build_display_rows, compare_instances, join_failure,
    navigable_indices, provider_cell_text, provider_column_default_visible, push_recent,
    throttle_retry_delay, visible_columns_fit_hidden,
};
use gpui::{App, Context};

const FLASH_DURATION: Duration = Duration::from_millis(1_800);

struct FacetCache {
    query: String,
    generation: u64,
    states: StateFacetCounts,
    types: Vec<(String, usize)>,
}

/// Fleet data and UI state shared by every main-window view.
pub struct FleetState {
    /// Instance list for the active provider, or for all of them; its search text is the query.
    pub view_model: ListView,
    /// Column sort applied to the visible rows.
    pub sort: SortState,
    /// `Instance::id` of the selected row.
    pub selected: Option<String>,
    pub sidebar_open: bool,
    pub inspector_open: bool,
    /// Whether the inspector shows every tag rather than the first few.
    pub tags_show_all: bool,
    /// Transient status bar text, cleared after `FLASH_DURATION`.
    pub flash: Option<String>,
    pub profile: String,
    pub demo: bool,
    pub recent_searches: Vec<String>,
    pub pinned_searches: Vec<String>,
    pub recent_instances: Vec<String>,
    /// Per-region instance counts from the last load of each connection, saved with the UI state.
    pub region_counts: RegionCounts,
    /// Column the table groups its rows by.
    pub group_by: GroupBy,
    /// Keys of the groups the user has collapsed.
    pub collapsed_groups: BTreeSet<String>,
    /// Columns the column chooser has flipped from their default. Name is never hidden, and the
    /// Provider column is shown by default only in the All view. Its All-view flip lives here;
    /// its single-provider flip is `single_provider_column_flipped`.
    pub hidden_columns: Vec<Column>,
    /// Whether the chooser has flipped the Provider column in single-provider views.
    pub single_provider_column_flipped: bool,
    /// Whether the column chooser menu is open.
    pub column_menu_open: bool,
    /// Index into `Column::ALL` of the item the keyboard is on in the column chooser.
    pub column_menu_focus: usize,
    /// Seconds until a throttled load is retried, while one is scheduled.
    pub retry_in_seconds: Option<u32>,
    flash_generation: u64,
    throttle_attempt: u32,
    retry_generation: u64,
    facet_cache: RefCell<Option<FacetCache>>,
    facet_computations: Cell<u64>,
    #[cfg(test)]
    clock: OnceCell<(SystemTime, Instant)>,
}

impl FleetState {
    /// Creates the state with the default sort and both panes open.
    pub fn new(view_model: impl Into<ListView>, profile: String, demo: bool) -> Self {
        Self {
            view_model: view_model.into(),
            sort: SortState::default(),
            selected: None,
            sidebar_open: true,
            inspector_open: true,
            tags_show_all: false,
            flash: None,
            profile,
            demo,
            recent_searches: Vec::new(),
            pinned_searches: Vec::new(),
            recent_instances: Vec::new(),
            region_counts: RegionCounts::new(),
            group_by: GroupBy::None,
            collapsed_groups: BTreeSet::new(),
            hidden_columns: Vec::new(),
            single_provider_column_flipped: false,
            column_menu_open: false,
            column_menu_focus: 0,
            retry_in_seconds: None,
            flash_generation: 0,
            throttle_attempt: 0,
            retry_generation: 0,
            facet_cache: RefCell::new(None),
            facet_computations: Cell::new(0),
            #[cfg(test)]
            clock: OnceCell::new(),
        }
    }

    /// The current wall time, shared by load stamps and refresh checks.
    pub fn now(&self, cx: &App) -> SystemTime {
        #[cfg(test)]
        {
            let (wall, origin) = *self
                .clock
                .get_or_init(|| (SystemTime::now(), cx.background_executor().now()));
            wall + cx
                .background_executor()
                .now()
                .saturating_duration_since(origin)
        }
        #[cfg(not(test))]
        {
            let _ = cx;
            SystemTime::now()
        }
    }

    /// The current search text.
    pub fn query(&self) -> &str {
        self.view_model.search_text()
    }

    /// Replaces the search text, clearing the selection if its row is hidden.
    pub fn set_query(&mut self, query: &str, cx: &mut Context<Self>) {
        self.view_model.set_search_text(query);
        self.reconcile_selection();
        cx.notify();
    }

    /// Cached per (query, load generation); relative-time terms keep their last-computed `now`.
    pub fn facet_counts(&self, now: SystemTime) -> (StateFacetCounts, Vec<(String, usize)>) {
        let generation = self.view_model.load_generation();
        let cached = self
            .facet_cache
            .borrow()
            .as_ref()
            .filter(|cache| cache.query == self.query() && cache.generation == generation)
            .map(|cache| (cache.states, cache.types.clone()));
        if let Some(counts) = cached {
            return counts;
        }
        let states = self.view_model.state_facet_counts(now);
        let types = self.view_model.type_facet_counts(now);
        self.facet_computations
            .set(self.facet_computations.get() + 1);
        *self.facet_cache.borrow_mut() = Some(FacetCache {
            query: self.query().to_string(),
            generation,
            states,
            types: types.clone(),
        });
        (states, types)
    }

    /// How many times `facet_counts` has recomputed rather than returned the cache.
    #[cfg(test)]
    pub fn facet_computations(&self) -> u64 {
        self.facet_computations.get()
    }

    /// Instances passing the query and filters, in the current sort order.
    pub fn visible_rows(&self) -> Vec<Instance> {
        self.visible_provider_rows()
            .into_iter()
            .map(|(_, instance)| instance)
            .collect()
    }

    fn visible_provider_rows(&self) -> Vec<ProviderRow> {
        let mut rows = self.view_model.rows();
        let sort = self.sort;
        if sort.column == Column::Provider {
            let names = self.view_model.provider_names();
            let label = |id: &str| {
                let name = names.get(id).map_or(id, String::as_str);
                provider_cell_text(id, name).text
            };
            rows.sort_by(|(left_id, left), (right_id, right)| {
                let ordering = label(left_id).cmp(&label(right_id));
                let ordering = if sort.ascending {
                    ordering
                } else {
                    ordering.reverse()
                };
                ordering.then_with(|| compare_instances(left, right, SortState::default()))
            });
        } else {
            rows.sort_by(|(_, left), (_, right)| compare_instances(left, right, sort));
        }
        rows
    }

    /// Whether the Provider column is shown: its default in the current view, flipped by the chooser.
    pub fn provider_column_shown(&self) -> bool {
        let all = self.view_model.is_all();
        let default =
            all && provider_column_default_visible(self.view_model.provider_names().len());
        let flipped = if all {
            self.hidden_columns.contains(&Column::Provider)
        } else {
            self.single_provider_column_flipped
        };
        default != flipped
    }

    pub fn column_shown(&self, column: Column) -> bool {
        match column {
            Column::Provider => self.provider_column_shown(),
            _ => !self.hidden_columns.contains(&column),
        }
    }

    /// The columns the table draws, in order, for the given window and table pane widths.
    pub fn table_columns(&self, window_width: f32, pane_width: f32) -> Vec<Column> {
        let provider = self.provider_column_shown();
        let pane_width = if provider {
            pane_width - PROVIDER_COLUMN_WIDTH
        } else {
            pane_width
        };
        let mut columns =
            visible_columns_fit_hidden(window_width, pane_width, &self.hidden_columns);
        if provider {
            columns.insert(0, Column::Provider);
        }
        columns
    }

    /// The table rows for the current grouping, each with its provider id, and the display rows
    /// that index into them.
    pub fn display_rows(&self) -> (Vec<ProviderRow>, Vec<DisplayRow>) {
        let paired = self.visible_provider_rows();
        let failed: Vec<String> = self
            .view_model
            .provider_failures()
            .into_iter()
            .map(|failure| failure.provider_id)
            .collect();
        let names = self.view_model.provider_names();
        let display = build_display_rows(
            &paired,
            self.group_by,
            &self.collapsed_groups,
            &failed,
            &names,
        );
        (paired, display)
    }

    /// Groups the table by `by`, with every group expanded.
    pub fn set_group_by(&mut self, by: GroupBy, cx: &mut Context<Self>) {
        self.group_by = by;
        self.collapsed_groups.clear();
        cx.notify();
    }

    /// Collapses or expands the group `key`. Collapsing the group that holds the selection
    /// clears the selection, because group headers cannot be selected.
    pub fn toggle_group(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.collapsed_groups.remove(key) {
            self.collapsed_groups.insert(key.to_string());
        }
        if let Some(selected) = self.selected.clone() {
            let (rows, display) = self.display_rows();
            let still_shown = navigable_indices(&display)
                .into_iter()
                .any(|index| rows[index].1.id == selected);
            if !still_shown {
                self.select(None, cx);
                return;
            }
        }
        cx.notify();
    }

    /// Shows or hides `column`; Name cannot be hidden.
    pub fn toggle_column(&mut self, column: Column, cx: &mut Context<Self>) {
        if column == Column::Name {
            return;
        }
        if column == Column::Provider && !self.view_model.is_all() {
            self.single_provider_column_flipped = !self.single_provider_column_flipped;
            cx.notify();
            return;
        }
        if let Some(index) = self
            .hidden_columns
            .iter()
            .position(|hidden| *hidden == column)
        {
            self.hidden_columns.remove(index);
        } else {
            self.hidden_columns.push(column);
        }
        cx.notify();
    }

    pub fn set_column_menu_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.column_menu_open = open;
        self.column_menu_focus = 0;
        cx.notify();
    }

    pub fn move_column_menu_focus(&mut self, down: bool, cx: &mut Context<Self>) {
        let last = Column::ALL.len() - 1;
        self.column_menu_focus = if down {
            (self.column_menu_focus + 1).min(last)
        } else {
            self.column_menu_focus.saturating_sub(1)
        };
        cx.notify();
    }

    pub fn toggle_focused_column(&mut self, cx: &mut Context<Self>) {
        self.toggle_column(Column::ALL[self.column_menu_focus], cx);
    }

    /// Sorts by `column`, flipping direction when it is already the sort column.
    pub fn toggle_sort(&mut self, column: Column, cx: &mut Context<Self>) {
        self.sort = self.sort.toggled(column);
        cx.notify();
    }

    /// Selects the row with `key`, opening the inspector when a row is chosen.
    pub fn select(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        self.tags_show_all = false;
        if let Some(key) = key.as_deref() {
            self.inspector_open = true;
            push_recent(&mut self.recent_instances, key, RECENT_INSTANCE_LIMIT);
        }
        self.view_model.set_selected_instance_id(key.clone());
        self.selected = key;
        cx.notify();
    }

    /// Moves the selection to the previous or next visible row.
    pub fn move_selection(&mut self, down: bool, cx: &mut Context<Self>) {
        let (rows, display) = self.display_rows();
        let keys: Vec<String> = navigable_indices(&display)
            .into_iter()
            .map(|index| rows[index].1.id.clone())
            .collect();
        let next = clouddeck_core::move_selection(&keys, self.selected.as_deref(), down);
        self.select(next, cx);
    }

    /// The selected instance, if its row is currently shown rather than inside a collapsed group.
    pub fn selected_instance(&self) -> Option<Instance> {
        let key = self.selected.as_deref()?;
        let (rows, display) = self.display_rows();
        navigable_indices(&display)
            .into_iter()
            .map(|index| &rows[index].1)
            .find(|instance| instance.id == key)
            .cloned()
    }

    /// Shows `text` in the status bar until `FLASH_DURATION` has passed.
    pub fn set_flash(&mut self, text: String, cx: &mut Context<Self>) {
        self.flash_generation += 1;
        let generation = self.flash_generation;
        self.flash = Some(text);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FLASH_DURATION).await;
            let _ = this.update(cx, |this, cx| {
                if this.flash_generation == generation {
                    this.flash = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Reloads instances from the compute service in the background.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let requests = self.view_model.begin_loads();
        if requests.is_empty() {
            return;
        }
        self.retry_generation += 1;
        self.retry_in_seconds = None;
        cx.notify();
        for (provider_id, request) in requests {
            let selection = request.selection().clone();
            let connection = request.connection().clone();
            let task = crate::runtime::runtime().spawn_blocking(move || request.run());
            cx.spawn(async move |this, cx| {
                let result = task
                    .await
                    .unwrap_or_else(|error| join_failure(&selection, &error));
                let _ = this.update(cx, |this, cx| {
                    let now = this.now(cx);
                    record_region_counts(
                        &mut this.region_counts,
                        &provider_id,
                        &connection,
                        selection.values(REGION),
                        &result,
                    );
                    this.view_model.finish_load_for(&provider_id, result, now);
                    this.reconcile_selection();
                    this.after_load(cx);
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn after_load(&mut self, cx: &mut Context<Self>) {
        match self.view_model.error() {
            None => self.throttle_attempt = 0,
            Some(CloudDeckError::Throttled) => self.schedule_throttle_retry(cx),
            Some(_) => {}
        }
    }

    fn schedule_throttle_retry(&mut self, cx: &mut Context<Self>) {
        let delay = throttle_retry_delay(self.throttle_attempt);
        self.throttle_attempt = self.throttle_attempt.saturating_add(1);
        let mut remaining = delay.as_secs() as u32;
        self.retry_in_seconds = Some(remaining);
        let generation = self.retry_generation;
        cx.spawn(async move |this, cx| {
            while remaining > 0 {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                remaining -= 1;
                let still_pending = this.update(cx, |this, cx| {
                    if this.retry_generation != generation {
                        return false;
                    }
                    this.retry_in_seconds = Some(remaining);
                    cx.notify();
                    true
                });
                if !matches!(still_pending, Ok(true)) {
                    return;
                }
            }
            let _ = this.update(cx, |this, cx| {
                if this.retry_generation == generation {
                    this.refresh(cx);
                }
            });
        })
        .detach();
    }

    fn reconcile_selection(&mut self) {
        if let Some(key) = self.selected.clone() {
            let (rows, display) = self.display_rows();
            let shown = navigable_indices(&display)
                .into_iter()
                .any(|index| rows[index].1.id == key);
            if !shown {
                self.selected = None;
            }
        }
        self.view_model
            .set_selected_instance_id(self.selected.clone());
    }
}

fn record_region_counts(
    counts: &mut RegionCounts,
    provider: &str,
    connection: &ConnectionId,
    requested: &[ValueId],
    result: &ListResult,
) {
    if requested.is_empty() {
        return;
    }
    let failed: Vec<&str> = result
        .failures
        .iter()
        .flat_map(|failure| failure.cell.values(REGION))
        .map(ValueId::as_str)
        .collect();
    let saved = counts
        .entry(provider.to_string())
        .or_default()
        .entry(connection.as_str().to_string())
        .or_default();
    for region in requested {
        let region = region.as_str();
        if failed.contains(&region) {
            continue;
        }
        let instances = result
            .resources
            .iter()
            .filter(|instance| instance.region.as_deref() == Some(region))
            .count();
        saved.insert(region.to_string(), instances as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{load_fleet, wait_until};
    use clouddeck_core::{
        InstanceListViewModel, fixture_fleet as fleet, native_id, sort_instances,
    };
    use gpui::{AppContext, Entity, TestAppContext};

    fn new_state(cx: &mut TestAppContext) -> Entity<FleetState> {
        let vm = InstanceListViewModel::new(fleet::static_provider(fleet::instances(&[
            ("i-1", "api-server", "running", "t3.medium"),
            ("i-2", "bastion", "stopped", "t3.medium"),
            ("i-3", "worker", "stopped", "t3.medium"),
            ("i-4", "cache", "running", "t3.medium"),
        ])));
        fresh_state(cx, vm)
    }

    fn fresh_state(cx: &mut TestAppContext, vm: InstanceListViewModel) -> Entity<FleetState> {
        cx.new(|_| FleetState::new(vm, "default".to_string(), true))
    }

    fn loaded_state(cx: &mut TestAppContext) -> Entity<FleetState> {
        let state = new_state(cx);
        load_fleet(cx, &state);
        state
    }

    fn visible_ids(state: &Entity<FleetState>, cx: &mut TestAppContext) -> Vec<String> {
        state.read_with(cx, |state, _| {
            state.visible_rows().iter().map(|i| i.id.clone()).collect()
        })
    }

    #[test]
    fn region_counts_record_each_requested_region_and_keep_failed_ones() {
        let connection = ConnectionId::from("prod");
        let requested = [ValueId::from("us-east-1"), ValueId::from("eu-west-1")];
        let in_region = |region: &str, count: usize| {
            (0..count)
                .map(|n| {
                    let mut instance =
                        fleet::instance(&format!("i-{n}"), "api", "running", "t3.medium");
                    instance.region = Some(region.to_string());
                    instance
                })
                .collect::<Vec<_>>()
        };
        let mut counts = RegionCounts::new();
        let first = ListResult {
            resources: in_region("us-east-1", 2),
            failures: Vec::new(),
        };
        record_region_counts(&mut counts, "aws", &connection, &requested, &first);
        assert_eq!(
            counts["aws"]["prod"],
            std::collections::BTreeMap::from([
                ("us-east-1".to_string(), 2),
                ("eu-west-1".to_string(), 0)
            ])
        );

        let second = ListResult {
            resources: in_region("eu-west-1", 3),
            failures: vec![clouddeck_core::CellFailure {
                cell: clouddeck_core::Selection::new()
                    .with(REGION, vec![ValueId::from("us-east-1")]),
                error: clouddeck_core::ProviderError::unsupported(),
            }],
        };
        record_region_counts(&mut counts, "aws", &connection, &requested, &second);
        assert_eq!(
            counts["aws"]["prod"],
            std::collections::BTreeMap::from([
                ("us-east-1".to_string(), 2),
                ("eu-west-1".to_string(), 3)
            ])
        );
    }

    #[test]
    fn region_counts_ignore_loads_without_a_region_selection() {
        let mut counts = RegionCounts::new();
        let result = ListResult::default();
        record_region_counts(
            &mut counts,
            "gcp",
            &ConnectionId::from("team"),
            &[],
            &result,
        );
        assert!(counts.is_empty());
    }

    #[gpui::test]
    fn refresh_loads_instances(cx: &mut TestAppContext) {
        let state = loaded_state(cx);

        state.read_with(cx, |state, _| {
            assert_eq!(state.view_model.instances().len(), 4);
            assert!(!state.view_model.is_loading());
        });
    }

    #[gpui::test]
    fn set_query_filters_visible_rows_by_state(cx: &mut TestAppContext) {
        let state = loaded_state(cx);

        state.update(cx, |state, cx| state.set_query("state:stopped", cx));
        let rows = state.read_with(cx, |state, _| state.visible_rows());
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|i| i.state.raw == "stopped"));

        state.update(cx, |state, cx| state.set_query("", cx));
        assert_eq!(visible_ids(&state, cx).len(), 4);
    }

    #[gpui::test]
    fn default_sort_is_name_ascending(cx: &mut TestAppContext) {
        let state = loaded_state(cx);

        state.read_with(cx, |state, _| {
            assert_eq!(state.sort, SortState::default());
            assert_eq!(state.sort.column, Column::Name);
            assert!(state.sort.ascending);
        });
        assert_eq!(
            visible_ids(&state, cx),
            vec![
                "us-east-1:i-1",
                "us-east-1:i-2",
                "us-east-1:i-4",
                "us-east-1:i-3"
            ]
        );
    }

    #[gpui::test]
    fn toggle_sort_reverses_name_order(cx: &mut TestAppContext) {
        let state = loaded_state(cx);

        state.update(cx, |state, cx| state.toggle_sort(Column::Name, cx));

        let mut expected: Vec<Instance> = state.read_with(cx, |state, _| {
            state
                .view_model
                .rows()
                .into_iter()
                .map(|(_, instance)| instance)
                .collect()
        });
        sort_instances(
            &mut expected,
            SortState {
                column: Column::Name,
                ascending: false,
            },
        );
        let expected_ids: Vec<String> = expected.iter().map(|i| i.id.clone()).collect();
        assert_eq!(visible_ids(&state, cx), expected_ids);
        assert_eq!(
            visible_ids(&state, cx),
            vec![
                "us-east-1:i-3",
                "us-east-1:i-4",
                "us-east-1:i-2",
                "us-east-1:i-1"
            ]
        );
    }

    #[gpui::test]
    fn provider_column_toggle_in_single_view_leaves_the_all_view_flip_alone(
        cx: &mut TestAppContext,
    ) {
        let state = new_state(cx);

        state.update(cx, |state, cx| state.toggle_column(Column::Provider, cx));

        state.read_with(cx, |state, _| {
            assert!(state.single_provider_column_flipped);
            assert!(state.hidden_columns.is_empty());
            assert!(state.provider_column_shown());
        });
    }

    #[gpui::test]
    fn toggle_sort_to_new_column_sorts_ascending(cx: &mut TestAppContext) {
        let state = loaded_state(cx);

        state.update(cx, |state, cx| state.toggle_sort(Column::State, cx));

        state.read_with(cx, |state, _| {
            assert_eq!(
                state.sort,
                SortState {
                    column: Column::State,
                    ascending: true,
                }
            );
        });
    }

    #[gpui::test]
    fn select_opens_inspector_and_hiding_the_row_clears_it(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        state.update(cx, |state, cx| {
            state.inspector_open = false;
            state.select(Some("us-east-1:i-1".to_string()), cx);
        });
        state.read_with(cx, |state, _| {
            assert_eq!(state.selected.as_deref(), Some("us-east-1:i-1"));
            assert!(state.inspector_open);
        });

        state.update(cx, |state, cx| state.set_query("state:stopped", cx));

        state.read_with(cx, |state, _| assert_eq!(state.selected, None));
    }

    #[gpui::test]
    fn move_selection_steps_through_visible_rows(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        let ids = visible_ids(&state, cx);

        state.update(cx, |state, cx| state.move_selection(true, cx));
        state.read_with(cx, |state, _| {
            assert_eq!(state.selected.as_deref(), Some(ids[0].as_str()));
        });

        state.update(cx, |state, cx| state.move_selection(true, cx));
        state.read_with(cx, |state, _| {
            assert_eq!(state.selected.as_deref(), Some(ids[1].as_str()));
        });
    }

    #[gpui::test]
    fn selected_instance_resolves_the_selected_key(cx: &mut TestAppContext) {
        let state = loaded_state(cx);
        state.update(cx, |state, cx| {
            state.select(Some("us-east-1:i-2".to_string()), cx);
        });

        let selected = state.read_with(cx, |state, _| state.selected_instance());
        assert_eq!(
            selected.map(|i| native_id(&i).to_string()),
            Some("i-2".to_string())
        );
    }

    fn huge_loaded_state(cx: &mut TestAppContext) -> Entity<FleetState> {
        let vm = InstanceListViewModel::new(fleet::static_provider(fleet::huge_fleet()));
        let state = fresh_state(cx, vm);
        load_fleet(cx, &state);
        state
    }

    fn facets(
        state: &Entity<FleetState>,
        cx: &mut TestAppContext,
    ) -> (StateFacetCounts, Vec<(String, usize)>) {
        state.read_with(cx, |state, _| state.facet_counts(SystemTime::now()))
    }

    fn computations(state: &Entity<FleetState>, cx: &mut TestAppContext) -> u64 {
        state.read_with(cx, |state, _| state.facet_computations())
    }

    #[gpui::test]
    fn facet_counts_recompute_only_when_the_query_or_the_load_changes(cx: &mut TestAppContext) {
        let state = huge_loaded_state(cx);

        let (states, _) = facets(&state, cx);
        assert_eq!(
            states,
            StateFacetCounts {
                all: 5000,
                running: 2500,
                stopped: 1250,
                pending: 1250,
            }
        );
        assert_eq!(computations(&state, cx), 1);
        facets(&state, cx);
        facets(&state, cx);
        assert_eq!(computations(&state, cx), 1);

        state.update(cx, |state, cx| state.set_query("state:stopped", cx));
        let (states, types) = facets(&state, cx);
        assert_eq!(computations(&state, cx), 2);
        assert_eq!(states.all, 5000);
        assert_eq!(states.stopped, 1250);
        let stopped_types = [
            "c6i.large",
            "m6i.large",
            "r6i.large",
            "t3.medium",
            "t3.micro",
        ];
        for (name, count) in &types {
            let expected = if stopped_types.contains(&name.as_str()) {
                250
            } else {
                0
            };
            assert_eq!(*count, expected, "{name}");
        }

        state.update(cx, |state, cx| state.refresh(cx));
        wait_until(cx, |cx| {
            state.read_with(cx, |state, _| {
                state.view_model.load_generation() == 2 && !state.view_model.is_loading()
            })
        });
        facets(&state, cx);
        assert_eq!(computations(&state, cx), 3);
    }

    fn grouped_by_state(cx: &mut TestAppContext) -> Entity<FleetState> {
        let state = loaded_state(cx);
        state.update(cx, |state, cx| state.set_group_by(GroupBy::State, cx));
        state
    }

    fn navigable_ids(state: &Entity<FleetState>, cx: &mut TestAppContext) -> Vec<String> {
        state.read_with(cx, |state, _| {
            let (rows, display) = state.display_rows();
            navigable_indices(&display)
                .into_iter()
                .map(|index| rows[index].1.id.clone())
                .collect()
        })
    }

    fn selected_id(state: &Entity<FleetState>, cx: &mut TestAppContext) -> Option<String> {
        state.read_with(cx, |state, _| state.selected.clone())
    }

    #[gpui::test]
    fn ungrouped_display_rows_are_the_visible_rows_in_order(cx: &mut TestAppContext) {
        let state = loaded_state(cx);

        let (indices, ids) = state.read_with(cx, |state, _| {
            let (rows, display) = state.display_rows();
            (
                navigable_indices(&display),
                rows.iter().map(|(_, i)| i.id.clone()).collect::<Vec<_>>(),
            )
        });

        assert_eq!(indices, (0..ids.len()).collect::<Vec<_>>());
        assert_eq!(ids, visible_ids(&state, cx));
    }

    #[gpui::test]
    fn arrow_keys_skip_group_headers(cx: &mut TestAppContext) {
        let state = grouped_by_state(cx);
        let expected = vec![
            "us-east-1:i-1".to_string(),
            "us-east-1:i-4".to_string(),
            "us-east-1:i-2".to_string(),
            "us-east-1:i-3".to_string(),
        ];
        assert_eq!(navigable_ids(&state, cx), expected);

        for id in &expected {
            state.update(cx, |state, cx| state.move_selection(true, cx));
            assert_eq!(selected_id(&state, cx).as_deref(), Some(id.as_str()));
        }
        state.update(cx, |state, cx| state.move_selection(false, cx));
        assert_eq!(selected_id(&state, cx).as_deref(), Some("us-east-1:i-2"));
    }

    #[gpui::test]
    fn arrow_keys_skip_a_collapsed_group(cx: &mut TestAppContext) {
        let state = grouped_by_state(cx);
        state.update(cx, |state, cx| state.toggle_group("Running", cx));

        state.update(cx, |state, cx| state.move_selection(true, cx));

        assert_eq!(selected_id(&state, cx).as_deref(), Some("us-east-1:i-2"));
        assert_eq!(
            navigable_ids(&state, cx),
            vec!["us-east-1:i-2".to_string(), "us-east-1:i-3".to_string()]
        );
    }

    #[gpui::test]
    fn collapsing_the_selected_group_clears_the_selection(cx: &mut TestAppContext) {
        let state = grouped_by_state(cx);
        state.update(cx, |state, cx| {
            state.select(Some("us-east-1:i-1".to_string()), cx)
        });

        state.update(cx, |state, cx| state.toggle_group("Running", cx));

        assert_eq!(selected_id(&state, cx), None);
        state.read_with(cx, |state, _| {
            assert!(state.collapsed_groups.contains("Running"));
            assert_eq!(state.selected_instance().map(|i| i.id), None);
        });
    }

    #[gpui::test]
    fn collapsing_another_group_keeps_the_selection(cx: &mut TestAppContext) {
        let state = grouped_by_state(cx);
        state.update(cx, |state, cx| {
            state.select(Some("us-east-1:i-2".to_string()), cx)
        });

        state.update(cx, |state, cx| state.toggle_group("Running", cx));

        assert_eq!(selected_id(&state, cx).as_deref(), Some("us-east-1:i-2"));
    }

    #[gpui::test]
    fn toggling_a_group_twice_expands_it_again(cx: &mut TestAppContext) {
        let state = grouped_by_state(cx);
        state.update(cx, |state, cx| state.toggle_group("Stopped", cx));
        assert_eq!(navigable_ids(&state, cx).len(), 2);

        state.update(cx, |state, cx| state.toggle_group("Stopped", cx));

        assert_eq!(navigable_ids(&state, cx).len(), 4);
    }

    #[gpui::test]
    fn changing_the_grouping_expands_every_group(cx: &mut TestAppContext) {
        let state = grouped_by_state(cx);
        state.update(cx, |state, cx| state.toggle_group("Running", cx));

        state.update(cx, |state, cx| state.set_group_by(GroupBy::Type, cx));

        state.read_with(cx, |state, _| {
            assert_eq!(state.group_by, GroupBy::Type);
            assert!(state.collapsed_groups.is_empty());
        });
    }

    #[gpui::test]
    fn flash_clears_after_1_8_seconds(cx: &mut TestAppContext) {
        let state = loaded_state(cx);

        state.update(cx, |state, cx| {
            state.set_flash("Copied i-1".to_string(), cx)
        });
        cx.executor().advance_clock(Duration::from_millis(1_700));
        cx.run_until_parked();
        state.read_with(cx, |state, _| {
            assert_eq!(state.flash.as_deref(), Some("Copied i-1"));
        });

        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
        state.read_with(cx, |state, _| assert_eq!(state.flash, None));
    }

    #[gpui::test]
    fn a_newer_flash_is_not_cleared_by_an_older_timer(cx: &mut TestAppContext) {
        let state = loaded_state(cx);

        state.update(cx, |state, cx| {
            state.set_flash("Copied i-1".to_string(), cx)
        });
        cx.executor().advance_clock(Duration::from_secs(1));
        state.update(cx, |state, cx| {
            state.set_flash("Copied i-2".to_string(), cx)
        });
        cx.executor().advance_clock(Duration::from_millis(1_000));
        cx.run_until_parked();
        state.read_with(cx, |state, _| {
            assert_eq!(state.flash.as_deref(), Some("Copied i-2"));
        });

        cx.executor().advance_clock(Duration::from_millis(900));
        cx.run_until_parked();
        state.read_with(cx, |state, _| assert_eq!(state.flash, None));
    }

    fn plain_state() -> FleetState {
        let vm = InstanceListViewModel::new(fleet::static_provider(fleet::instances(&[(
            "i-1",
            "api-server",
            "running",
            "t3.medium",
        )])));
        FleetState::new(vm, "default".to_string(), true)
    }

    #[test]
    fn column_shown_respects_hidden_columns() {
        let mut state = plain_state();
        assert!(state.column_shown(Column::Name));
        state.hidden_columns.push(Column::Name);
        assert!(!state.column_shown(Column::Name));
    }

    #[test]
    fn table_columns_omit_hidden_columns_except_the_name() {
        let mut state = plain_state();
        assert!(state.table_columns(4000.0, 4000.0).contains(&Column::InstanceId));
        state.hidden_columns.push(Column::InstanceId);
        state.hidden_columns.push(Column::Name);
        let columns = state.table_columns(4000.0, 4000.0);
        assert!(!columns.contains(&Column::InstanceId));
        assert!(columns.contains(&Column::Name));
    }
}
