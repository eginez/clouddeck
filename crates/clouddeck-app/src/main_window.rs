//! Main window layout: toolbar, sidebar, table or centered state, inspector, status bar (design/images/01, 04, 41-43).

use clouddeck_core::{
    CenterContext, CenterInput, CenterState, CenterText, CloudDeckError,
    ComputeServicePartialFailure, RegionScope, center_state, center_text, layout_rules,
    region_scope_label, updated_text,
};
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Entity, IntoElement, Pixels, Render,
    SharedString, TitlebarOptions, Window, WindowAppearance, WindowBounds, WindowOptions, div,
    hsla, point, prelude::*, px, relative, size,
};

use crate::center_state_view::CenterStateView;
use crate::context_bar::{ContextBar, ContextBarEvent};
use crate::fleet_state::FleetState;
use crate::inspector_pane::InspectorPane;
use crate::instance_table::InstanceTable;
use crate::search_field::SearchField;
use crate::sidebar::Sidebar;
use crate::toolbar::{Toolbar, ToolbarEvent};
use crate::tokens::{FONT_SIZE, INSPECTOR_WIDTH, Palette, SIDEBAR_WIDTH, UI_FONT};

const OVERLAY_MAX_FRACTION: f32 = 0.92;
const SCRIM_ALPHA: f32 = 0.2;
const MIN_WINDOW_WIDTH: f32 = 520.0;
const MIN_WINDOW_HEIGHT: f32 = 400.0;

pub struct MainWindow {
    pub fleet: Entity<FleetState>,
    pub scope: RegionScope,
    pub available_regions: Vec<String>,
    pub failures: Vec<ComputeServicePartialFailure>,
    toolbar: Entity<Toolbar>,
    sidebar: Entity<Sidebar>,
    table: Entity<InstanceTable>,
    inspector: Entity<InspectorPane>,
    context_bar: Entity<ContextBar>,
    center: Entity<CenterStateView>,
    /// Whether the window was narrow on the last render, to reset panes only when it crosses.
    last_narrow: Option<bool>,
    /// Elements drawn by the last render, so tests can see removals that `debug_bounds` keeps.
    #[cfg(test)]
    rendered: RenderedPanes,
}

#[cfg(test)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct RenderedPanes {
    sidebar: bool,
    inspector: bool,
    scrim: bool,
    center_state: bool,
}

impl MainWindow {
    pub fn new(
        fleet: Entity<FleetState>,
        available_regions: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let scope = RegionScope::all(&available_regions);
        let search = cx.new(|cx| SearchField::new(fleet.clone(), window, cx));
        let toolbar = cx.new(|cx| Toolbar::new(fleet.clone(), search, cx));
        let sidebar = cx.new(|cx| Sidebar::new(fleet.clone(), cx));
        let table = cx.new(|cx| InstanceTable::new(fleet.clone(), cx));
        let inspector = cx.new(|cx| InspectorPane::new(fleet.clone(), cx));
        let context_bar = cx.new(|cx| ContextBar::new(fleet.clone(), cx));
        let center = cx.new(|_| {
            CenterStateView::new(
                CenterState::Loading,
                CenterText {
                    title: String::new(),
                    body: String::new(),
                    action: None,
                    command: None,
                },
            )
        });

        cx.observe(&fleet, |_, _, cx| cx.notify()).detach();
        cx.subscribe(&toolbar, |this, _, event: &ToolbarEvent, cx| match event {
            ToolbarEvent::ToggleSidebar => this.toggle_sidebar(cx),
            ToolbarEvent::FiltersClicked => {} // UI 21
        })
        .detach();
        cx.subscribe(&context_bar, |this, _, event: &ContextBarEvent, cx| match event {
            ContextBarEvent::ToggleSidebar => this.toggle_sidebar(cx),
            ContextBarEvent::ToggleInspector => this.toggle_inspector(cx),
            ContextBarEvent::Refresh => this.refresh(cx),
            ContextBarEvent::OpenProfilePicker
            | ContextBarEvent::OpenRegionPicker
            | ContextBarEvent::OpenFailureDetails => {} // UI 21
        })
        .detach();

        Self {
            fleet,
            scope,
            available_regions,
            failures: Vec::new(),
            toolbar,
            sidebar,
            table,
            inspector,
            context_bar,
            center,
            last_narrow: None,
            #[cfg(test)]
            rendered: RenderedPanes::default(),
        }
    }

    fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.fleet.update(cx, |fleet, cx| {
            fleet.sidebar_open = !fleet.sidebar_open;
            cx.notify();
        });
    }

    fn toggle_inspector(&mut self, cx: &mut Context<Self>) {
        self.fleet.update(cx, |fleet, cx| {
            fleet.inspector_open = !fleet.inspector_open;
            cx.notify();
        });
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.fleet.update(cx, |fleet, cx| fleet.refresh(cx));
    }

    fn close_panes(&mut self, cx: &mut Context<Self>) {
        self.fleet.update(cx, |fleet, cx| {
            fleet.sidebar_open = false;
            fleet.inspector_open = false;
            cx.notify();
        });
    }

    fn sync_context_bar(&self, cx: &mut Context<Self>) {
        let region_label = region_scope_label(self.scope.selected(), &self.available_regions);
        let failed_regions: Vec<String> = self
            .failures
            .iter()
            .map(|failure| failure.region.clone())
            .collect();
        let last_success_text = updated_text(
            self.fleet.read(cx).view_model.last_success(),
            local_offset_minutes(),
        );
        self.context_bar.update(cx, |bar, cx| {
            if bar.region_label == region_label
                && bar.failed_regions == failed_regions
                && bar.last_success_text == last_success_text
            {
                return;
            }
            bar.region_label = region_label;
            bar.failed_regions = failed_regions;
            bar.last_success_text = last_success_text;
            cx.notify();
        });
    }

    fn center_state_for(&self, cx: &App) -> Option<(CenterState, CenterText)> {
        let fleet = self.fleet.read(cx);
        let input = CenterInput {
            is_loading: fleet.view_model.is_loading(),
            loaded_count: fleet.view_model.instances().len(),
            shown_count: fleet.visible_rows().len(),
            error: fleet
                .view_model
                .error_message()
                .map(|message| CloudDeckError::Unknown(message.to_string())),
        };
        let state = center_state(&input)?;
        let context = CenterContext {
            profile: fleet.profile.clone(),
            scope_label: region_scope_label(self.scope.selected(), &self.available_regions),
            scope_is_all: self.scope.is_all(&self.available_regions),
            query: fleet.query().to_string(),
            last_good: None,
            retry_in_seconds: None,
        };
        let text = center_text(&state, &context);
        Some((state, text))
    }

    fn sidebar_pane(&self, overlay: bool) -> AnyElement {
        let pane = div()
            .debug_selector(|| "main.sidebar".into())
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .child(self.sidebar.clone());
        if overlay {
            pane.absolute().top_0().bottom_0().left_0().into_any_element()
        } else {
            pane.into_any_element()
        }
    }

    fn inspector_pane(&self, overlay: bool) -> AnyElement {
        let pane = div()
            .debug_selector(|| "main.inspector".into())
            .w(px(INSPECTOR_WIDTH))
            .h_full()
            .child(self.inspector.clone());
        if overlay {
            pane.absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .max_w(relative(OVERLAY_MAX_FRACTION))
                .into_any_element()
        } else {
            pane.into_any_element()
        }
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let palette = Palette::for_dark(dark);
        let viewport = window.viewport_size();
        let rules = layout_rules(f32::from(viewport.width), f32::from(viewport.height));

        if self.last_narrow != Some(rules.narrow) {
            self.last_narrow = Some(rules.narrow);
            self.fleet.update(cx, |fleet, _| {
                fleet.sidebar_open = rules.panes_open_by_default;
                fleet.inspector_open = rules.panes_open_by_default;
            });
        }
        self.sync_context_bar(cx);

        let center_content = self.center_state_for(cx);
        let (sidebar_open, inspector_visible) = {
            let fleet = self.fleet.read(cx);
            (
                fleet.sidebar_open,
                fleet.inspector_open && fleet.selected_instance().is_some(),
            )
        };
        let overlays = rules.panes_are_overlays;
        let scrim_visible = overlays && (sidebar_open || inspector_visible);
        #[cfg(test)]
        let center_state_visible = center_content.is_some();

        let center = match center_content {
            Some((state, text)) => {
                self.center.update(cx, |view, _| {
                    view.state = state;
                    view.text = text;
                });
                div()
                    .debug_selector(|| "main.center-state".into())
                    .size_full()
                    .child(self.center.clone())
                    .into_any_element()
            }
            None => div()
                .debug_selector(|| "main.table".into())
                .size_full()
                .child(self.table.clone())
                .into_any_element(),
        };
        let center = div()
            .debug_selector(|| "main.center".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .child(center);

        let scrim = scrim_visible.then(|| {
            div()
                .id("main.scrim")
                .debug_selector(|| "main.scrim".into())
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .right_0()
                .bg(hsla(0.0, 0.0, 0.0, SCRIM_ALPHA))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_panes(cx)))
        });

        let mut middle = div().flex().flex_1().min_h_0().relative();
        if overlays {
            middle = middle
                .child(center)
                .children(scrim)
                .children(sidebar_open.then(|| self.sidebar_pane(true)))
                .children(inspector_visible.then(|| self.inspector_pane(true)));
        } else {
            middle = middle
                .children(sidebar_open.then(|| self.sidebar_pane(false)))
                .child(center)
                .children(inspector_visible.then(|| self.inspector_pane(false)));
        }

        #[cfg(test)]
        {
            self.rendered = RenderedPanes {
                sidebar: sidebar_open,
                inspector: inspector_visible,
                scrim: scrim_visible,
                center_state: center_state_visible,
            };
        }

        div()
            .debug_selector(|| "main.root".into())
            .flex()
            .flex_col()
            .size_full()
            .bg(palette.win)
            .font_family(UI_FONT)
            .text_size(px(FONT_SIZE))
            .text_color(palette.text)
            .child(
                div()
                    .debug_selector(|| "main.toolbar".into())
                    .w_full()
                    .child(self.toolbar.clone()),
            )
            .child(middle)
            .child(
                div()
                    .debug_selector(|| "main.statusbar".into())
                    .w_full()
                    .child(self.context_bar.clone()),
            )
    }
}

pub fn window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(SharedString::from("CloudDeck")),
            appears_transparent: true,
            traffic_light_position: Some(point(px(14.0), px(15.0))),
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(size(px(MIN_WINDOW_WIDTH), px(MIN_WINDOW_HEIGHT))),
        ..Default::default()
    }
}

fn local_offset_minutes() -> i32 {
    chrono::Local::now().offset().local_minus_utc() / 60
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    use clouddeck_core::{EC2Instance, EC2InstanceTag, MockComputeService};
    use gpui::{Bounds, Entity, TestAppContext, VisualTestContext, point, size};

    use super::*;
    use crate::view_models::InstanceListViewModel;

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

    fn three_instances() -> Vec<EC2Instance> {
        vec![
            instance("i-1", "api-server", "running"),
            instance("i-2", "bastion", "stopped"),
            instance("i-3", "worker", "stopped"),
        ]
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

    /// Opens the main window at the given size with a refreshed fleet of `instances`.
    fn rooted_main_window(
        cx: &mut TestAppContext,
        instances: Vec<EC2Instance>,
        width: f32,
        height: f32,
    ) -> (Entity<FleetState>, Entity<MainWindow>, &mut VisualTestContext) {
        let vm = InstanceListViewModel::new(Box::new(MockComputeService::new(instances, None)));
        let fleet = cx.new(|_| FleetState::new(vm, "default".to_string(), true));
        fleet.update(cx, |fleet, cx| fleet.refresh(cx));
        wait_until(cx, |cx| {
            fleet.read_with(cx, |fleet, _| fleet.view_model.last_success().is_some())
        });

        let main_slot: Rc<RefCell<Option<Entity<MainWindow>>>> = Rc::new(RefCell::new(None));
        let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(width), px(height)));
        let slot_for_window = main_slot.clone();
        let fleet_for_window = fleet.clone();
        let window = cx.update(|cx| {
            cx.open_window(window_options(bounds), move |window, cx| {
                gpui_component::init(cx);
                let main = cx.new(|cx| {
                    MainWindow::new(
                        fleet_for_window,
                        vec!["us-east-1".to_string()],
                        window,
                        cx,
                    )
                });
                *slot_for_window.borrow_mut() = Some(main.clone());
                cx.new(|cx| gpui_component::Root::new(main, window, cx))
            })
            .unwrap()
        });
        let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
        cx.run_until_parked();
        let main = main_slot
            .borrow()
            .as_ref()
            .expect("main window entity should be created")
            .clone();
        (fleet, main, cx)
    }

    fn set_query(fleet: &Entity<FleetState>, query: &str, cx: &mut VisualTestContext) {
        fleet.update(cx, |fleet, cx| fleet.set_query(query, cx));
        cx.run_until_parked();
    }

    fn click_center_of(selector: &'static str, cx: &mut VisualTestContext) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should render"));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    }

    fn rendered(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> RenderedPanes {
        main.read_with(cx, |main, _| main.rendered)
    }

    #[gpui::test]
    fn wide_window_with_nothing_selected_shows_panes_and_table(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

        assert!(cx.debug_bounds("main.toolbar").is_some());
        assert!(cx.debug_bounds("main.sidebar").is_some());
        assert!(cx.debug_bounds("main.table").is_some());
        assert!(cx.debug_bounds("main.statusbar").is_some());
        assert!(cx.debug_bounds("main.inspector").is_none());
        assert!(cx.debug_bounds("main.scrim").is_none());
    }

    #[gpui::test]
    fn selecting_a_row_shows_the_inspector(cx: &mut TestAppContext) {
        let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

        click_center_of("table.row.us-east-1:i-1", cx);

        assert!(rendered(&main, cx).inspector);
        assert!(cx.debug_bounds("main.inspector").is_some());
    }

    #[gpui::test]
    fn narrow_window_shows_inspector_as_overlay_with_scrim(cx: &mut TestAppContext) {
        let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 700.0, 760.0);
        assert!(!rendered(&main, cx).inspector);

        fleet.update(cx, |fleet, cx| fleet.select(Some("us-east-1:i-1".to_string()), cx));
        cx.run_until_parked();

        assert!(rendered(&main, cx).inspector);
        assert!(rendered(&main, cx).scrim);
        assert!(cx.debug_bounds("main.inspector").is_some());
    }

    #[gpui::test]
    fn widening_a_narrow_window_reopens_the_sidebar(cx: &mut TestAppContext) {
        let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

        cx.simulate_resize(size(px(700.0), px(760.0)));
        cx.run_until_parked();
        assert!(!rendered(&main, cx).sidebar);

        cx.simulate_resize(size(px(1280.0), px(760.0)));
        cx.run_until_parked();
        assert!(rendered(&main, cx).sidebar);
        assert!(!rendered(&main, cx).scrim);
    }

    #[gpui::test]
    fn query_with_no_matches_shows_the_centered_state_instead_of_the_table(
        cx: &mut TestAppContext,
    ) {
        let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
        assert!(!rendered(&main, cx).center_state);

        set_query(&fleet, "zzz-no-match", cx);

        assert!(rendered(&main, cx).center_state);
    }

    #[gpui::test]
    fn empty_fleet_shows_the_centered_state(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = rooted_main_window(cx, vec![], 1280.0, 760.0);

        assert!(cx.debug_bounds("main.center-state").is_some());
        assert!(cx.debug_bounds("main.table").is_none());
    }

    #[gpui::test]
    fn narrow_window_closes_panes_and_scrim_click_closes_overlays(cx: &mut TestAppContext) {
        let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

        cx.simulate_resize(size(px(700.0), px(760.0)));
        cx.run_until_parked();
        assert!(!rendered(&main, cx).sidebar);

        fleet.update(cx, |fleet, cx| {
            fleet.sidebar_open = true;
            cx.notify();
        });
        cx.run_until_parked();
        assert_eq!(
            rendered(&main, cx),
            RenderedPanes {
                sidebar: true,
                inspector: false,
                scrim: true,
                center_state: false,
            }
        );

        click_center_of("main.scrim", cx);

        assert!(!rendered(&main, cx).sidebar);
        assert!(!rendered(&main, cx).scrim);
    }

    #[gpui::test]
    fn toolbar_sidebar_button_hides_the_sidebar(cx: &mut TestAppContext) {
        let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
        assert!(rendered(&main, cx).sidebar);

        click_center_of("toolbar.sidebar", cx);

        assert!(!rendered(&main, cx).sidebar);
    }
}
