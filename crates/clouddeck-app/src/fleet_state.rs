//! Shared state entity the new views observe: instances, query, sort, selection, panes.

use std::time::Duration;

use clouddeck_core::{Column, EC2Instance, SortState, sort_instances};
use gpui::Context;

use crate::view_models::InstanceListViewModel;

const FLASH_DURATION: Duration = Duration::from_millis(1_800);

/// Fleet data and UI state shared by every main-window view.
pub struct FleetState {
    /// Instance list; its search text is the query.
    pub view_model: InstanceListViewModel,
    /// Column sort applied to the visible rows.
    pub sort: SortState,
    /// `EC2Instance::id()` of the selected row.
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
    flash_generation: u64,
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
            flash_generation: 0,
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

    /// Instances passing the query and filters, in the current sort order.
    pub fn visible_rows(&self) -> Vec<EC2Instance> {
        let mut rows = self.view_model.filtered_instances();
        sort_instances(&mut rows, self.sort);
        rows
    }

    /// Sorts by `column`, flipping direction when it is already the sort column.
    pub fn toggle_sort(&mut self, column: Column, cx: &mut Context<Self>) {
        self.sort = self.sort.toggled(column);
        cx.notify();
    }

    /// Selects the row with `key`, opening the inspector when a row is chosen.
    pub fn select(&mut self, key: Option<String>, cx: &mut Context<Self>) {
        self.tags_show_all = false;
        if key.is_some() {
            self.inspector_open = true;
        }
        self.view_model.set_selected_instance_id(key.clone());
        self.selected = key;
        cx.notify();
    }

    /// Moves the selection to the previous or next visible row.
    pub fn move_selection(&mut self, down: bool, cx: &mut Context<Self>) {
        let keys: Vec<String> = self.visible_rows().iter().map(|i| i.id()).collect();
        let next = clouddeck_core::move_selection(&keys, self.selected.as_deref(), down);
        self.select(next, cx);
    }

    /// The selected instance, if it is still among the visible rows.
    pub fn selected_instance(&self) -> Option<EC2Instance> {
        let key = self.selected.as_deref()?;
        self.visible_rows().into_iter().find(|i| i.id() == key)
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
        let Some(service) = self.view_model.begin_load() else {
            return;
        };
        cx.notify();
        let task =
            crate::runtime::runtime().spawn(async move { service.list_instances(None).await });
        cx.spawn(async move |this, cx| {
            let result = task
                .await
                .unwrap_or_else(|error| Err(std::io::Error::other(error.to_string()).into()));
            let _ = this.update(cx, |this, cx| {
                this.view_model.finish_load(result);
                this.reconcile_selection();
                cx.notify();
            });
        })
        .detach();
    }

    fn reconcile_selection(&mut self) {
        if let Some(key) = self.selected.as_deref() {
            let visible = self.visible_rows().iter().any(|i| i.id() == key);
            if !visible {
                self.selected = None;
            }
        }
        self.view_model.set_selected_instance_id(self.selected.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clouddeck_core::{EC2InstanceTag, MockComputeService};
    use gpui::{AppContext, Entity, TestAppContext};

    fn instance(id: &str, name: &str, state: &str) -> EC2Instance {
        EC2Instance {
            instance_id: id.to_string(),
            region: Some("us-east-1".to_string()),
            instance_type: Some("t3.medium".to_string()),
            state: Some(state.to_string()),
            launch_time: None,
            public_ip_address: None,
            private_ip_address: None,
            vpc_id: None,
            subnet_id: None,
            security_groups: vec![],
            tags: vec![EC2InstanceTag {
                key: "Name".to_string(),
                value: name.to_string(),
            }],
            iam_instance_profile_arn: None,
            private_dns_name: None,
            public_dns_name: None,
            availability_zone: None,
            image_id: None,
            key_name: None,
            architecture: None,
            state_transition_reason: None,
        }
    }

    fn fixture_instances() -> Vec<EC2Instance> {
        vec![
            instance("i-1", "api-server", "running"),
            instance("i-2", "bastion", "stopped"),
            instance("i-3", "worker", "stopped"),
            instance("i-4", "cache", "running"),
        ]
    }

    fn new_state(cx: &mut TestAppContext) -> Entity<FleetState> {
        let vm = InstanceListViewModel::new(Box::new(MockComputeService::new(
            fixture_instances(),
            None,
        )));
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

    fn wait_until(
        cx: &mut TestAppContext,
        mut condition: impl FnMut(&mut TestAppContext) -> bool,
    ) {
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
            state.visible_rows().iter().map(|i| i.id()).collect()
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
        assert!(rows.iter().all(|i| i.state.as_deref() == Some("stopped")));

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
            vec!["us-east-1:i-1", "us-east-1:i-2", "us-east-1:i-4", "us-east-1:i-3"]
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
        let expected_ids: Vec<String> = expected.iter().map(|i| i.id()).collect();
        assert_eq!(visible_ids(&state, cx), expected_ids);
        assert_eq!(
            visible_ids(&state, cx),
            vec!["us-east-1:i-3", "us-east-1:i-4", "us-east-1:i-2", "us-east-1:i-1"]
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
        assert_eq!(selected.map(|i| i.instance_id), Some("i-2".to_string()));
    }

    #[gpui::test]
    fn flash_clears_after_1_8_seconds(cx: &mut TestAppContext) {
        let state = loaded_state(cx);

        state.update(cx, |state, cx| state.set_flash("Copied i-1".to_string(), cx));
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

        state.update(cx, |state, cx| state.set_flash("Copied i-1".to_string(), cx));
        cx.executor().advance_clock(Duration::from_secs(1));
        state.update(cx, |state, cx| state.set_flash("Copied i-2".to_string(), cx));
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
