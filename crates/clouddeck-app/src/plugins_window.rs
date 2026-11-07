//! Plugins window: installed, bundled, dev and available plugins with their actions.

use std::collections::BTreeMap;
use std::path::PathBuf;

use clouddeck_core::extension_host::{
    Capability, bundled_extensions_root, extension_state_root, extensions_root,
};
use clouddeck_core::extension_manager::install::{EXTERNAL_EXTENSIONS_ALLOWED, Manager};
use clouddeck_core::extension_manager::plugins::{
    Action, Badge, CapabilityView, PluginEntry, PluginRowVm, PluginsViewModel, Slot, Tab,
    load_entries,
};
use gpui::{
    AnyElement, AnyView, App, Bounds, ClickEvent, Context, Div, Entity, FocusHandle, Global,
    IntoElement, KeyDownEvent, ParentElement, PathPromptOptions, Render, SharedString, Stateful,
    Styled, Subscription, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, div,
    point, prelude::*, px, size,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::tooltip::Tooltip;
use gpui_component::{Root, Sizable};

use crate::app_providers;
use crate::icons::AppIcon;
use crate::tokens::{MONO_FONT, Palette, palette};
use crate::updater;

pub(crate) const WINDOW_SIZE: (f32, f32) = (760.0, 520.0);
const LIST_WIDTH: f32 = 260.0;
const SEARCH_WIDTH: f32 = 220.0;
const TITLEBAR_INSET: f32 = 36.0;
const STATUS_HEIGHT: f32 = 24.0;

struct OpenPluginsWindow(WindowHandle<Root>);

impl Global for OpenPluginsWindow {}

/// Where the pane reads plugins from and keeps their state.
pub(crate) struct Roots {
    pub bundled: Option<PathBuf>,
    pub installed: PathBuf,
    pub state: PathBuf,
}

impl Roots {
    pub(crate) fn app() -> Self {
        Self {
            bundled: bundled_extensions_root(),
            installed: extensions_root(),
            state: extension_state_root(),
        }
    }
}

/// The pane's model over `entries`. No signed index exists yet, so Available stays empty.
pub(crate) fn model_for(entries: Vec<PluginEntry>) -> PluginsViewModel {
    PluginsViewModel::new(
        entries,
        Vec::new(),
        BTreeMap::new(),
        EXTERNAL_EXTENSIONS_ALLOWED,
    )
}

/// Opens the Plugins window with the plugins on disk, or focuses it when already open.
pub fn open(cx: &mut App) -> Option<WindowHandle<Root>> {
    if let Some(handle) = cx.try_global::<OpenPluginsWindow>().map(|open| open.0)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Some(handle);
    }
    let roots = Roots::app();
    let entries = load_entries(
        roots.bundled.as_deref(),
        &roots.installed,
        &roots.state,
        &app_providers::refusals(cx),
    )
    .ok()?;
    let handle = open_window_for(
        cx,
        model_for(entries),
        Manager::github(),
        roots,
        WINDOW_SIZE,
    )?;
    cx.set_global(OpenPluginsWindow(handle));
    Some(handle)
}

/// Always opens a new window, bypassing the single-instance check.
pub(crate) fn open_window_for(
    cx: &mut App,
    model: PluginsViewModel,
    manager: Manager,
    roots: Roots,
    window_size: (f32, f32),
) -> Option<WindowHandle<Root>> {
    let bounds = Bounds::centered(None, size(px(window_size.0), px(window_size.1)), cx);
    cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::from("Plugins")),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.0), px(15.0))),
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some(crate::main_window::APP_ID.to_string()),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| PluginsWindowView::new(model, manager, roots, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
    .ok()
}

#[derive(Debug, Clone)]
struct Notice {
    text: String,
    error: bool,
}

impl Notice {
    fn info(text: String) -> Self {
        Self { text, error: false }
    }

    fn error(text: String) -> Self {
        Self { text, error: true }
    }
}

struct PluginsWindowView {
    model: PluginsViewModel,
    manager: Manager,
    roots: Roots,
    search: Entity<InputState>,
    focus: FocusHandle,
    notice: Option<Notice>,
    _subscriptions: Vec<Subscription>,
}

impl PluginsWindowView {
    fn new(
        model: PluginsViewModel,
        manager: Manager,
        roots: Roots,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = model.query.clone();
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search plugins...")
                .default_value(query)
        });
        let subscription = cx.subscribe_in(
            &search,
            window,
            |this: &mut Self, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let query = state.read(cx).value().to_string();
                    this.model.set_query(&query);
                    cx.notify();
                }
            },
        );
        let focus = cx.focus_handle();
        focus.focus(window);
        Self {
            model,
            manager,
            roots,
            search,
            focus,
            notice: None,
            _subscriptions: vec![subscription],
        }
    }

    fn act(&mut self, id: &str, action: Action, cx: &mut Context<Self>) {
        let result = self.model.act(&self.manager, id, action);
        self.settle(result, cx);
    }

    fn install_dev(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                let result = this.model.install_dev(&this.manager, &path);
                this.settle(result, cx);
            });
        })
        .detach();
    }

    fn restart(&mut self, cx: &mut Context<Self>) {
        let relaunch = std::env::current_exe()
            .map_err(|error| error.to_string())
            .and_then(|exe| updater::spawn_relaunch(&exe));
        match relaunch {
            Ok(()) => cx.quit(),
            Err(message) => {
                self.notice = Some(Notice::error(message));
                cx.notify();
            }
        }
    }

    fn settle(&mut self, result: Result<String, String>, cx: &mut Context<Self>) {
        self.notice = Some(match result {
            Ok(text) => Notice::info(text),
            Err(text) => Notice::error(text),
        });
        self.reload(cx);
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let loaded = load_entries(
            self.roots.bundled.as_deref(),
            &self.roots.installed,
            &self.roots.state,
            &app_providers::refusals(cx),
        );
        match loaded {
            Ok(entries) => self.model.set_entries(entries),
            Err(error) => {
                let prefix = self
                    .notice
                    .take()
                    .map(|notice| notice.text + " ")
                    .unwrap_or_default();
                self.notice = Some(Notice::error(format!(
                    "{prefix}Could not read plugins: {error}"
                )));
            }
        }
        cx.notify();
    }

    fn restart_banner(&self, palette: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let names = self.model.restart_required_names();
        if names.is_empty() {
            return None;
        }
        Some(
            div()
                .debug_selector(|| "plugins.restart".into())
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(12.))
                .py(px(6.))
                .border_b_1()
                .border_color(palette.line2)
                .bg(palette.panel)
                .child("Restart CloudDeck to apply changes")
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.))
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_color(palette.dim)
                        .child(names.join(", ")),
                )
                .child(
                    button("plugins.restart.button", "Restart", palette)
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.restart(cx))),
                )
                .into_any_element(),
        )
    }

    fn header(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .px(px(12.))
            .py(px(8.))
            .border_b_1()
            .border_color(palette.line2)
            .child(
                div()
                    .debug_selector(|| "plugins.search".into())
                    .w(px(SEARCH_WIDTH))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .px(px(7.))
                    .rounded(px(6.))
                    .bg(palette.surf)
                    .border_1()
                    .border_color(palette.line)
                    .child(AppIcon::Search.icon().xsmall().text_color(palette.faint))
                    .child(
                        div().flex_1().min_w(px(0.)).child(
                            Input::new(&self.search)
                                .appearance(false)
                                .bordered(false)
                                .text_size(px(12.)),
                        ),
                    ),
            )
            .child(self.tab_button(
                "plugins.tab.installed",
                format!("Installed ({})", self.model.installed_count()),
                Tab::Installed,
                palette,
                cx,
            ))
            .child(self.tab_button(
                "plugins.tab.available",
                "Available".to_string(),
                Tab::Available,
                palette,
                cx,
            ))
            .child(div().flex_1())
            .child(
                button("plugins.install-dev", "Install Dev Extension...", palette)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.install_dev(cx))),
            )
            .into_any_element()
    }

    fn tab_button(
        &self,
        id: &'static str,
        label: String,
        tab: Tab,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let selected = self.model.tab == tab;
        button(id, label, palette)
            .bg(if selected { palette.sel } else { palette.surf })
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.model.set_tab(tab);
                cx.notify();
            }))
    }

    fn list(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .id("plugins.list")
            .debug_selector(|| "plugins.list".into())
            .w(px(LIST_WIDTH))
            .flex_none()
            .h_full()
            .overflow_y_scroll()
            .border_r_1()
            .border_color(palette.line2)
            .bg(palette.panel);
        match self.model.empty_state() {
            Some(text) => {
                list = list.child(
                    div()
                        .debug_selector(|| "plugins.empty".into())
                        .p(px(16.))
                        .text_color(palette.dim)
                        .child(text),
                );
            }
            None => {
                let rows: Vec<AnyElement> = self
                    .model
                    .rows
                    .iter()
                    .enumerate()
                    .map(|(ix, row)| self.row(ix, row, palette, cx))
                    .collect();
                list = list.children(rows);
            }
        }
        list.into_any_element()
    }

    fn row(
        &self,
        ix: usize,
        row: &PluginRowVm,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.model.selected.as_deref() == Some(row.id.as_str());
        let plugin_id = row.id.clone();
        let text = div()
            .flex_1()
            .min_w(px(0.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .min_w(px(0.))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(row.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(11.))
                            .font_family(MONO_FONT)
                            .text_color(palette.dim)
                            .child(row.version.clone()),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.dim)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(row.author.clone()),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(row.description.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.))
                    .children(row.badges.iter().map(|badge| badge_pill(badge, palette))),
            );
        let hover = palette.hover;
        let mut item = div()
            .id(SharedString::from(format!("plugins.row.{ix}")))
            .debug_selector(move || format!("plugins.row.{ix}"))
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(10.))
            .py(px(8.))
            .border_b_1()
            .border_color(palette.line2)
            .bg(if selected { palette.sel } else { palette.panel })
            .hover(move |style| style.bg(hover))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.model.select(Some(&plugin_id));
                cx.notify();
            }))
            .child(text);
        if let Some(slot) = &row.primary {
            item = item.child(self.action_button(
                format!("plugins.row.{ix}.primary"),
                slot,
                &row.id,
                palette,
                cx,
            ));
        }
        item.into_any_element()
    }

    fn action_button(
        &self,
        id: String,
        slot: &Slot,
        plugin_id: &str,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let action = slot.action;
        let plugin = plugin_id.to_string();
        let mut button = div()
            .id(SharedString::from(id.clone()))
            .debug_selector(move || id)
            .flex_none()
            .px(px(10.))
            .py(px(3.))
            .rounded(px(6.))
            .border_1()
            .border_color(palette.line)
            .bg(palette.surf)
            .text_size(px(12.))
            .text_color(if slot.blocked.is_some() {
                palette.dim
            } else {
                palette.text
            })
            .child(action_label(action));
        match slot.blocked {
            Some(reason) => {
                button =
                    button.tooltip(move |_, cx| AnyView::from(cx.new(|_| Tooltip::new(reason))));
            }
            None => {
                button = button.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.act(&plugin, action, cx);
                }));
            }
        }
        button.into_any_element()
    }

    fn detail(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(detail) = &self.model.detail else {
            return div()
                .flex_1()
                .min_w(px(0.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(palette.dim)
                .debug_selector(|| "plugins.detail.empty".into())
                .child("Select a plugin to see its details.")
                .into_any_element();
        };
        let remove = self
            .model
            .rows
            .iter()
            .find(|row| row.id == detail.id)
            .and_then(|row| row.secondary.clone());
        let mut panel = div()
            .id("plugins.detail")
            .debug_selector(|| "plugins.detail".into())
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(16.))
            .overflow_y_scroll()
            .child(
                div()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_size(px(16.))
                    .child(detail.name.clone()),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .text_color(palette.dim)
                    .child(div().font_family(MONO_FONT).child(detail.version.clone()))
                    .child(detail.author.clone()),
            );
        if let Some(url) = detail.repository.clone() {
            panel = panel.child(
                div()
                    .id("plugins.detail.link")
                    .debug_selector(|| "plugins.detail.link".into())
                    .text_color(palette.accent)
                    .cursor_pointer()
                    .child(url.clone())
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&url))),
            );
        }
        panel = panel
            .child(
                div()
                    .text_color(palette.dim)
                    .child(format!("Platforms: {}", detail.platforms)),
            )
            .child(div().child(detail.description.clone()));
        if !detail.capabilities.is_empty() {
            panel = panel
                .child(section_title("Capabilities", palette))
                .children(
                    detail
                        .capabilities
                        .iter()
                        .map(|view| capability_row(view, palette)),
                );
        }
        panel = panel.children(
            detail
                .status_lines
                .iter()
                .map(|line| div().text_color(palette.dim).child(line.clone())),
        );
        if let Some(path) = &detail.install_path {
            panel = panel.child(
                div()
                    .font_family(MONO_FONT)
                    .text_size(px(11.))
                    .text_color(palette.dim)
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(path.display().to_string()),
            );
        }
        panel = panel.child(div().flex_1());
        if let Some(slot) = remove {
            panel = panel.child(self.action_button(
                "plugins.detail.remove".to_string(),
                &slot,
                &detail.id,
                palette,
                cx,
            ));
        }
        panel.into_any_element()
    }

    fn status(&self, palette: &Palette) -> AnyElement {
        let (text, color) = match &self.notice {
            Some(notice) if notice.error => (notice.text.clone(), palette.err),
            Some(notice) => (notice.text.clone(), palette.dim),
            None => (String::new(), palette.dim),
        };
        div()
            .debug_selector(|| "plugins.status".into())
            .flex_none()
            .h(px(STATUS_HEIGHT))
            .flex()
            .items_center()
            .px(px(12.))
            .border_t_1()
            .border_color(palette.line2)
            .text_size(px(11.))
            .text_color(color)
            .overflow_hidden()
            .text_ellipsis()
            .child(text)
            .into_any_element()
    }
}

impl Render for PluginsWindowView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette(cx);
        div()
            .debug_selector(|| "plugins.root".into())
            .key_context("PluginsWindow")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, window, _| {
                if event.keystroke.key == "escape" {
                    window.remove_window();
                }
            }))
            .size_full()
            .flex()
            .flex_col()
            .pt(px(TITLEBAR_INSET))
            .bg(palette.win)
            .text_color(palette.text)
            .text_size(px(12.))
            .children(self.restart_banner(&palette, cx))
            .child(self.header(&palette, cx))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .child(self.list(&palette, cx))
                    .child(self.detail(&palette, cx)),
            )
            .child(self.status(&palette))
    }
}

fn button(id: &'static str, label: impl Into<SharedString>, palette: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_none()
        .px(px(10.))
        .py(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(palette.line)
        .bg(palette.surf)
        .text_size(px(12.))
        .text_color(palette.text)
        .child(label.into())
}

fn section_title(label: &'static str, palette: &Palette) -> AnyElement {
    div()
        .text_size(px(11.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(palette.dim)
        .child(label)
        .into_any_element()
}

fn capability_row(view: &CapabilityView, palette: &Palette) -> AnyElement {
    let (state, color) = if view.granted {
        ("Granted", palette.runtext)
    } else {
        ("Not granted", palette.dim)
    };
    div()
        .flex()
        .justify_between()
        .gap(px(8.))
        .child(capability_label(&view.capability))
        .child(div().flex_none().text_color(color).child(state))
        .into_any_element()
}

fn capability_label(capability: &Capability) -> String {
    match capability {
        Capability::NetworkHttp { .. } => "Network access (HTTP)".to_string(),
        Capability::ProcessExec { command, .. } => format!("Run {command}"),
        Capability::DownloadFile { .. } => "Download files".to_string(),
        Capability::Credentials => "Credentials".to_string(),
        Capability::Secrets => "Secrets".to_string(),
    }
}

fn action_label(action: Action) -> &'static str {
    match action {
        Action::Enable => "Enable",
        Action::Disable => "Disable",
        Action::Update => "Update",
        Action::Install => "Install",
        Action::Remove => "Remove",
    }
}

fn badge_pill(badge: &Badge, palette: &Palette) -> AnyElement {
    let (label, color) = match badge {
        Badge::Bundled => ("Bundled".to_string(), palette.dim),
        Badge::Installed => ("Installed".to_string(), palette.dim),
        Badge::Dev => ("Dev".to_string(), palette.dim),
        Badge::Disabled => ("Disabled".to_string(), palette.warntext),
        Badge::Refused(_) => ("Refused".to_string(), palette.err),
        Badge::UpdateAvailable(version) => (format!("Update {version}"), palette.accent),
    };
    div()
        .px(px(5.))
        .py(px(1.))
        .rounded(px(4.))
        .border_1()
        .border_color(palette.line2)
        .bg(palette.tagbg)
        .text_size(px(10.))
        .text_color(color)
        .child(label)
        .into_any_element()
}

/// Plugin and manager doubles shared by the window tests and the layout-check states.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::path::{Path, PathBuf};

    use clouddeck_core::about::PluginOrigin;
    use clouddeck_core::extension_manager::index::IndexError;
    use clouddeck_core::extension_manager::install::{Manager, Outcome};
    use clouddeck_core::extension_manager::plugins::{PluginActions, PluginEntry};
    use clouddeck_core::extension_manager::source::ExtensionSource;
    use clouddeck_core::release::source::FetchedRelease;

    use super::Roots;

    pub(crate) struct NoSource;

    impl ExtensionSource for NoSource {
        fn index(&self) -> Result<(Vec<u8>, String), IndexError> {
            unreachable!("the Plugins window never fetches the index")
        }

        fn release(&self, _: &str, _: &str) -> Result<FetchedRelease, String> {
            unreachable!("the Plugins window never fetches releases")
        }

        fn download(&self, _: &str, _: &str, _: &str, _: &str, _: u64) -> Result<Vec<u8>, String> {
            unreachable!("the Plugins window never downloads")
        }
    }

    /// Accepts enable and disable; every other action is unreachable from the layout states.
    pub(crate) struct ToggleOnlyActions;

    impl PluginActions for ToggleOnlyActions {
        fn enable(&self, _: &str) -> Result<(), String> {
            Ok(())
        }

        fn disable(&self, _: &str) -> Result<(), String> {
            Ok(())
        }

        fn remove(&self, _: &str) -> Result<(), String> {
            unreachable!("the layout states never remove plugins")
        }

        fn install(&self, _: &str) -> Result<Outcome, String> {
            unreachable!("the layout states never install plugins")
        }

        fn update(&self, _: &str) -> Result<Outcome, String> {
            unreachable!("the layout states never update plugins")
        }

        fn install_dev(&self, _: &Path) -> Result<Outcome, String> {
            unreachable!("the layout states never install dev plugins")
        }
    }

    /// A manager whose roots do not exist; safe to build but never acted on.
    pub(crate) fn inert_manager() -> Manager {
        Manager::new(
            inert_path("installed"),
            inert_path("state"),
            Box::new(NoSource),
            String::new(),
        )
    }

    pub(crate) fn inert_roots() -> Roots {
        Roots {
            bundled: None,
            installed: inert_path("installed"),
            state: inert_path("state"),
        }
    }

    fn inert_path(name: &str) -> PathBuf {
        PathBuf::from("/nonexistent-clouddeck-layout").join(name)
    }

    pub(crate) fn plugin(id: &str, name: &str, origin: PluginOrigin) -> PluginEntry {
        PluginEntry {
            id: id.into(),
            name: name.into(),
            version: "0.1.0".into(),
            origin,
            authors: vec!["CloudDeck".into()],
            description: format!("{name} plugin."),
            repository: None,
            platforms: Vec::new(),
            capabilities: Vec::new(),
            granted: Vec::new(),
            enabled: true,
            crash_disabled: false,
            refusal: None,
            install_dir: None,
        }
    }

    pub(crate) fn entries() -> Vec<PluginEntry> {
        vec![
            plugin("demo", "Demo", PluginOrigin::Bundled),
            plugin("toy", "Toy", PluginOrigin::Installed),
        ]
    }

    /// The bundled aws, gcp and demo plugins.
    pub(crate) fn bundled_set() -> Vec<PluginEntry> {
        vec![
            plugin("aws", "AWS", PluginOrigin::Bundled),
            plugin("gcp", "GCP", PluginOrigin::Bundled),
            plugin("demo", "Demo", PluginOrigin::Bundled),
        ]
    }

    /// `bundled_set` plus one installed plugin, which is the only kind with row actions.
    pub(crate) fn with_installed_set() -> Vec<PluginEntry> {
        let mut entries = bundled_set();
        entries.push(plugin("toy", "Toy", PluginOrigin::Installed));
        entries
    }

    /// `bundled_set` plus one installed plugin the registry refused.
    pub(crate) fn refused_set() -> Vec<PluginEntry> {
        let mut entries = bundled_set();
        let mut old = plugin("old", "Old", PluginOrigin::Installed);
        old.refusal = Some("Rebuild Old for CloudDeck (built for API 5)".into());
        entries.push(old);
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fixtures::{NoSource, entries};
    use gpui::{Entity, TestAppContext, VisualTestContext};

    fn open_test_window<'a>(
        cx: &'a mut TestAppContext,
        model: PluginsViewModel,
        roots: &tempfile::TempDir,
    ) -> (Entity<PluginsWindowView>, &'a mut VisualTestContext) {
        let manager = Manager::new(
            roots.path().join("installed"),
            roots.path().join("state"),
            Box::new(NoSource),
            String::new(),
        );
        let roots = Roots {
            bundled: None,
            installed: roots.path().join("installed"),
            state: roots.path().join("state"),
        };
        let handle = cx
            .update(|cx| {
                gpui_component::init(cx);
                open_window_for(cx, model, manager, roots, WINDOW_SIZE)
            })
            .expect("plugins window opens");
        let view = cx
            .update(|cx| {
                handle
                    .update(cx, |root, _, _| {
                        root.view().clone().downcast::<PluginsWindowView>().ok()
                    })
                    .ok()
                    .flatten()
            })
            .expect("plugins view is reachable");
        let window_cx = VisualTestContext::from_window(handle.into(), cx).into_mut();
        window_cx.run_until_parked();
        (view, window_cx)
    }

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should be rendered"));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    }

    #[gpui::test]
    fn shows_search_tabs_list_and_empty_detail(cx: &mut TestAppContext) {
        let roots = tempfile::tempdir().unwrap();
        let (_, window) = open_test_window(cx, model_for(entries()), &roots);

        for selector in [
            "plugins.search",
            "plugins.tab.installed",
            "plugins.tab.available",
            "plugins.install-dev",
            "plugins.list",
            "plugins.row.0",
            "plugins.row.1",
            "plugins.detail.empty",
            "plugins.status",
        ] {
            assert!(window.debug_bounds(selector).is_some(), "{selector}");
        }
        assert!(window.debug_bounds("plugins.restart").is_none());
    }

    #[gpui::test]
    fn selecting_a_row_shows_its_detail(cx: &mut TestAppContext) {
        let roots = tempfile::tempdir().unwrap();
        let (view, window) = open_test_window(cx, model_for(entries()), &roots);

        click(window, "plugins.row.1");

        assert!(window.debug_bounds("plugins.detail").is_some());
        assert_eq!(
            view.read_with(window, |view, _| view.model.selected.clone()),
            Some("toy".to_string())
        );
    }

    #[gpui::test]
    fn available_tab_shows_the_empty_index_message(cx: &mut TestAppContext) {
        let roots = tempfile::tempdir().unwrap();
        let (view, window) = open_test_window(cx, model_for(entries()), &roots);

        click(window, "plugins.tab.available");

        assert_eq!(
            view.read_with(window, |view, _| view.model.tab),
            Tab::Available
        );
        assert!(window.debug_bounds("plugins.empty").is_some());
        assert!(view.read_with(window, |view, _| view.model.rows.is_empty()));
    }

    #[gpui::test]
    fn remove_failure_is_shown_in_the_status_line(cx: &mut TestAppContext) {
        let roots = tempfile::tempdir().unwrap();
        let (view, window) = open_test_window(cx, model_for(entries()), &roots);
        click(window, "plugins.row.1");

        click(window, "plugins.detail.remove");

        let notice = view.read_with(window, |view, _| view.notice.clone());
        assert!(notice.is_some_and(|notice| notice.error));
    }
}
