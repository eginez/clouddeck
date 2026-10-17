//! Left sidebar: state and type facets, pinned searches, recent instances (design/images/20).

use std::time::SystemTime;

use clouddeck_core::{
    EC2Instance, QueryField, RECENT_INSTANCES_SHOWN, facet_selection, known_tag_keys,
    remove_field_value, resolve_recent_instances, set_field_value, state_facet_counts, top_types,
    type_facet_counts,
};
use gpui::{
    App, ClickEvent, Context, Entity, FontWeight, Hsla, IntoElement, Render, SharedString, Window,
    div, prelude::*, px,
};
use gpui_component::Sizable;

use crate::fleet_state::FleetState;
use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, MONO_FONT, Palette, ROW_HEIGHT};

const TYPE_FACETS_SHOWN: usize = 5;
const PADDING: f32 = 8.0;
const ROW_INSET: f32 = 6.0;
const ROW_RADIUS: f32 = 5.0;
const ROW_GAP: f32 = 8.0;
const DOT_SIZE: f32 = 8.0;
const SECTION_FONT_SIZE: f32 = 11.0;
const SECTION_TOP: f32 = 14.0;
const SECTION_BOTTOM: f32 = 4.0;

pub struct Sidebar {
    state: Entity<FleetState>,
    pub show_all_types: bool,
}

impl Sidebar {
    pub fn new(state: Entity<FleetState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            show_all_types: false,
        }
    }

    fn toggle_facet(
        &mut self,
        field: QueryField,
        value: &str,
        active: bool,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, cx| {
            let known = known_tag_keys(state.view_model.instances());
            let query = if active {
                remove_field_value(state.query(), field, value, &known)
            } else {
                set_field_value(state.query(), field, value, &known)
            };
            state.set_query(&query, cx);
        });
    }

    fn apply_query(&mut self, query: &str, cx: &mut Context<Self>) {
        self.state
            .update(cx, |state, cx| state.set_query(query, cx));
    }

    fn open_recent(&mut self, key: &str, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            state.set_query("", cx);
            state.select(Some(key.to_string()), cx);
        });
    }
}

impl Render for Sidebar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::tokens::palette(cx);
        let state = self.state.read(cx);
        let instances = state.view_model.instances();
        let query = state.query().to_string();
        let known = known_tag_keys(instances);
        let now = SystemTime::now();
        let selection = facet_selection(&query, &known);
        let states = state_facet_counts(instances, &query, &known, now);
        let type_counts = type_facet_counts(instances, &query, &known, now);
        let type_total = type_counts.len();
        let state_value = selection.state.as_deref();
        let type_value = selection.instance_type.as_deref();
        let types = visible_types(&type_counts, self.show_all_types, type_value);
        let pins = state.pinned_searches.clone();
        let recent: Vec<EC2Instance> = resolve_recent_instances(&state.recent_instances, instances)
            .into_iter()
            .take(RECENT_INSTANCES_SHOWN)
            .cloned()
            .collect();

        let state_rows = [
            ("all", "All", "", None, states.all, state_value.is_none()),
            (
                "running",
                "Running",
                "running",
                Some(palette.state_dot("running")),
                states.running,
                is_value(state_value, "running"),
            ),
            (
                "stopped",
                "Stopped",
                "stopped",
                Some(palette.state_dot("stopped")),
                states.stopped,
                is_value(state_value, "stopped"),
            ),
            (
                "pending",
                "Pending",
                "pending",
                Some(palette.state_dot("pending")),
                states.pending,
                is_value(state_value, "pending"),
            ),
        ];

        let mut panel = div()
            .flex()
            .flex_col()
            .size_full()
            .p(px(PADDING))
            .bg(palette.side)
            .text_size(px(FONT_SIZE))
            .text_color(palette.text)
            .child(section_header("STATE", &palette));
        for (key, label, value, dot, count, active) in state_rows {
            panel = panel.child(facet_row(
                FacetRow {
                    key: format!("sidebar.state.{key}"),
                    label: label.to_string(),
                    dot,
                    mono: false,
                    count,
                    active,
                },
                &palette,
                facet_click(QueryField::State, value.to_string(), active, cx),
            ));
        }

        panel = panel.child(section_header("TYPE", &palette));
        for (name, count) in types {
            let active = type_value
                .is_some_and(|value| value.split(',').any(|piece| is_value(Some(piece), &name)));
            panel = panel.child(facet_row(
                FacetRow {
                    key: format!("sidebar.type.{name}"),
                    label: name.clone(),
                    dot: None,
                    mono: true,
                    count,
                    active,
                },
                &palette,
                facet_click(QueryField::Type, name, active, cx),
            ));
        }
        if type_total > TYPE_FACETS_SHOWN {
            panel = panel.child(types_toggle(type_total, self.show_all_types, &palette, cx));
        }

        if !pins.is_empty() {
            panel = panel.child(section_header("PINNED SEARCHES", &palette));
            for (index, pin) in pins.into_iter().enumerate() {
                panel = panel.child(pin_row(index, pin, &palette, cx));
            }
        }

        if !recent.is_empty() {
            panel = panel.child(section_header("RECENT", &palette));
            for (index, instance) in recent.iter().enumerate() {
                panel = panel.child(recent_row(index, instance, &palette, cx));
            }
        }

        panel
    }
}

fn is_value(selected: Option<&str>, value: &str) -> bool {
    selected.is_some_and(|s| s.eq_ignore_ascii_case(value))
}

fn visible_types(
    counts: &[(String, usize)],
    show_all: bool,
    active: Option<&str>,
) -> Vec<(String, usize)> {
    if show_all {
        return counts.to_vec();
    }
    let mut shown = top_types(counts, TYPE_FACETS_SHOWN);
    let Some(active) = active else {
        return shown;
    };
    for value in active.split(',') {
        if shown.iter().any(|(name, _)| is_value(Some(value), name)) {
            continue;
        }
        if let Some(entry) = counts.iter().find(|(name, _)| is_value(Some(value), name)) {
            shown.push(entry.clone());
        }
    }
    shown
}

fn facet_click(
    field: QueryField,
    value: String,
    active: bool,
    cx: &Context<Sidebar>,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_facet(field, &value, active, cx))
}

fn section_header(title: &'static str, palette: &Palette) -> impl IntoElement {
    div()
        .px(px(ROW_INSET))
        .pt(px(SECTION_TOP))
        .pb(px(SECTION_BOTTOM))
        .text_size(px(SECTION_FONT_SIZE))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(palette.faint)
        .child(title)
}

struct FacetRow {
    key: String,
    label: String,
    dot: Option<Hsla>,
    mono: bool,
    count: usize,
    active: bool,
}

fn facet_row(
    row: FacetRow,
    palette: &Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let FacetRow {
        key,
        label,
        dot,
        mono,
        count,
        active,
    } = row;
    let text = if count == 0 {
        palette.faint
    } else {
        palette.text
    };
    div()
        .id(SharedString::from(key.clone()))
        .debug_selector(move || key)
        .flex()
        .items_center()
        .gap(px(ROW_GAP))
        .h(px(ROW_HEIGHT))
        .px(px(ROW_INSET))
        .rounded(px(ROW_RADIUS))
        .text_color(text)
        .when(active, |row| row.bg(palette.sel))
        .when(!active, |row| row.hover(|row| row.bg(palette.hover)))
        .on_click(on_click)
        .when_some(dot, |row, dot| {
            row.child(div().w(px(DOT_SIZE)).h(px(DOT_SIZE)).rounded_full().bg(dot))
        })
        .child(
            div()
                .flex_1()
                .truncate()
                .when(mono, |label| label.font_family(MONO_FONT))
                .child(label),
        )
        .child(div().text_color(palette.dim).child(count.to_string()))
}

fn types_toggle(
    total: usize,
    showing_all: bool,
    palette: &Palette,
    cx: &Context<Sidebar>,
) -> impl IntoElement {
    let label = if showing_all {
        "Show fewer".to_string()
    } else {
        format!("Show all {total} types")
    };
    div()
        .id("sidebar.types-toggle")
        .debug_selector(|| "sidebar.types-toggle".into())
        .flex()
        .items_center()
        .h(px(ROW_HEIGHT))
        .px(px(ROW_INSET))
        .text_color(palette.dim)
        .hover(|row| row.text_color(palette.text))
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.show_all_types = !this.show_all_types;
            cx.notify();
        }))
        .child(label)
}

fn pin_row(
    index: usize,
    query: String,
    palette: &Palette,
    cx: &Context<Sidebar>,
) -> impl IntoElement {
    let key = format!("sidebar.pin.{index}");
    let click_query = query.clone();
    div()
        .id(SharedString::from(key.clone()))
        .debug_selector(move || key)
        .flex()
        .items_center()
        .gap(px(ROW_GAP))
        .h(px(ROW_HEIGHT))
        .px(px(ROW_INSET))
        .rounded(px(ROW_RADIUS))
        .text_color(palette.text)
        .hover(|row| row.bg(palette.hover))
        .on_click(
            cx.listener(move |this, _: &ClickEvent, _, cx| this.apply_query(&click_query, cx)),
        )
        .child(AppIcon::Search.icon().xsmall().text_color(palette.faint))
        .child(div().flex_1().truncate().child(query))
}

fn recent_row(
    index: usize,
    instance: &EC2Instance,
    palette: &Palette,
    cx: &Context<Sidebar>,
) -> impl IntoElement {
    let key = instance.id();
    let click_key = key.clone();
    let name = instance.name().unwrap_or("unnamed").to_string();
    let dot = palette.state_dot(instance.state.as_deref().unwrap_or_default());
    div()
        .id(SharedString::from(format!("sidebar.recent.{index}")))
        .debug_selector(move || format!("sidebar.recent.{index}"))
        .flex()
        .items_center()
        .gap(px(ROW_GAP))
        .h(px(ROW_HEIGHT))
        .px(px(ROW_INSET))
        .rounded(px(ROW_RADIUS))
        .text_color(palette.text)
        .hover(|row| row.bg(palette.hover))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_recent(&click_key, cx)))
        .child(div().w(px(DOT_SIZE)).h(px(DOT_SIZE)).rounded_full().bg(dot))
        .child(div().flex_1().truncate().child(name))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use clouddeck_core::{EC2InstanceTag, MockComputeService};
    use gpui::{Entity, TestAppContext, VisualTestContext};

    use super::*;
    use crate::view_models::InstanceListViewModel;

    fn instance(id: &str, name: &str, state: &str, instance_type: &str) -> EC2Instance {
        EC2Instance {
            instance_id: id.to_string(),
            region: Some("us-east-1".to_string()),
            instance_type: Some(instance_type.to_string()),
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

    fn fleet() -> Vec<EC2Instance> {
        vec![
            instance("i-1", "api-server", "running", "t3.medium"),
            instance("i-2", "web-frontend", "running", "t3.micro"),
            instance("i-3", "worker", "running", "m6i.large"),
            instance("i-4", "cache", "running", "t3.small"),
            instance("i-5", "bastion", "stopped", "t3.micro"),
            instance("i-6", "batch", "stopped", "g5.xlarge"),
            instance("i-7", "indexer", "pending", "r6i.xlarge"),
        ]
    }

    const STATE_KEYS: [&str; 4] = [
        "sidebar.state.all",
        "sidebar.state.running",
        "sidebar.state.stopped",
        "sidebar.state.pending",
    ];

    const TYPE_KEYS: [&str; 6] = [
        "sidebar.type.g5.xlarge",
        "sidebar.type.m6i.large",
        "sidebar.type.r6i.xlarge",
        "sidebar.type.t3.medium",
        "sidebar.type.t3.micro",
        "sidebar.type.t3.small",
    ];

    fn rooted_sidebar(
        cx: &mut TestAppContext,
        configure: impl FnOnce(&mut FleetState),
    ) -> (Entity<FleetState>, Entity<Sidebar>, &mut VisualTestContext) {
        let mut vm = InstanceListViewModel::new(Box::new(MockComputeService::new(fleet(), None)));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(vm.load_instances());
        let mut fleet_state = FleetState::new(vm, "default".to_string(), true);
        configure(&mut fleet_state);
        let state = cx.new(|_| fleet_state);
        let state_for_window = state.clone();
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let sidebar = cx.new(|cx| Sidebar::new(state_for_window.clone(), cx));
            *slot_for_window.borrow_mut() = Some(sidebar.clone());
            gpui_component::Root::new(sidebar, window, cx)
        });
        let sidebar = slot.borrow().clone().expect("sidebar should be created");
        (state, sidebar, cx)
    }

    fn rendered(cx: &mut VisualTestContext, selector: &'static str) -> bool {
        cx.debug_bounds(selector).is_some()
    }

    fn rendered_type_rows(cx: &mut VisualTestContext) -> usize {
        TYPE_KEYS.iter().filter(|key| rendered(cx, key)).count()
    }

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should render"));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    }

    fn query(state: &Entity<FleetState>, cx: &mut VisualTestContext) -> String {
        state.read_with(cx, |state, _| state.query().to_string())
    }

    #[gpui::test]
    fn renders_state_facets_and_the_top_five_types(cx: &mut TestAppContext) {
        let (_state, _sidebar, cx) = rooted_sidebar(cx, |_| {});

        for key in STATE_KEYS {
            assert!(rendered(cx, key), "{key} should render");
        }
        assert!(rendered(cx, "sidebar.types-toggle"));
        assert_eq!(rendered_type_rows(cx), 5);
        assert!(!rendered(cx, "sidebar.type.t3.small"));
        assert!(!rendered(cx, "sidebar.pin.0"));
        assert!(!rendered(cx, "sidebar.recent.0"));
    }

    #[gpui::test]
    fn show_all_types_reveals_every_type(cx: &mut TestAppContext) {
        let (_state, sidebar, cx) = rooted_sidebar(cx, |_| {});

        click(cx, "sidebar.types-toggle");
        assert_eq!(rendered_type_rows(cx), 6);
        assert!(rendered(cx, "sidebar.type.t3.small"));

        // debug_bounds keeps selectors from earlier frames, so collapsing is checked on the field.
        click(cx, "sidebar.types-toggle");
        assert!(!sidebar.read_with(cx, |sidebar, _| sidebar.show_all_types));
    }

    #[gpui::test]
    fn clicking_a_state_facet_sets_then_clears_it(cx: &mut TestAppContext) {
        let (state, _sidebar, cx) = rooted_sidebar(cx, |_| {});

        click(cx, "sidebar.state.stopped");
        assert_eq!(query(&state, cx), "state:stopped");

        click(cx, "sidebar.state.stopped");
        assert_eq!(query(&state, cx), "");
    }

    #[gpui::test]
    fn all_removes_the_state_term_and_keeps_other_terms(cx: &mut TestAppContext) {
        let (state, _sidebar, cx) = rooted_sidebar(cx, |state| {
            state.view_model.set_search_text("web state:running");
        });

        click(cx, "sidebar.state.all");
        assert_eq!(query(&state, cx), "web");
    }

    #[gpui::test]
    fn clicking_a_type_facet_adds_the_type_term(cx: &mut TestAppContext) {
        let (state, _sidebar, cx) = rooted_sidebar(cx, |_| {});

        click(cx, "sidebar.type.t3.micro");
        assert_eq!(query(&state, cx), "type:t3.micro");
    }

    #[gpui::test]
    fn pinned_search_row_applies_its_query(cx: &mut TestAppContext) {
        let (state, _sidebar, cx) = rooted_sidebar(cx, |state| {
            state.view_model.set_search_text("web");
            state.pinned_searches = vec!["state:stopped".to_string()];
        });

        assert!(rendered(cx, "sidebar.pin.0"));
        click(cx, "sidebar.pin.0");
        assert_eq!(query(&state, cx), "state:stopped");
    }

    #[gpui::test]
    fn clicking_the_active_type_facet_clears_it(cx: &mut TestAppContext) {
        let (state, _sidebar, cx) = rooted_sidebar(cx, |state| {
            state.view_model.set_search_text("type:t3.micro");
        });

        click(cx, "sidebar.type.t3.micro");
        assert_eq!(query(&state, cx), "");
    }

    #[gpui::test]
    fn clicking_an_active_multi_value_type_removes_only_that_value(cx: &mut TestAppContext) {
        let (state, _sidebar, cx) = rooted_sidebar(cx, |state| {
            state.view_model.set_search_text("type:t3.micro,t3.small");
        });

        click(cx, "sidebar.type.t3.micro");
        assert_eq!(query(&state, cx), "type:t3.small");

        click(cx, "sidebar.type.t3.small");
        assert_eq!(query(&state, cx), "");
    }

    #[gpui::test]
    fn clicking_an_inactive_type_replaces_a_multi_value_term(cx: &mut TestAppContext) {
        let (state, _sidebar, cx) = rooted_sidebar(cx, |state| {
            state
                .view_model
                .set_search_text("web type:t3.micro,t3.small");
        });

        click(cx, "sidebar.type.m6i.large");
        assert_eq!(query(&state, cx), "web type:m6i.large");
    }

    #[gpui::test]
    fn active_type_outside_the_top_five_stays_visible_and_clears(cx: &mut TestAppContext) {
        let (state, sidebar, cx) = rooted_sidebar(cx, |state| {
            state.view_model.set_search_text("type:T3.Small");
        });

        assert!(!sidebar.read_with(cx, |sidebar, _| sidebar.show_all_types));
        assert!(rendered(cx, "sidebar.type.t3.small"));
        assert!(rendered(cx, "sidebar.types-toggle"));
        assert_eq!(rendered_type_rows(cx), 6);

        click(cx, "sidebar.type.t3.small");
        assert_eq!(query(&state, cx), "");
    }

    #[gpui::test]
    fn active_multi_value_type_outside_the_top_five_stays_visible(cx: &mut TestAppContext) {
        let (_state, sidebar, cx) = rooted_sidebar(cx, |state| {
            state.view_model.set_search_text("type:t3.micro,T3.Small");
        });

        assert!(!sidebar.read_with(cx, |sidebar, _| sidebar.show_all_types));
        assert!(rendered(cx, "sidebar.type.t3.small"));
        assert!(rendered(cx, "sidebar.type.t3.micro"));
        assert_eq!(rendered_type_rows(cx), 6);
    }

    #[gpui::test]
    fn active_state_facet_matches_the_term_case_insensitively(cx: &mut TestAppContext) {
        let (state, _sidebar, cx) = rooted_sidebar(cx, |state| {
            state.view_model.set_search_text("state:Stopped");
        });

        click(cx, "sidebar.state.stopped");
        assert_eq!(query(&state, cx), "");
    }

    #[gpui::test]
    fn recent_instance_row_clears_the_query_and_selects_it(cx: &mut TestAppContext) {
        let (state, _sidebar, cx) = rooted_sidebar(cx, |state| {
            state.view_model.set_search_text("state:stopped");
            state.recent_instances = vec!["us-east-1:i-3".to_string()];
        });

        assert!(rendered(cx, "sidebar.recent.0"));
        click(cx, "sidebar.recent.0");

        state.read_with(cx, |state, _| {
            assert_eq!(state.query(), "");
            assert_eq!(state.selected.as_deref(), Some("us-east-1:i-3"));
        });
    }
}
