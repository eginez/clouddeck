//! Titlebar toolbar: sidebar toggle, title, Filters, search (design/images/01, 20, 44).

use clouddeck_core::layout_rules;
use gpui::{
    AnyView, App, ClickEvent, Context, Entity, EventEmitter, FontWeight, IntoElement, MouseButton,
    Render, Window, div, prelude::*, px,
};
use gpui_component::Sizable;
use gpui_component::tooltip::Tooltip;

use crate::fleet_state::FleetState;
use crate::icons::AppIcon;
use crate::linux_titlebar::{AppMenuButton, WindowControls};
use crate::search_field::SearchField;
use crate::tokens::{FONT_SIZE, Palette, TOOLBAR_HEIGHT};

const TRAFFIC_LIGHT_INSET: f32 = 78.0;
const SIDE_INSET: f32 = 14.0;
const GAP: f32 = 8.0;
const TITLE_FONT_SIZE: f32 = 13.0;
const BUTTON_HEIGHT: f32 = 24.0;
const BUTTON_RADIUS: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarEvent {
    ToggleSidebar,
    FiltersClicked,
}

pub struct Toolbar {
    state: Entity<FleetState>,
    search: Entity<SearchField>,
    app_menu: Option<Entity<AppMenuButton>>,
    /// Label and accent state from the last render, so tests can check what was drawn.
    #[cfg(test)]
    rendered_filters: (String, bool),
}

impl EventEmitter<ToolbarEvent> for Toolbar {}

impl Toolbar {
    pub fn new(
        state: Entity<FleetState>,
        search: Entity<SearchField>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let app_menu = cfg!(target_os = "linux").then(|| cx.new(|_| AppMenuButton::default()));
        Self {
            state,
            search,
            app_menu,
            #[cfg(test)]
            rendered_filters: (String::new(), false),
        }
    }

    /// The Filters button label for the current query.
    pub fn filters_label(&self, cx: &App) -> String {
        filters_text(self.filter_count(cx))
    }

    fn filter_count(&self, cx: &App) -> usize {
        self.state.read(cx).view_model.recognised_filter_count()
    }
}

fn filters_text(count: usize) -> String {
    match count {
        0 => "Filters".to_string(),
        1 => "1 filter".to_string(),
        n => format!("{n} filters"),
    }
}

impl Render for Toolbar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::tokens::palette(cx);
        let viewport = window.viewport_size();
        let rules = layout_rules(f32::from(viewport.width), f32::from(viewport.height));
        let count = self.filter_count(cx);
        #[cfg(test)]
        {
            self.rendered_filters = (filters_text(count), count > 0);
        }
        let linux = cfg!(target_os = "linux");
        let leading = if cfg!(target_os = "macos") {
            TRAFFIC_LIGHT_INSET
        } else {
            SIDE_INSET
        };

        div()
            .flex()
            .items_center()
            .gap(px(GAP))
            .w_full()
            .h(px(TOOLBAR_HEIGHT))
            .pl(px(leading))
            .pr(px(SIDE_INSET))
            .bg(palette.chrome)
            .border_b_1()
            .border_color(palette.line)
            .when_some(self.app_menu.clone(), |bar, app_menu| bar.child(app_menu))
            .child(sidebar_button(&palette, cx))
            .when(rules.show_title, |bar| bar.child(title(&palette)))
            .child(div().flex_1().when(linux, |spacer| {
                spacer.on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            }))
            .child(filters_button(filters_text(count), count > 0, &palette, cx))
            .child(
                div()
                    .debug_selector(|| "toolbar.search".into())
                    .child(self.search.clone()),
            )
            .when(linux, |bar| bar.child(WindowControls))
    }
}

fn sidebar_button(palette: &Palette, cx: &Context<Toolbar>) -> impl IntoElement {
    div()
        .id("toolbar.sidebar")
        .debug_selector(|| "toolbar.sidebar".into())
        .flex()
        .items_center()
        .justify_center()
        .w(px(BUTTON_HEIGHT))
        .h(px(BUTTON_HEIGHT))
        .rounded(px(BUTTON_RADIUS))
        .hover(|button| button.bg(palette.hover))
        .tooltip(|_, cx| AnyView::from(cx.new(|_| Tooltip::new("Toggle sidebar (⌘B)"))))
        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(ToolbarEvent::ToggleSidebar)))
        .child(AppIcon::PanelLeft.icon().small().text_color(palette.dim))
}

fn title(palette: &Palette) -> impl IntoElement {
    div()
        .debug_selector(|| "toolbar.title".into())
        .text_size(px(TITLE_FONT_SIZE))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(palette.text)
        .child("EC2 Instances")
}

fn filters_button(
    label: String,
    active: bool,
    palette: &Palette,
    cx: &Context<Toolbar>,
) -> impl IntoElement {
    let color = if active { palette.accent } else { palette.dim };
    div()
        .id("toolbar.filters")
        .debug_selector(|| "toolbar.filters".into())
        .flex()
        .items_center()
        .gap(px(4.0))
        .px(px(8.0))
        .h(px(BUTTON_HEIGHT))
        .rounded(px(BUTTON_RADIUS))
        .text_size(px(FONT_SIZE))
        .text_color(color)
        .hover(|button| button.bg(palette.hover))
        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(ToolbarEvent::FiltersClicked)))
        .child(AppIcon::Filter.icon().xsmall().text_color(color))
        .child(label)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use clouddeck_core::{EC2Instance, EC2InstanceTag, MockComputeService};
    use gpui::{
        Bounds, Entity, TestAppContext, VisualTestContext, WindowBounds, WindowOptions, point, size,
    };

    use super::*;
    use crate::view_models::InstanceListViewModel;

    type Events = Rc<RefCell<Vec<ToolbarEvent>>>;

    fn instance(id: &str, name: &str, state: &str, owner: Option<&str>) -> EC2Instance {
        let mut tags = vec![EC2InstanceTag {
            key: "Name".to_string(),
            value: name.to_string(),
        }];
        if let Some(owner) = owner {
            tags.push(EC2InstanceTag {
                key: "Owner".to_string(),
                value: owner.to_string(),
            });
        }
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
            tags,
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

    fn three_instance_view_model() -> InstanceListViewModel {
        let instances = vec![
            instance("i-1", "api-server", "running", Some("esteban")),
            instance("i-2", "bastion", "stopped", None),
            instance("i-3", "worker", "stopped", None),
        ];
        let mut vm = InstanceListViewModel::new(Box::new(MockComputeService::new(instances, None)));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(vm.load_instances());
        vm
    }

    /// Opens the toolbar at the given width; gpui's test harness never resizes a
    /// rendered element out of `debug_bounds`, so each width gets a fresh window.
    fn rooted_toolbar(
        cx: &mut TestAppContext,
        window_width: f32,
    ) -> (
        Entity<FleetState>,
        Entity<Toolbar>,
        Events,
        &mut VisualTestContext,
    ) {
        let state =
            cx.new(|_| FleetState::new(three_instance_view_model(), "default".to_string(), true));
        let toolbar_slot = Rc::new(RefCell::new(None));
        let events: Events = Rc::new(RefCell::new(Vec::new()));
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(window_width), px(800.0)));
        let slot_for_window = toolbar_slot.clone();
        let events_for_window = events.clone();
        let state_for_window = state.clone();
        let window = cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    ..Default::default()
                },
                move |window, cx| {
                    gpui_component::init(cx);
                    let search =
                        cx.new(|cx| SearchField::new(state_for_window.clone(), window, cx));
                    let toolbar = cx.new(|cx| Toolbar::new(state_for_window, search, cx));
                    let log = events_for_window.clone();
                    cx.subscribe(&toolbar, move |_, event: &ToolbarEvent, _| {
                        log.borrow_mut().push(*event)
                    })
                    .detach();
                    *slot_for_window.borrow_mut() = Some(toolbar.clone());
                    cx.new(|cx| gpui_component::Root::new(toolbar, window, cx))
                },
            )
            .unwrap()
        });
        let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
        cx.run_until_parked();
        let toolbar = toolbar_slot
            .borrow()
            .as_ref()
            .expect("toolbar entity should be created")
            .clone();
        (state, toolbar, events, cx)
    }

    fn set_query(state: &Entity<FleetState>, query: &str, cx: &mut VisualTestContext) {
        state.update(cx, |state, cx| state.set_query(query, cx));
        cx.run_until_parked();
    }

    fn filters_label(toolbar: &Entity<Toolbar>, cx: &mut VisualTestContext) -> String {
        toolbar.read_with(cx, |toolbar, cx| toolbar.filters_label(cx))
    }

    fn rendered_filters(toolbar: &Entity<Toolbar>, cx: &mut VisualTestContext) -> (String, bool) {
        toolbar.read_with(cx, |toolbar, _| toolbar.rendered_filters.clone())
    }

    #[gpui::test]
    fn wide_window_shows_every_toolbar_element(cx: &mut TestAppContext) {
        let (_state, _toolbar, _events, cx) = rooted_toolbar(cx, 1280.0);

        assert!(cx.debug_bounds("toolbar.sidebar").is_some());
        assert!(cx.debug_bounds("toolbar.title").is_some());
        assert!(cx.debug_bounds("toolbar.filters").is_some());
        assert!(cx.debug_bounds("toolbar.search").is_some());
    }

    #[gpui::test]
    fn compact_window_hides_the_title(cx: &mut TestAppContext) {
        let (_state, _toolbar, _events, cx) = rooted_toolbar(cx, 520.0);

        assert!(cx.debug_bounds("toolbar.title").is_none());
        assert!(cx.debug_bounds("toolbar.sidebar").is_some());
        assert!(cx.debug_bounds("toolbar.filters").is_some());
        assert!(cx.debug_bounds("toolbar.search").is_some());
    }

    #[gpui::test]
    fn filters_label_counts_recognised_fields(cx: &mut TestAppContext) {
        let (state, toolbar, _events, cx) = rooted_toolbar(cx, 1280.0);

        set_query(&state, "state:stopped owner:x", cx);
        assert_eq!(filters_label(&toolbar, cx), "2 filters");

        set_query(&state, "state:stopped", cx);
        assert_eq!(filters_label(&toolbar, cx), "1 filter");

        set_query(&state, "", cx);
        assert_eq!(filters_label(&toolbar, cx), "Filters");

        set_query(&state, "web", cx);
        assert_eq!(filters_label(&toolbar, cx), "Filters");
    }

    #[gpui::test]
    fn rendered_filters_follow_the_query(cx: &mut TestAppContext) {
        let (state, toolbar, _events, cx) = rooted_toolbar(cx, 1280.0);

        set_query(&state, "state:stopped owner:x", cx);
        assert_eq!(
            rendered_filters(&toolbar, cx),
            ("2 filters".to_string(), true)
        );

        set_query(&state, "", cx);
        assert_eq!(
            rendered_filters(&toolbar, cx),
            ("Filters".to_string(), false)
        );
    }

    #[gpui::test]
    fn clicking_the_sidebar_button_emits_toggle_sidebar(cx: &mut TestAppContext) {
        let (_state, _toolbar, events, cx) = rooted_toolbar(cx, 1280.0);

        let button = cx
            .debug_bounds("toolbar.sidebar")
            .expect("sidebar button renders");
        cx.simulate_click(button.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(*events.borrow(), vec![ToolbarEvent::ToggleSidebar]);
    }

    #[gpui::test]
    fn clicking_the_filters_button_emits_filters_clicked(cx: &mut TestAppContext) {
        let (_state, _toolbar, events, cx) = rooted_toolbar(cx, 1280.0);

        let button = cx
            .debug_bounds("toolbar.filters")
            .expect("filters button renders");
        cx.simulate_click(button.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(*events.borrow(), vec![ToolbarEvent::FiltersClicked]);
    }

    #[cfg(target_os = "linux")]
    #[gpui::test]
    fn linux_toolbar_shows_window_controls_and_app_menu(cx: &mut TestAppContext) {
        let (_state, _toolbar, _events, cx) = rooted_toolbar(cx, 1280.0);

        assert!(cx.debug_bounds("linux.min").is_some());
        assert!(cx.debug_bounds("linux.max").is_some());
        assert!(cx.debug_bounds("linux.close").is_some());
        assert!(cx.debug_bounds("linux.menu").is_some());
    }

    #[cfg(target_os = "linux")]
    #[gpui::test]
    fn clicking_the_linux_menu_lists_app_menu_items(cx: &mut TestAppContext) {
        let (_state, _toolbar, _events, cx) = rooted_toolbar(cx, 1280.0);

        assert!(cx.debug_bounds("linux.menu.item.0").is_none());
        let button = cx
            .debug_bounds("linux.menu")
            .expect("app menu button renders");
        cx.simulate_click(button.center(), Default::default());
        cx.run_until_parked();

        assert!(cx.debug_bounds("linux.menu.item.0").is_some());
    }

    #[cfg(target_os = "macos")]
    #[gpui::test]
    fn macos_toolbar_has_no_linux_chrome(cx: &mut TestAppContext) {
        let (_state, _toolbar, _events, cx) = rooted_toolbar(cx, 1280.0);

        assert!(cx.debug_bounds("linux.menu").is_none());
        assert!(cx.debug_bounds("linux.min").is_none());
        assert!(cx.debug_bounds("linux.max").is_none());
        assert!(cx.debug_bounds("linux.close").is_none());
    }
}
