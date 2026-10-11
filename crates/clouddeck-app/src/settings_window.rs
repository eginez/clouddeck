//! Settings window (design/images/50-59).

use std::time::Duration;

use clouddeck_core::{
    DefaultSort, Density, FLEET_REGIONS, LaunchBehavior, LogLevel, RECENT_SEARCH_LIMITS,
    REFRESH_INTERVALS, REQUEST_TIMEOUTS, SETTINGS_CATEGORIES, SettingId, Settings, Theme,
    settings_path,
};
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Entity, EventEmitter, Global, IntoElement,
    ParentElement, Render, SharedString, Styled, Subscription, TitlebarOptions, Window,
    WindowBounds, WindowHandle, WindowOptions, div, point, prelude::*, px, size, white,
};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputState};
use gpui_component::select::{Select, SelectEvent, SelectItem, SelectState};
use gpui_component::{IndexPath, Root, Sizable};

use crate::icons::AppIcon;
use crate::tokens::{Palette, palette};

const SAVE_DEBOUNCE: Duration = Duration::from_millis(300);
const CATEGORY_ICONS: [AppIcon; 8] = [
    AppIcon::PanelLeft,
    AppIcon::Server,
    AppIcon::Filter,
    AppIcon::Search,
    AppIcon::Key,
    AppIcon::Cloud,
    AppIcon::Ellipsis,
    AppIcon::CircleAlert,
];

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsEvent {
    Changed(Settings),
}

pub struct SettingsWindow {
    pub settings: Settings,
    pub category: &'static str,
    search: Entity<InputState>,
    selects: Vec<(SettingId, Entity<SelectState<Vec<Choice>>>)>,
    _subscriptions: Vec<Subscription>,
    save_generation: u64,
    /// Settings drawn by the last render, since `debug_bounds` keeps removed elements.
    #[cfg(test)]
    rendered: Vec<SettingId>,
}

impl EventEmitter<SettingsEvent> for SettingsWindow {}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Pick {
    Theme(Theme),
    Density(Density),
    LaunchBehavior(LaunchBehavior),
    DefaultSort(DefaultSort),
    LogLevel(LogLevel),
    RefreshInterval(u32),
    RecentSearchLimit(u32),
    RequestTimeout(u32),
}

impl Pick {
    fn apply(self, settings: &mut Settings) {
        match self {
            Pick::Theme(value) => settings.theme = value,
            Pick::Density(value) => settings.density = value,
            Pick::LaunchBehavior(value) => settings.on_launch = value,
            Pick::DefaultSort(value) => settings.default_sort = value,
            Pick::LogLevel(value) => settings.log_level = value,
            Pick::RefreshInterval(value) => settings.refresh_interval_minutes = value,
            Pick::RecentSearchLimit(value) => settings.recent_search_limit = value,
            Pick::RequestTimeout(value) => settings.request_timeout_seconds = value,
        }
    }
}

#[derive(Clone)]
struct Choice {
    title: SharedString,
    pick: Pick,
}

impl SelectItem for Choice {
    type Value = Pick;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &Pick {
        &self.pick
    }
}

fn choice(title: impl Into<SharedString>, pick: Pick) -> Choice {
    Choice {
        title: title.into(),
        pick,
    }
}

fn minutes_label(minutes: u32) -> String {
    if minutes == 1 {
        "1 minute".to_owned()
    } else {
        format!("{minutes} minutes")
    }
}

fn choices(id: SettingId) -> Vec<Choice> {
    match id {
        SettingId::Theme => vec![
            choice("Follow system", Pick::Theme(Theme::FollowSystem)),
            choice("Light", Pick::Theme(Theme::Light)),
            choice("Dark", Pick::Theme(Theme::Dark)),
        ],
        SettingId::Density => vec![
            choice("Compact (28pt rows)", Pick::Density(Density::Compact)),
            choice(
                "Comfortable (34pt rows)",
                Pick::Density(Density::Comfortable),
            ),
        ],
        SettingId::OnLaunch => vec![
            choice(
                "Remember last profile and regions",
                Pick::LaunchBehavior(LaunchBehavior::RememberLast),
            ),
            choice(
                "Always ask",
                Pick::LaunchBehavior(LaunchBehavior::AlwaysAsk),
            ),
        ],
        SettingId::DefaultSort => vec![
            choice("Name", Pick::DefaultSort(DefaultSort::Name)),
            choice(
                "Launched (newest first)",
                Pick::DefaultSort(DefaultSort::LaunchedNewest),
            ),
            choice("State", Pick::DefaultSort(DefaultSort::State)),
        ],
        SettingId::LogLevel => vec![
            choice("Errors only", Pick::LogLevel(LogLevel::ErrorsOnly)),
            choice("Info", Pick::LogLevel(LogLevel::Info)),
            choice("Debug", Pick::LogLevel(LogLevel::Debug)),
        ],
        SettingId::RefreshInterval => REFRESH_INTERVALS
            .iter()
            .map(|&minutes| choice(minutes_label(minutes), Pick::RefreshInterval(minutes)))
            .collect(),
        SettingId::RecentSearchLimit => RECENT_SEARCH_LIMITS
            .iter()
            .map(|&limit| choice(limit.to_string(), Pick::RecentSearchLimit(limit)))
            .collect(),
        SettingId::RequestTimeout => REQUEST_TIMEOUTS
            .iter()
            .map(|&seconds| choice(format!("{seconds} seconds"), Pick::RequestTimeout(seconds)))
            .collect(),
        _ => Vec::new(),
    }
}

fn current(id: SettingId, settings: &Settings) -> Option<Pick> {
    Some(match id {
        SettingId::Theme => Pick::Theme(settings.theme),
        SettingId::Density => Pick::Density(settings.density),
        SettingId::OnLaunch => Pick::LaunchBehavior(settings.on_launch),
        SettingId::DefaultSort => Pick::DefaultSort(settings.default_sort),
        SettingId::LogLevel => Pick::LogLevel(settings.log_level),
        SettingId::RefreshInterval => Pick::RefreshInterval(settings.refresh_interval_minutes),
        SettingId::RecentSearchLimit => Pick::RecentSearchLimit(settings.recent_search_limit),
        SettingId::RequestTimeout => Pick::RequestTimeout(settings.request_timeout_seconds),
        _ => return None,
    })
}

fn flag(settings: &Settings, id: SettingId) -> Option<bool> {
    match id {
        SettingId::ShowSidebarOnLaunch => Some(settings.show_sidebar_on_launch),
        SettingId::OpenInspectorOnSelect => Some(settings.open_inspector_on_select),
        SettingId::ShowUpdatedTime => Some(settings.show_updated_time),
        SettingId::AutoRefresh => Some(settings.auto_refresh),
        SettingId::RefreshOnFocus => Some(settings.refresh_on_focus),
        SettingId::ShowUnnamedLabel => Some(settings.show_unnamed_label),
        SettingId::MatchTags => Some(settings.match_tags),
        SettingId::DemoMode => Some(settings.demo_mode),
        _ => None,
    }
}

fn flag_mut(settings: &mut Settings, id: SettingId) -> Option<&mut bool> {
    match id {
        SettingId::ShowSidebarOnLaunch => Some(&mut settings.show_sidebar_on_launch),
        SettingId::OpenInspectorOnSelect => Some(&mut settings.open_inspector_on_select),
        SettingId::ShowUpdatedTime => Some(&mut settings.show_updated_time),
        SettingId::AutoRefresh => Some(&mut settings.auto_refresh),
        SettingId::RefreshOnFocus => Some(&mut settings.refresh_on_focus),
        SettingId::ShowUnnamedLabel => Some(&mut settings.show_unnamed_label),
        SettingId::MatchTags => Some(&mut settings.match_tags),
        SettingId::DemoMode => Some(&mut settings.demo_mode),
        _ => None,
    }
}

fn groups_for(category: &str) -> Vec<(&'static str, Vec<SettingId>)> {
    let mut groups: Vec<(&'static str, Vec<SettingId>)> = Vec::new();
    for id in SettingId::ALL
        .into_iter()
        .filter(|id| id.category() == category)
    {
        match groups.iter_mut().find(|(name, _)| *name == id.group()) {
            Some((_, ids)) => ids.push(id),
            None => groups.push((id.group(), vec![id])),
        }
    }
    groups
}

impl SettingsWindow {
    pub fn new(settings: Settings, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search settings"));
        let mut selects = Vec::new();
        let mut subscriptions = Vec::new();
        for id in SettingId::ALL {
            let items = choices(id);
            if items.is_empty() {
                continue;
            }
            let selected = current(id, &settings)
                .and_then(|pick| items.iter().position(|choice| choice.pick == pick))
                .map(IndexPath::new);
            let state = cx.new(|cx| SelectState::new(items, selected, window, cx));
            subscriptions.push(cx.subscribe_in(
                &state,
                window,
                move |this: &mut Self, _, event: &SelectEvent<Vec<Choice>>, _, cx| {
                    if let SelectEvent::Confirm(Some(pick)) = event {
                        this.choose(id, *pick, cx);
                    }
                },
            ));
            selects.push((id, state));
        }
        Self {
            settings,
            category: SETTINGS_CATEGORIES[0],
            search,
            selects,
            _subscriptions: subscriptions,
            save_generation: 0,
            #[cfg(test)]
            rendered: Vec::new(),
        }
    }

    fn change(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Settings)) {
        let mut next = self.settings.clone();
        edit(&mut next);
        let next = next.normalized();
        if next == self.settings {
            return;
        }
        self.settings = next;
        self.schedule_save(cx);
        cx.emit(SettingsEvent::Changed(self.settings.clone()));
        cx.notify();
    }

    fn toggle(&mut self, id: SettingId, cx: &mut Context<Self>) {
        self.change(cx, |settings| {
            if let Some(value) = flag_mut(settings, id) {
                *value = !*value;
            }
        });
    }

    fn choose(&mut self, id: SettingId, pick: Pick, cx: &mut Context<Self>) {
        if id.is_disabled(&self.settings) {
            return;
        }
        self.change(cx, |settings| pick.apply(settings));
    }

    fn toggle_region(&mut self, region: &str, cx: &mut Context<Self>) {
        self.change(cx, |settings| {
            let visible = settings
                .regions
                .iter()
                .filter(|name| FLEET_REGIONS.contains(&name.as_str()))
                .count();
            match settings.regions.iter().position(|name| name == region) {
                Some(index) if visible > 1 => {
                    settings.regions.remove(index);
                }
                Some(_) => {}
                None => settings.regions.push(region.to_owned()),
            }
        });
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        if cfg!(test) {
            return;
        }
        self.save_generation += 1;
        let generation = self.save_generation;
        let settings = self.settings.clone();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DEBOUNCE).await;
            let superseded = this
                .update(cx, |this, _| this.save_generation != generation)
                .unwrap_or(false);
            if !superseded {
                cx.background_executor()
                    .spawn(async move {
                        if let Err(error) = settings.save(&settings_path()) {
                            eprintln!("Could not save settings: {error}");
                        }
                    })
                    .detach();
            }
        })
        .detach();
    }

    fn category_button(
        &self,
        index: usize,
        name: &'static str,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = name == self.category;
        div()
            .id(("settings-cat", index))
            .debug_selector(move || format!("settings.cat.{index}"))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.category = SETTINGS_CATEGORIES[index];
                cx.notify();
            }))
            .flex()
            .items_center()
            .gap(px(8.))
            .h(px(26.))
            .px(px(8.))
            .rounded(px(6.))
            .text_size(px(12.))
            .when(selected, |row| row.bg(palette.sel).text_color(palette.text))
            .when(!selected, |row| row.text_color(palette.dim))
            .child(CATEGORY_ICONS[index].icon().xsmall())
            .child(name)
            .into_any_element()
    }

    fn switch(
        &self,
        id: SettingId,
        on: bool,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let track = div()
            .w(px(32.))
            .h(px(18.))
            .rounded(px(9.))
            .p(px(2.))
            .flex()
            .items_center()
            .bg(if on { palette.accent } else { palette.line })
            .when(on, |track| track.justify_end())
            .child(div().size(px(14.)).rounded(px(7.)).bg(white()));
        div()
            .id(id.label())
            .debug_selector(move || format!("settings.control.{id:?}"))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle(id, cx)))
            .child(track)
            .into_any_element()
    }

    fn select_control(&self, id: SettingId, palette: &Palette) -> AnyElement {
        let Some((_, state)) = self.selects.iter().find(|(select_id, _)| *select_id == id) else {
            return div().into_any_element();
        };
        let disabled = id.is_disabled(&self.settings);
        div()
            .debug_selector(move || format!("settings.control.{id:?}"))
            .w(px(190.))
            .text_color(palette.text)
            .child(Select::new(state).menu_width(px(220.)).disabled(disabled))
            .into_any_element()
    }

    fn regions_control(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        div()
            .debug_selector(|| "settings.control.Regions".into())
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(FLEET_REGIONS.iter().map(|region| {
                let name = region.to_string();
                let view = view.clone();
                let checked = self
                    .settings
                    .regions
                    .iter()
                    .any(|selected| selected.as_str() == *region);
                Checkbox::new(*region)
                    .label(*region)
                    .checked(checked)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| this.toggle_region(&name, cx));
                    })
            }))
            .text_color(palette.text)
            .into_any_element()
    }

    fn control(&self, id: SettingId, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        match id {
            SettingId::Regions => self.regions_control(palette, cx),
            SettingId::AwsCliPath | SettingId::AwsConfigFile => div()
                .debug_selector(move || format!("settings.control.{id:?}"))
                .text_size(px(11.))
                .text_color(palette.faint)
                .child("Set in a later update")
                .into_any_element(),
            _ => match flag(&self.settings, id) {
                Some(on) => self.switch(id, on, palette, cx),
                None => self.select_control(id, palette),
            },
        }
    }

    fn setting_row(&self, id: SettingId, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let disabled = id.is_disabled(&self.settings);
        let changed = id.is_changed(&self.settings);
        div()
            .debug_selector(move || format!("settings.item.{id:?}"))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.))
            .py(px(9.))
            .border_b_1()
            .border_color(palette.line)
            .when(disabled, |row| row.opacity(0.5))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w(px(0.))
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(
                                div()
                                    .w(px(3.))
                                    .h(px(14.))
                                    .rounded(px(2.))
                                    .when(changed, |bar| bar.bg(palette.accent)),
                            )
                            .child(div().text_size(px(13.)).child(id.label())),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(palette.dim)
                            .child(id.description()),
                    ),
            )
            .child(self.control(id, palette, cx))
            .into_any_element()
    }

    fn group_section(
        &self,
        group: &'static str,
        ids: &[SettingId],
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .mb(px(18.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.faint)
                    .mb(px(4.))
                    .child(group.to_uppercase()),
            )
            .children(ids.iter().map(|id| self.setting_row(*id, palette, cx)))
            .into_any_element()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette(cx);
        let tree = div()
            .debug_selector(|| "settings.tree".into())
            .w(px(200.))
            .h_full()
            .flex()
            .flex_col()
            .gap(px(2.))
            .pt(px(36.))
            .px(px(10.))
            .pb(px(10.))
            .bg(palette.side)
            .border_r_1()
            .border_color(palette.line)
            .child(
                div()
                    .debug_selector(|| "settings.search".into())
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(26.))
                    .px(px(8.))
                    .mb(px(8.))
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
            .children(
                SETTINGS_CATEGORIES
                    .iter()
                    .enumerate()
                    .map(|(index, name)| self.category_button(index, name, &palette, cx)),
            );

        let groups = groups_for(self.category);
        #[cfg(test)]
        {
            self.rendered = groups
                .iter()
                .flat_map(|(_, ids)| ids.iter().copied())
                .collect();
        }
        let pane = div()
            .debug_selector(|| "settings.pane".into())
            .id("settings-pane")
            .flex_1()
            .overflow_y_scroll()
            .p(px(20.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.dim)
                    .mb(px(4.))
                    .child(format!("Settings › {}", self.category)),
            )
            .child(
                div()
                    .text_size(px(18.))
                    .text_color(palette.text)
                    .mb(px(14.))
                    .child(self.category),
            )
            .children(
                groups
                    .into_iter()
                    .map(|(group, ids)| self.group_section(group, &ids, &palette, cx)),
            );

        let footer = div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(44.))
            .px(px(20.))
            .border_t_1()
            .border_color(palette.line)
            .child(div().text_size(px(11.)).text_color(palette.dim).child(
                "Changes apply immediately. A blue bar marks values that differ from the default.",
            ))
            .child(
                div()
                    .id("settings-done")
                    .debug_selector(|| "settings.done".into())
                    .px(px(12.))
                    .py(px(4.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(palette.line)
                    .bg(palette.surf)
                    .text_size(px(12.))
                    .text_color(palette.text)
                    .on_click(cx.listener(|_, _: &ClickEvent, window, _| window.remove_window()))
                    .child("Done"),
            );

        let content = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .child(pane)
            .child(footer);

        div()
            .size_full()
            .flex()
            .bg(palette.win)
            .text_color(palette.text)
            .text_size(px(13.))
            .child(tree)
            .child(content)
    }
}

struct OpenSettingsWindow(WindowHandle<Root>);

impl Global for OpenSettingsWindow {}

pub fn open_settings_window(cx: &mut App, settings: Settings) -> Option<WindowHandle<Root>> {
    if let Some(handle) = cx.try_global::<OpenSettingsWindow>().map(|open| open.0)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Some(handle);
    }
    let bounds = Bounds::centered(None, size(px(820.0), px(560.0)), cx);
    let handle = cx
        .open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Settings")),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(15.0))),
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| SettingsWindow::new(settings, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .ok()?;
    cx.set_global(OpenSettingsWindow(handle));
    Some(handle)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use gpui::{Entity, TestAppContext, VisualTestContext};

    use super::*;

    fn rooted(
        cx: &mut TestAppContext,
        settings: Settings,
    ) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let view = cx.new(|cx| SettingsWindow::new(settings, window, cx));
            *slot_for_window.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let view = slot
            .borrow()
            .as_ref()
            .expect("settings window should be created")
            .clone();
        (view, cx)
    }

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should be rendered"));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    }

    fn recorded_events(
        view: &Entity<SettingsWindow>,
        cx: &mut VisualTestContext,
    ) -> Rc<RefCell<Vec<SettingsEvent>>> {
        let events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(view, move |_, event: &SettingsEvent, _| {
                sink.borrow_mut().push(event.clone());
            })
            .detach();
        });
        events
    }

    #[gpui::test]
    fn renders_the_tree_and_the_appearance_pane(cx: &mut TestAppContext) {
        let (_view, cx) = rooted(cx, Settings::default());

        assert!(cx.debug_bounds("settings.tree").is_some());
        assert!(cx.debug_bounds("settings.search").is_some());
        for selector in [
            "settings.cat.0",
            "settings.cat.1",
            "settings.cat.2",
            "settings.cat.3",
            "settings.cat.4",
            "settings.cat.5",
            "settings.cat.6",
            "settings.cat.7",
        ] {
            assert!(cx.debug_bounds(selector).is_some(), "{selector}");
        }
        assert!(cx.debug_bounds("settings.pane").is_some());
        assert!(cx.debug_bounds("settings.item.Theme").is_some());
    }

    #[gpui::test]
    fn choosing_a_category_shows_its_settings(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());

        click(cx, "settings.cat.1");

        assert_eq!(view.read_with(cx, |view, _| view.category), "Fleet");
        assert!(cx.debug_bounds("settings.item.AutoRefresh").is_some());
        assert!(view.read_with(cx, |view, _| {
            view.rendered.contains(&SettingId::AutoRefresh)
        }));
        assert!(!view.read_with(cx, |view, _| view.rendered.contains(&SettingId::Theme)));
    }

    #[gpui::test]
    fn switch_toggles_the_setting_and_emits_changed(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());
        let events = recorded_events(&view, cx);

        click(cx, "settings.cat.1");
        click(cx, "settings.control.AutoRefresh");

        assert!(!view.read_with(cx, |view, _| view.settings.auto_refresh));
        assert!(matches!(
            events.borrow().as_slice(),
            [SettingsEvent::Changed(settings)] if !settings.auto_refresh
        ));
    }

    #[gpui::test]
    fn refresh_interval_is_inert_while_auto_refresh_is_off(cx: &mut TestAppContext) {
        let settings = Settings {
            auto_refresh: false,
            ..Settings::default()
        };
        let (view, cx) = rooted(cx, settings);
        let events = recorded_events(&view, cx);

        click(cx, "settings.cat.1");
        click(cx, "settings.control.RefreshInterval");
        view.update(cx, |view, cx| {
            view.choose(SettingId::RefreshInterval, Pick::RefreshInterval(10), cx)
        });

        assert_eq!(
            view.read_with(cx, |view, _| view.settings.refresh_interval_minutes),
            5
        );
        assert!(events.borrow().is_empty());
    }

    #[gpui::test]
    fn the_last_region_cannot_be_removed(cx: &mut TestAppContext) {
        let settings = Settings {
            regions: vec!["us-east-1".into()],
            ..Settings::default()
        };
        let (view, cx) = rooted(cx, settings);
        let events = recorded_events(&view, cx);

        view.update(cx, |view, cx| view.toggle_region("us-east-1", cx));

        assert_eq!(
            view.read_with(cx, |view, _| view.settings.regions.clone()),
            vec!["us-east-1".to_string()]
        );
        assert!(events.borrow().is_empty());
    }

    #[gpui::test]
    fn an_unchecked_region_is_added(cx: &mut TestAppContext) {
        let settings = Settings {
            regions: vec!["us-east-1".into()],
            ..Settings::default()
        };
        let (view, cx) = rooted(cx, settings);

        view.update(cx, |view, cx| view.toggle_region("eu-west-1", cx));

        assert_eq!(
            view.read_with(cx, |view, _| view.settings.regions.clone()),
            vec!["us-east-1".to_string(), "eu-west-1".to_string()]
        );
    }

    #[gpui::test]
    fn choosing_a_theme_updates_settings_and_emits_changed(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());
        let events = recorded_events(&view, cx);

        view.update(cx, |view, cx| {
            view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx)
        });

        assert_eq!(
            view.read_with(cx, |view, _| view.settings.theme),
            Theme::Dark
        );
        assert!(matches!(
            events.borrow().as_slice(),
            [SettingsEvent::Changed(settings)] if settings.theme == Theme::Dark
        ));
    }

    #[gpui::test]
    fn opening_again_reuses_the_open_window(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);

        assert!(
            cx.update(|cx| open_settings_window(cx, Settings::default()))
                .is_some()
        );
        assert!(
            cx.update(|cx| open_settings_window(cx, Settings::default()))
                .is_some()
        );

        assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    }
}
