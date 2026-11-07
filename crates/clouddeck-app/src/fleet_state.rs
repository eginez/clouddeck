//! Shared state entity the new views observe: instances, query, sort, selection, panes.

#[cfg(test)]
use std::cell::OnceCell;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use std::time::Instant;
use std::time::{Duration, SystemTime};

use clouddeck_core::{
    CloudDeckError, Column, DisplayRow, GroupBy, Instance, RECENT_INSTANCE_LIMIT, SortState,
    StateFacetCounts, build_display_rows, join_failure, navigable_indices, push_recent,
    sort_instances, throttle_retry_delay,
};
use gpui::{App, Context};

use crate::view_models::InstanceListViewModel;

const FLASH_DURATION: Duration = Duration::from_millis(1_800);

struct FacetCache {
    query: String,
    generation: u64,
    states: StateFacetCounts,
    types: Vec<(String, usize)>,
}

/// Fleet data and UI state shared by every main-window view.
pub struct FleetState {
    /// Instance list; its search text is the query.
    pub view_model: InstanceListViewModel,
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
    /// Column the table groups its rows by.
    pub group_by: GroupBy,
    /// Keys of the groups the user has collapsed.
    pub collapsed_groups: BTreeSet<String>,
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
    pub fn new(view_model: InstanceListViewModel, profile: String, demo: bool) -> Self {
        Self {
            view_model,
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
            group_by: GroupBy::None,
            collapsed_groups: BTreeSet::new(),
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
        let mut rows = self.view_model.filtered_instances();
        sort_instances(&mut rows, self.sort);
        rows
    }

    /// The table rows for the current grouping, with the instances they refer to by index.
    pub fn display_rows(&self) -> (Vec<Instance>, Vec<DisplayRow>) {
        let rows = self.visible_rows();
        let provider_id = self.view_model.row_provider_id();
        let failed = if self.view_model.error().is_some() {
            vec![provider_id.clone()]
        } else {
            Vec::new()
        };
        let names: BTreeMap<String, String> = self
            .view_model
            .provider_facet()
            .into_iter()
            .map(|facet| (facet.id, facet.name))
            .collect();
        let paired: Vec<(String, Instance)> = rows
            .iter()
            .map(|instance| (provider_id.clone(), instance.clone()))
            .collect();
        let display = build_display_rows(
            &paired,
            self.group_by,
            &self.collapsed_groups,
            &failed,
            &names,
        );
        (rows, display)
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
                .any(|index| rows[index].id == selected);
            if !still_shown {
                self.select(None, cx);
                return;
            }
        }
        cx.notify();
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
            .map(|index| rows[index].id.clone())
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
            .map(|index| &rows[index])
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
        let Some(request) = self.view_model.begin_load() else {
            return;
        };
        self.retry_generation += 1;
        self.retry_in_seconds = None;
        cx.notify();
        let selection = request.selection().clone();
        let task = crate::runtime::runtime().spawn_blocking(move || request.run());
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .unwrap_or_else(|error| join_failure(&selection, &error));
            let _ = this.update(cx, |this, cx| {
                let now = this.now(cx);
                this.view_model.finish_load_at(result, now);
                this.reconcile_selection();
                this.after_load(cx);
                cx.notify();
            });
        })
        .detach();
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
                .any(|index| rows[index].id == key);
            if !shown {
                self.selected = None;
            }
        }
        self.view_model
            .set_selected_instance_id(self.selected.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clouddeck_core::{fixture_fleet as fleet, native_id};
    use gpui::{AppContext, Entity, TestAppContext};

    fn new_state(cx: &mut TestAppContext) -> Entity<FleetState> {
        let vm = InstanceListViewModel::new(fleet::static_provider(fleet::instances(&[
            ("i-1", "api-server", "running", "t3.medium"),
            ("i-2", "bastion", "stopped", "t3.medium"),
            ("i-3", "worker", "stopped", "t3.medium"),
            ("i-4", "cache", "running", "t3.medium"),
        ])));
        cx.new(|_| FleetState::new(vm, "default".to_string(), true))
    }

    fn loaded_state(cx: &mut TestAppContext) -> Entity<FleetState> {
        let state = new_state(cx);
        state.update(cx, |state, cx| state.refresh(cx));
        wait_until(cx, |cx| {
            state.read_with(cx, |state, _| {
                !state.view_model.instances().is_empty() && !state.view_model.is_loading()
            })
        });
        state
    }

    fn wait_until(cx: &mut TestAppContext, mut condition: impl FnMut(&mut TestAppContext) -> bool) {
        for _ in 0..200 {
            cx.run_until_parked();
            if condition(cx) {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("condition was not satisfied within the timeout");
    }

    fn visible_ids(state: &Entity<FleetState>, cx: &mut TestAppContext) -> Vec<String> {
        state.read_with(cx, |state, _| {
            state.visible_rows().iter().map(|i| i.id.clone()).collect()
        })
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

        let mut expected = state.read_with(cx, |state, _| state.view_model.filtered_instances());
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
        let state = cx.new(|_| FleetState::new(vm, "default".to_string(), true));
        state.update(cx, |state, cx| state.refresh(cx));
        wait_until(cx, |cx| {
            state.read_with(cx, |state, _| {
                !state.view_model.instances().is_empty() && !state.view_model.is_loading()
            })
        });
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
                .map(|index| rows[index].id.clone())
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
                rows.iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
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
}
