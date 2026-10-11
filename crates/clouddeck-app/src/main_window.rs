//! Main window layout: toolbar, sidebar, table or centered state, inspector, status bar (design/images/01, 04, 41-43).

use clouddeck_core::{
    CenterContext, CenterInput, CenterState, CenterText, CloudDeckError, CommandContext, CommandId,
    ComputeServicePartialFailure, EC2Instance, EscapeAction, EscapeContext, OPEN_IN_CONSOLE_FLASH,
    ProfileInput, RegionScope, center_state, center_text, clock_text, console_url_for,
    copied_flash, escape_action, filter_command_query, known_tag_keys, layout_rules,
    region_scope_label, stale_count, updated_text,
};
use gpui::{
    AnyElement, AnyView, App, Bounds, ClickEvent, ClipboardItem, Context, Div, Entity,
    EventEmitter, FocusHandle, Focusable, Global, IntoElement, KeyBinding, MouseDownEvent, Pixels,
    Point, Render, SharedString, Subscription, TitlebarOptions, Window, WindowAppearance,
    WindowBounds, WindowOptions, actions, div, hsla, point, prelude::*, px, relative, size,
};

use crate::app_menus;
use crate::center_state_view::CenterStateView;
use crate::command_palette::{CommandPalette, PaletteEvent};
use crate::context_bar::{ContextBar, ContextBarEvent};
use crate::failure_popover::{FailurePopover, FailurePopoverEvent, RegionFailure};
use crate::fleet_state::FleetState;
use crate::inspector_pane::InspectorPane;
use crate::instance_table::{InstanceTable, InstanceTableEvent, table_key_bindings};
use crate::profile_picker_view::{ProfilePicker, ProfilePickerEvent};
use crate::region_picker_view::{RegionPicker, RegionPickerEvent};
use crate::row_context_menu::{RowContextMenu, RowMenuEvent};
use crate::search_field::{SearchField, search_key_bindings};
use crate::sidebar::Sidebar;
use crate::tokens::{
    FONT_SIZE, INSPECTOR_WIDTH, Palette, SIDEBAR_WIDTH, STATUS_BAR_HEIGHT, UI_FONT,
};
use crate::toolbar::{Toolbar, ToolbarEvent};

actions!(main_window, [EscapePressed]);

const OVERLAY_MAX_FRACTION: f32 = 0.92;
const SCRIM_ALPHA: f32 = 0.2;
const MIN_WINDOW_WIDTH: f32 = 520.0;
const MIN_WINDOW_HEIGHT: f32 = 400.0;
const OVERLAY_ABOVE_STATUS_BAR: f32 = STATUS_BAR_HEIGHT + 4.0;
const REGION_OVERLAY_LEFT: f32 = 150.0;
const PALETTE_TOP: f32 = 80.0;
const OVERLAY_EDGE_INSET: f32 = 8.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Overlay {
    Profile,
    Region,
    Palette,
    RowMenu {
        key: String,
        position: Point<Pixels>,
    },
    Failure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MainWindowEvent {
    PickProfile(String),
    ConnectToAws,
    RegionsChanged(Vec<String>),
    Retry(Option<String>),
    OpenSettings,
}

struct MenusInstalled;

impl Global for MenusInstalled {}

pub struct MainWindow {
    pub fleet: Entity<FleetState>,
    pub scope: RegionScope,
    pub available_regions: Vec<String>,
    pub failures: Vec<ComputeServicePartialFailure>,
    pub profiles: Vec<ProfileInput>,
    pub overlay: Option<Overlay>,
    toolbar: Entity<Toolbar>,
    sidebar: Entity<Sidebar>,
    table: Entity<InstanceTable>,
    inspector: Entity<InspectorPane>,
    context_bar: Entity<ContextBar>,
    center: Entity<CenterStateView>,
    search: Entity<SearchField>,
    focus_handle: FocusHandle,
    overlay_view: Option<AnyView>,
    overlay_subscription: Option<Subscription>,
    region_scope_dirty: bool,
    /// Whether the window was narrow on the last render, to reset panes only when it crosses.
    last_narrow: Option<bool>,
    /// Elements drawn by the last render, so tests can see removals that `debug_bounds` keeps.
    #[cfg(test)]
    rendered: RenderedPanes,
    #[cfg(test)]
    rendered_overlay: Option<&'static str>,
}

impl EventEmitter<MainWindowEvent> for MainWindow {}

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
        scope: RegionScope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        if !cx.has_global::<MenusInstalled>() {
            app_menus::install(cx);
            cx.bind_keys(table_key_bindings());
            cx.bind_keys(search_key_bindings());
            cx.bind_keys([KeyBinding::new("escape", EscapePressed, Some("MainWindow"))]);
            cx.set_global(MenusInstalled);
        }
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);
        let viewport = window.viewport_size();
        let rules = layout_rules(f32::from(viewport.width), f32::from(viewport.height));
        fleet.update(cx, |fleet, _| {
            fleet.sidebar_open &= rules.panes_open_by_default;
            fleet.inspector_open &= rules.panes_open_by_default;
        });
        let search = cx.new(|cx| SearchField::new(fleet.clone(), window, cx));
        let toolbar = cx.new(|cx| Toolbar::new(fleet.clone(), search.clone(), cx));
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
            ToolbarEvent::FiltersClicked => {}
        })
        .detach();
        cx.subscribe_in(
            &context_bar,
            window,
            |this, _, event: &ContextBarEvent, window, cx| match event {
                ContextBarEvent::ToggleSidebar => this.toggle_sidebar(cx),
                ContextBarEvent::ToggleInspector => this.toggle_inspector(cx),
                ContextBarEvent::Refresh => this.refresh(cx),
                ContextBarEvent::OpenProfilePicker => {
                    this.toggle_overlay(Overlay::Profile, window, cx)
                }
                ContextBarEvent::OpenRegionPicker => {
                    this.toggle_overlay(Overlay::Region, window, cx)
                }
                ContextBarEvent::OpenFailureDetails => {
                    this.toggle_overlay(Overlay::Failure, window, cx)
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &table,
            window,
            |this, _, event: &InstanceTableEvent, window, cx| {
                let InstanceTableEvent::RowContextMenu { key, position } = event;
                this.open_row_menu(key.clone(), *position, window, cx);
            },
        )
        .detach();

        Self {
            fleet,
            scope,
            available_regions,
            failures: Vec::new(),
            profiles: Vec::new(),
            overlay: None,
            toolbar,
            sidebar,
            table,
            inspector,
            context_bar,
            center,
            search,
            focus_handle,
            overlay_view: None,
            overlay_subscription: None,
            region_scope_dirty: false,
            last_narrow: Some(rules.narrow),
            #[cfg(test)]
            rendered: RenderedPanes::default(),
            #[cfg(test)]
            rendered_overlay: None,
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

    fn toggle_overlay(&mut self, overlay: Overlay, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay.as_ref() == Some(&overlay) {
            self.close_overlay(window, cx);
        } else {
            self.open_overlay(overlay, window, cx);
        }
    }

    fn open_overlay(&mut self, overlay: Overlay, window: &mut Window, cx: &mut Context<Self>) {
        let (view, subscription): (AnyView, Subscription) = match &overlay {
            Overlay::Profile => {
                let profiles = self.profiles.clone();
                let current = self.fleet.read(cx).profile.clone();
                let picker = cx.new(|cx| ProfilePicker::new(profiles, current, window, cx));
                let subscription = cx.subscribe_in(
                    &picker,
                    window,
                    |this, _, event: &ProfilePickerEvent, window, cx| match event {
                        ProfilePickerEvent::Picked(name) => {
                            this.close_overlay(window, cx);
                            cx.emit(MainWindowEvent::PickProfile(name.clone()));
                        }
                        ProfilePickerEvent::ConnectToAws => {
                            this.close_overlay(window, cx);
                            cx.emit(MainWindowEvent::ConnectToAws);
                        }
                        ProfilePickerEvent::Dismissed => this.close_overlay(window, cx),
                    },
                );
                (picker.into(), subscription)
            }
            Overlay::Region => {
                let scope = self.scope.clone();
                let available = self.available_regions.clone();
                let counts = self.region_counts(cx);
                let picker = cx.new(|cx| RegionPicker::new(scope, available, counts, window, cx));
                let subscription = cx.subscribe_in(
                    &picker,
                    window,
                    |this, _, event: &RegionPickerEvent, window, cx| match event {
                        RegionPickerEvent::Changed(regions) => {
                            this.scope =
                                RegionScope::from_selected(regions, &this.available_regions);
                            this.region_scope_dirty = true;
                            cx.notify();
                        }
                        RegionPickerEvent::Dismissed => this.close_overlay(window, cx),
                    },
                );
                (picker.into(), subscription)
            }
            Overlay::Palette => {
                let context = self.command_context(cx);
                let palette = cx.new(|cx| CommandPalette::new(context, window, cx));
                let subscription = cx.subscribe_in(
                    &palette,
                    window,
                    |this, _, event: &PaletteEvent, window, cx| match event {
                        PaletteEvent::Run(id) => {
                            this.close_overlay(window, cx);
                            this.run_command(*id, window, cx);
                        }
                        PaletteEvent::Dismissed => this.close_overlay(window, cx),
                    },
                );
                (palette.into(), subscription)
            }
            Overlay::RowMenu { .. } => {
                let context = self.command_context(cx);
                let menu = cx.new(|_| RowContextMenu::new(context));
                let subscription = cx.subscribe_in(
                    &menu,
                    window,
                    |this, _, event: &RowMenuEvent, window, cx| match event {
                        RowMenuEvent::Run(id) => {
                            this.close_overlay(window, cx);
                            this.run_command(*id, window, cx);
                        }
                        RowMenuEvent::Dismissed => this.close_overlay(window, cx),
                    },
                );
                (menu.into(), subscription)
            }
            Overlay::Failure => {
                let (failures, refreshed_at, other_regions) = self.failure_popover_data(cx);
                let popover =
                    cx.new(|_| FailurePopover::new(failures, refreshed_at, other_regions));
                let subscription = cx.subscribe_in(
                    &popover,
                    window,
                    |this, _, event: &FailurePopoverEvent, window, cx| match event {
                        FailurePopoverEvent::Retry(region) => {
                            cx.emit(MainWindowEvent::Retry(Some(region.clone())));
                        }
                        FailurePopoverEvent::RetryAll => {
                            cx.emit(MainWindowEvent::Retry(None));
                        }
                        FailurePopoverEvent::Copied => {}
                        FailurePopoverEvent::Dismissed => this.close_overlay(window, cx),
                    },
                );
                (popover.into(), subscription)
            }
        };
        self.overlay = Some(overlay);
        self.overlay_view = Some(view);
        self.overlay_subscription = Some(subscription);
        cx.notify();
    }

    fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_region_scope(cx);
        self.overlay = None;
        self.overlay_view = None;
        self.overlay_subscription = None;
        self.focus_handle.focus(window);
        cx.notify();
    }

    /// Announces a region selection once the picker that made it is no longer open,
    /// so the shell rebuilds the fleet without tearing down the picker mid-interaction.
    fn commit_region_scope(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.region_scope_dirty) {
            cx.emit(MainWindowEvent::RegionsChanged(
                self.scope.selected().to_vec(),
            ));
        }
    }

    fn open_row_menu(
        &mut self,
        key: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.fleet
            .update(cx, |fleet, cx| fleet.select(Some(key.clone()), cx));
        self.open_overlay(Overlay::RowMenu { key, position }, window, cx);
    }

    fn command_context(&self, cx: &App) -> CommandContext {
        let fleet = self.fleet.read(cx);
        let selected = fleet.selected_instance();
        CommandContext {
            has_selection: selected.is_some(),
            selection_has_private_ip: selected
                .as_ref()
                .is_some_and(|instance| instance.private_ip_address.is_some()),
            selection_has_public_ip: selected
                .as_ref()
                .is_some_and(|instance| instance.public_ip_address.is_some()),
            query_is_empty: fleet.query().is_empty(),
            has_failed_region: !self.failures.is_empty(),
        }
    }

    fn region_counts(&self, cx: &App) -> Vec<(String, usize)> {
        let instances = self.fleet.read(cx).view_model.instances();
        self.available_regions
            .iter()
            .map(|region| (region.clone(), stale_count(instances, region)))
            .collect()
    }

    fn region_failures(&self, cx: &App) -> Vec<RegionFailure> {
        let instances = self.fleet.read(cx).view_model.instances();
        self.failures
            .iter()
            .map(|failure| RegionFailure {
                region: failure.region.clone(),
                message: failure.message.clone(),
                error: CloudDeckError::Unknown(failure.message.clone()),
                stale_instances: stale_count(instances, &failure.region),
                retry_in_seconds: None,
            })
            .collect()
    }

    fn failure_popover_data(&self, cx: &App) -> (Vec<RegionFailure>, String, usize) {
        let failures = self.region_failures(cx);
        let refreshed_at = self
            .fleet
            .read(cx)
            .view_model
            .last_success()
            .map(|at| clock_text(at, local_offset_minutes()))
            .unwrap_or_default();
        let other_regions = self.available_regions.len().saturating_sub(failures.len());
        (failures, refreshed_at, other_regions)
    }

    fn sync_failure_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay != Some(Overlay::Failure) {
            return;
        }
        let Some(popover) = self
            .overlay_view
            .clone()
            .and_then(|view| view.downcast::<FailurePopover>().ok())
        else {
            return;
        };
        let (failures, refreshed_at, other_regions) = self.failure_popover_data(cx);
        if failures.is_empty() {
            self.close_overlay(window, cx);
            return;
        }
        popover.update(cx, |popover, cx| {
            popover.failures = failures;
            popover.refreshed_at = refreshed_at;
            popover.other_regions = other_regions;
            cx.notify();
        });
    }

    fn run_command(&mut self, id: CommandId, window: &mut Window, cx: &mut Context<Self>) {
        match id {
            CommandId::SwitchProfile => self.open_overlay(Overlay::Profile, window, cx),
            CommandId::SwitchRegion => self.open_overlay(Overlay::Region, window, cx),
            CommandId::Refresh => self.refresh(cx),
            CommandId::ToggleSidebar => self.toggle_sidebar(cx),
            CommandId::ToggleInspector => self.toggle_inspector(cx),
            CommandId::FilterRunning
            | CommandId::FilterStopped
            | CommandId::FilterPending
            | CommandId::ClearFilters => {
                let fleet = self.fleet.read(cx);
                let known = known_tag_keys(fleet.view_model.instances());
                if let Some(query) = filter_command_query(id, fleet.query(), &known) {
                    self.fleet
                        .update(cx, |fleet, cx| fleet.set_query(&query, cx));
                }
            }
            CommandId::CopyId => {
                self.copy_selected(|instance| Some(instance.instance_id.clone()), cx)
            }
            CommandId::CopyPrivateIp => {
                self.copy_selected(|instance| instance.private_ip_address.clone(), cx)
            }
            CommandId::CopyPublicIp => {
                self.copy_selected(|instance| instance.public_ip_address.clone(), cx)
            }
            CommandId::OpenInConsole => self.open_selected_in_console(cx),
            CommandId::RetryFailedRegion => cx.emit(MainWindowEvent::Retry(None)),
            CommandId::ConnectToAws => cx.emit(MainWindowEvent::ConnectToAws),
            CommandId::OpenSettings => cx.emit(MainWindowEvent::OpenSettings),
        }
    }

    fn copy_selected(&self, value: fn(&EC2Instance) -> Option<String>, cx: &mut Context<Self>) {
        let Some(instance) = self.fleet.read(cx).selected_instance() else {
            return;
        };
        let Some(text) = value(&instance) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        let flash = copied_flash(&text);
        self.fleet
            .update(cx, |fleet, cx| fleet.set_flash(flash, cx));
    }

    fn open_selected_in_console(&self, cx: &mut Context<Self>) {
        let Some(instance) = self.fleet.read(cx).selected_instance() else {
            return;
        };
        let flash = match console_url_for(&instance) {
            Ok(url) => {
                cx.open_url(&url);
                OPEN_IN_CONSOLE_FLASH.to_string()
            }
            Err(error) => error.to_string(),
        };
        self.fleet
            .update(cx, |fleet, cx| fleet.set_flash(flash, cx));
    }

    fn escape_pressed(&mut self, _: &EscapePressed, window: &mut Window, cx: &mut Context<Self>) {
        let search_focused = self
            .search
            .read(cx)
            .input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let fleet = self.fleet.read(cx);
        let ctx = EscapeContext {
            overlay_open: self.overlay.is_some(),
            search_focused,
            query_empty: fleet.query().is_empty(),
            narrow: self.last_narrow == Some(true),
            sidebar_open: fleet.sidebar_open,
            has_selection: fleet.selected.is_some(),
        };
        match escape_action(&ctx) {
            EscapeAction::CloseOverlay => self.close_overlay(window, cx),
            EscapeAction::ClearSearch => self.fleet.update(cx, |fleet, cx| fleet.set_query("", cx)),
            EscapeAction::CloseSidebarOverlay => self.fleet.update(cx, |fleet, cx| {
                fleet.sidebar_open = false;
                cx.notify();
            }),
            EscapeAction::ClearSelection => {
                self.fleet.update(cx, |fleet, cx| fleet.select(None, cx))
            }
            EscapeAction::Nothing => {}
        }
    }

    fn overlay_layer(&self, cx: &mut Context<Self>) -> Option<(&'static str, AnyElement)> {
        let overlay = self.overlay.clone()?;
        let view = self.overlay_view.clone()?;
        let (selector, placed) = match overlay {
            Overlay::Profile => (
                "main.overlay.profile",
                overlay_panel("main.overlay.profile", view)
                    .absolute()
                    .left(px(OVERLAY_EDGE_INSET))
                    .bottom(px(OVERLAY_ABOVE_STATUS_BAR)),
            ),
            Overlay::Region => (
                "main.overlay.region",
                overlay_panel("main.overlay.region", view)
                    .absolute()
                    .left(px(REGION_OVERLAY_LEFT))
                    .bottom(px(OVERLAY_ABOVE_STATUS_BAR)),
            ),
            Overlay::Palette => (
                "main.overlay.palette",
                div()
                    .absolute()
                    .top(px(PALETTE_TOP))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(overlay_panel("main.overlay.palette", view)),
            ),
            Overlay::RowMenu { position, .. } => (
                "main.overlay.rowmenu",
                overlay_panel("main.overlay.rowmenu", view)
                    .absolute()
                    .left(position.x)
                    .top(position.y),
            ),
            Overlay::Failure => (
                "main.overlay.failure",
                overlay_panel("main.overlay.failure", view)
                    .absolute()
                    .right(px(OVERLAY_EDGE_INSET))
                    .bottom(px(OVERLAY_ABOVE_STATUS_BAR)),
            ),
        };
        let catcher = div()
            .id("main.overlay.dismiss")
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .occlude()
            .on_any_mouse_down(
                cx.listener(|this, _: &MouseDownEvent, window, cx| this.close_overlay(window, cx)),
            );
        let layer = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left_0()
            .right_0()
            .child(catcher)
            .child(placed);
        Some((selector, layer.into_any_element()))
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
            pane.absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .into_any_element()
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
        self.sync_failure_popover(window, cx);

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

        let overlay = self.overlay_layer(cx);
        #[cfg(test)]
        {
            self.rendered_overlay = overlay.as_ref().map(|(selector, _)| *selector);
        }

        div()
            .key_context("MainWindow")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::escape_pressed))
            .on_action(
                cx.listener(|this, _: &app_menus::SwitchProfile, window, cx| {
                    this.open_overlay(Overlay::Profile, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &app_menus::SwitchRegion, window, cx| {
                    this.open_overlay(Overlay::Region, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &app_menus::OpenCommandPalette, window, cx| {
                    this.open_overlay(Overlay::Palette, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &app_menus::ToggleSidebar, _, cx| this.toggle_sidebar(cx)),
            )
            .on_action(
                cx.listener(|this, _: &app_menus::ToggleInspector, _, cx| {
                    this.toggle_inspector(cx)
                }),
            )
            .on_action(cx.listener(|this, _: &app_menus::RefreshFleet, _, cx| this.refresh(cx)))
            .on_action(cx.listener(|this, _: &app_menus::CopyInstanceId, _, cx| {
                this.copy_selected(|instance| Some(instance.instance_id.clone()), cx)
            }))
            .on_action(cx.listener(|this, _: &app_menus::OpenInConsole, _, cx| {
                this.open_selected_in_console(cx)
            }))
            .on_action(cx.listener(|_, _: &app_menus::OpenSettings, _, cx| {
                cx.emit(MainWindowEvent::OpenSettings)
            }))
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
            .children(overlay.map(|(_, layer)| layer))
    }
}

fn overlay_panel(selector: &'static str, view: AnyView) -> Div {
    div()
        .debug_selector(move || selector.into())
        .occlude()
        .child(view)
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
    use gpui::{Bounds, Entity, MouseButton, TestAppContext, VisualTestContext, point, size};

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

    /// Opens the main window at the given size with a refreshed fleet of `instances`.
    fn rooted_main_window(
        cx: &mut TestAppContext,
        instances: Vec<EC2Instance>,
        width: f32,
        height: f32,
    ) -> (
        Entity<FleetState>,
        Entity<MainWindow>,
        &mut VisualTestContext,
    ) {
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
                    let regions = vec!["us-east-1".to_string()];
                    let scope = RegionScope::all(&regions);
                    MainWindow::new(fleet_for_window, regions, scope, window, cx)
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

        fleet.update(cx, |fleet, cx| {
            fleet.select(Some("us-east-1:i-1".to_string()), cx)
        });
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

    fn rendered_overlay(
        main: &Entity<MainWindow>,
        cx: &mut VisualTestContext,
    ) -> Option<&'static str> {
        main.read_with(cx, |main, _| main.rendered_overlay)
    }

    fn overlay_view<T: 'static>(
        main: &Entity<MainWindow>,
        cx: &mut VisualTestContext,
    ) -> Entity<T> {
        main.read_with(cx, |main, _| main.overlay_view.clone())
            .expect("an overlay should be open")
            .downcast::<T>()
            .ok()
            .expect("open overlay should be the expected view")
    }

    #[gpui::test]
    fn switch_profile_opens_profile_overlay_and_escape_closes_it(cx: &mut TestAppContext) {
        let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

        cx.dispatch_action(app_menus::SwitchProfile);
        cx.run_until_parked();
        assert_eq!(rendered_overlay(&main, cx), Some("main.overlay.profile"));

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(rendered_overlay(&main, cx), None);
    }

    #[gpui::test]
    fn palette_run_filter_stopped_sets_query_and_closes_overlay(cx: &mut TestAppContext) {
        let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

        cx.dispatch_action(app_menus::OpenCommandPalette);
        cx.run_until_parked();
        assert_eq!(rendered_overlay(&main, cx), Some("main.overlay.palette"));

        let palette = overlay_view::<CommandPalette>(&main, cx);
        palette.update(cx, |_, cx| {
            cx.emit(PaletteEvent::Run(CommandId::FilterStopped))
        });
        cx.run_until_parked();

        assert_eq!(
            fleet.read_with(cx, |fleet, _| fleet.query().to_string()),
            "state:stopped"
        );
        assert_eq!(rendered_overlay(&main, cx), None);
    }

    #[gpui::test]
    fn copy_instance_id_copies_the_selected_row_and_flashes(cx: &mut TestAppContext) {
        let (fleet, _main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
        fleet.update(cx, |fleet, cx| {
            fleet.select(Some("us-east-1:i-1".to_string()), cx)
        });
        cx.run_until_parked();

        cx.dispatch_action(app_menus::CopyInstanceId);
        cx.run_until_parked();

        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some("i-1".to_string())
        );
        assert!(fleet.read_with(cx, |fleet, _| fleet.flash.is_some()));
    }

    #[gpui::test]
    fn clicking_failure_bar_opens_failure_overlay(cx: &mut TestAppContext) {
        let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
        main.update(cx, |main, cx| {
            main.failures = vec![ComputeServicePartialFailure {
                region: "eu-central-1".to_string(),
                message: "throttled".to_string(),
            }];
            cx.notify();
        });
        cx.run_until_parked();

        click_center_of("bar.failure", cx);

        assert_eq!(rendered_overlay(&main, cx), Some("main.overlay.failure"));
    }

    #[gpui::test]
    fn right_clicking_a_row_opens_row_menu_and_selects_it(cx: &mut TestAppContext) {
        let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
        let row = cx
            .debug_bounds("table.row.us-east-1:i-2")
            .expect("row should render");

        cx.simulate_mouse_down(row.center(), MouseButton::Right, Default::default());
        cx.run_until_parked();

        assert_eq!(rendered_overlay(&main, cx), Some("main.overlay.rowmenu"));
        assert_eq!(
            fleet.read_with(cx, |fleet, _| fleet.selected.clone()),
            Some("us-east-1:i-2".to_string())
        );
    }

    #[gpui::test]
    fn escape_with_selection_and_no_overlay_clears_selection(cx: &mut TestAppContext) {
        let (fleet, _main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
        fleet.update(cx, |fleet, cx| {
            fleet.select(Some("us-east-1:i-1".to_string()), cx)
        });
        cx.run_until_parked();

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();

        assert_eq!(fleet.read_with(cx, |fleet, _| fleet.selected.clone()), None);
    }

    #[gpui::test]
    fn region_picker_change_emits_regions_changed_once_dismissed(cx: &mut TestAppContext) {
        let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
        let events: Rc<RefCell<Vec<MainWindowEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&main, move |_, event: &MainWindowEvent, _| {
                sink.borrow_mut().push(event.clone())
            })
            .detach();
        });

        cx.dispatch_action(app_menus::SwitchRegion);
        cx.run_until_parked();
        let picker = overlay_view::<RegionPicker>(&main, cx);
        picker.update(cx, |_, cx| {
            cx.emit(RegionPickerEvent::Changed(vec!["us-east-1".to_string()]))
        });
        cx.run_until_parked();
        assert!(
            events.borrow().is_empty(),
            "the fleet should not reload while the picker is still open"
        );

        picker.update(cx, |_, cx| cx.emit(RegionPickerEvent::Dismissed));
        cx.run_until_parked();

        assert_eq!(
            *events.borrow(),
            vec![MainWindowEvent::RegionsChanged(vec![
                "us-east-1".to_string()
            ])]
        );
    }
}
