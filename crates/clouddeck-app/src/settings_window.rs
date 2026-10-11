//! Settings window (design/images/50-59).

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clouddeck_core::{
    DefaultSort, Density, FLEET_REGIONS, KEYMAP, LaunchBehavior, LogLevel, Platform,
    RECENT_SEARCH_LIMITS, REFRESH_INTERVALS, REQUEST_TIMEOUTS, SETTINGS_CATEGORIES, SettingId,
    Settings, SettingsSearchResults, Theme, UiState, changed_categories, context_path,
    resolve_aws_cli_path, search_settings, settings_no_match_text, settings_path,
    settings_search_breadcrumb, shortcut_label, test_aws_cli, ui_state_path,
};
use gpui::{
    AnyElement, App, Bounds, ClickEvent, Context, Div, Entity, EventEmitter, Global,
    HighlightStyle, IntoElement, ParentElement, Render, SharedString, Stateful, Styled, StyledText,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, div, point,
    prelude::*, px, size, white,
};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::select::{Select, SelectEvent, SelectItem, SelectState};
use gpui_component::{IndexPath, Root, Sizable};

use crate::icons::AppIcon;
use crate::tokens::{Palette, palette};

const SAVE_DEBOUNCE: Duration = Duration::from_millis(300);
const NOTE_DURATION: Duration = Duration::from_secs(2);
const CLI_PATHS: [&str; 2] = ["/opt/homebrew/bin/aws", "/usr/local/bin/aws"];
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
    RecentSearchesCleared,
    RememberedScopeForgotten,
}

pub struct SettingsWindow {
    pub settings: Settings,
    pub category: &'static str,
    search: Entity<InputState>,
    config_input: Entity<InputState>,
    selects: Vec<(SettingId, Entity<SelectState<Vec<Choice>>>)>,
    _subscriptions: Vec<Subscription>,
    path: PathBuf,
    unsaved: bool,
    save_generation: u64,
    note: Option<Note>,
    note_generation: u64,
    cli_version: Option<String>,
    cli_test_generation: u64,
    /// Settings drawn by the last render, since `debug_bounds` keeps removed elements.
    #[cfg(test)]
    rendered: Vec<SettingId>,
    #[cfg(test)]
    rendered_changes: Vec<SettingId>,
    #[cfg(test)]
    rendered_dots: Vec<usize>,
    #[cfg(test)]
    rendered_search: bool,
    #[cfg(test)]
    rendered_categories: Vec<usize>,
}

impl EventEmitter<SettingsEvent> for SettingsWindow {}

#[derive(Debug, Clone, PartialEq)]
enum Pick {
    Theme(Theme),
    Density(Density),
    LaunchBehavior(LaunchBehavior),
    DefaultSort(DefaultSort),
    LogLevel(LogLevel),
    RefreshInterval(u32),
    RecentSearchLimit(u32),
    RequestTimeout(u32),
    AwsCliPath(Option<String>),
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
            Pick::AwsCliPath(value) => settings.aws_cli_path = value,
        }
    }
}

#[derive(Debug, Clone)]
struct Note {
    text: SharedString,
    icon: Option<AppIcon>,
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

fn button(id: &'static str, label: &'static str, palette: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(12.))
        .py(px(4.))
        .rounded(px(6.))
        .border_1()
        .border_color(palette.line)
        .bg(palette.surf)
        .text_size(px(12.))
        .text_color(palette.text)
        .child(label)
}

fn minutes_label(minutes: u32) -> String {
    if minutes == 1 {
        "1 minute".to_owned()
    } else {
        format!("{minutes} minutes")
    }
}

fn choices(id: SettingId, settings: &Settings) -> Vec<Choice> {
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
        SettingId::AwsCliPath => {
            let custom = settings
                .aws_cli_path
                .as_deref()
                .filter(|path| !CLI_PATHS.contains(path));
            std::iter::once(choice("Auto-detect", Pick::AwsCliPath(None)))
                .chain(
                    CLI_PATHS.iter().copied().chain(custom).map(|path| {
                        choice(path.to_owned(), Pick::AwsCliPath(Some(path.to_owned())))
                    }),
                )
                .collect()
        }
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
        SettingId::AwsCliPath => Pick::AwsCliPath(settings.aws_cli_path.clone()),
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

fn fold_case(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn match_ranges(text: &str, query: &str) -> Vec<Range<usize>> {
    let needle: Vec<char> = query.trim().chars().map(fold_case).collect();
    let mut ranges = Vec::new();
    if needle.is_empty() {
        return ranges;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut start = 0;
    while start + needle.len() <= chars.len() {
        let hit = chars[start..start + needle.len()]
            .iter()
            .zip(&needle)
            .all(|((_, c), n)| fold_case(*c) == *n);
        if hit {
            let begin = chars[start].0;
            let end = chars
                .get(start + needle.len())
                .map_or(text.len(), |(index, _)| *index);
            ranges.push(begin..end);
            start += needle.len();
        } else {
            start += 1;
        }
    }
    ranges
}

fn highlighted(text: &str, query: &str, palette: &Palette) -> StyledText {
    StyledText::new(text.to_owned()).with_highlights(match_ranges(text, query).into_iter().map(
        |range| {
            (
                range,
                HighlightStyle {
                    background_color: Some(palette.sel),
                    ..Default::default()
                },
            )
        },
    ))
}

fn host_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Mac
    } else {
        Platform::Linux
    }
}

fn keymap_row(
    index: usize,
    action: &str,
    shortcut: &str,
    query: &str,
    palette: &Palette,
) -> AnyElement {
    div()
        .debug_selector(move || format!("settings.keymap.{index}"))
        .flex()
        .items_center()
        .justify_between()
        .py(px(8.))
        .border_b_1()
        .border_color(palette.line)
        .child(
            div()
                .text_size(px(13.))
                .child(highlighted(action, query, palette)),
        )
        .child(
            div()
                .px(px(6.))
                .py(px(2.))
                .rounded(px(4.))
                .border_1()
                .border_color(palette.line)
                .bg(palette.surf)
                .text_size(px(11.))
                .text_color(palette.dim)
                .child(shortcut_label(shortcut, host_platform())),
        )
        .into_any_element()
}

fn keymap_results(
    query: &str,
    rows: &[(&'static str, &'static str)],
    palette: &Palette,
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
                .child("KEYMAP"),
        )
        .child(
            div()
                .flex()
                .justify_between()
                .py(px(4.))
                .text_size(px(11.))
                .text_color(palette.faint)
                .child("Action")
                .child("Shortcut"),
        )
        .children(
            rows.iter().enumerate().map(|(index, (action, shortcut))| {
                keymap_row(index, action, shortcut, query, palette)
            }),
        )
        .into_any_element()
}

impl SettingsWindow {
    pub fn new(settings: Settings, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search settings")
                .clean_on_escape()
        });
        let config_input = cx
            .new(|cx| InputState::new(window, cx).default_value(settings.aws_config_file.clone()));
        let mut selects = Vec::new();
        let mut subscriptions = vec![cx.observe(&search, |_, _, cx| cx.notify())];
        subscriptions.push(cx.on_app_quit(|this: &mut Self, _| {
            this.flush_pending_save();
            std::future::ready(())
        }));
        subscriptions.push(cx.subscribe_in(
            &config_input,
            window,
            |this: &mut Self, state, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    let value = state.read(cx).value().to_string();
                    this.change(cx, |settings| settings.aws_config_file = value);
                }
            },
        ));
        for id in SettingId::ALL {
            let items = choices(id, &settings);
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
                        this.choose(id, pick.clone(), cx);
                    }
                },
            ));
            selects.push((id, state));
        }
        Self {
            settings,
            category: SETTINGS_CATEGORIES[0],
            search,
            config_input,
            selects,
            _subscriptions: subscriptions,
            path: settings_path(),
            unsaved: false,
            save_generation: 0,
            note: None,
            note_generation: 0,
            cli_version: None,
            cli_test_generation: 0,
            #[cfg(test)]
            rendered: Vec::new(),
            #[cfg(test)]
            rendered_changes: Vec::new(),
            #[cfg(test)]
            rendered_dots: Vec::new(),
            #[cfg(test)]
            rendered_search: false,
            #[cfg(test)]
            rendered_categories: Vec::new(),
        }
    }

    fn query(&self, cx: &App) -> String {
        self.search.read(cx).value().to_string()
    }

    fn open_category(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.category = SETTINGS_CATEGORIES[index];
        self.search
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    fn change(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Settings)) {
        let mut next = self.settings.clone();
        edit(&mut next);
        let next = next.normalized();
        if next == self.settings {
            return;
        }
        self.settings = next;
        self.unsaved = true;
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

    fn reset(&mut self, id: SettingId, window: &mut Window, cx: &mut Context<Self>) {
        if id.is_disabled(&self.settings) {
            return;
        }
        self.change(cx, |settings| id.reset(settings));
        self.sync_controls(window, cx);
    }

    fn restore_defaults(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.change(cx, |settings| *settings = Settings::default());
        self.sync_controls(window, cx);
        self.show_note_for("Defaults restored", Some(AppIcon::Check), cx);
    }

    fn sync_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (id, state) in &self.selects {
            if let Some(pick) = current(*id, &self.settings) {
                state.update(cx, |state, cx| state.set_selected_value(&pick, window, cx));
            }
        }
        let value = self.settings.aws_config_file.clone();
        self.config_input.update(cx, |input, cx| {
            if input.value().to_string() != value {
                input.set_value(value, window, cx);
            }
        });
    }

    fn show_note(
        &mut self,
        text: impl Into<SharedString>,
        icon: Option<AppIcon>,
        cx: &mut Context<Self>,
    ) {
        self.note_generation += 1;
        self.note = Some(Note {
            text: text.into(),
            icon,
        });
        cx.notify();
    }

    fn show_note_for(
        &mut self,
        text: impl Into<SharedString>,
        icon: Option<AppIcon>,
        cx: &mut Context<Self>,
    ) {
        self.show_note(text, icon, cx);
        let generation = self.note_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(NOTE_DURATION).await;
            this.update(cx, |this, cx| {
                if this.note_generation == generation {
                    this.note = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn run_cli_test(&mut self, cx: &mut Context<Self>) {
        let path = resolve_aws_cli_path(self.settings.aws_cli_path.as_deref().map(Path::new));
        let timeout = Duration::from_secs(u64::from(self.settings.request_timeout_seconds));
        self.cli_test_generation += 1;
        let generation = self.cli_test_generation;
        self.show_note("Testing…", None, cx);
        cx.spawn(async move |this, cx| {
            let outcome = crate::runtime::runtime()
                .spawn(async move { test_aws_cli(&path, timeout).await })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string()));
            this.update(cx, |this, cx| this.finish_cli_test(generation, outcome, cx))
                .ok();
        })
        .detach();
    }

    fn finish_cli_test(
        &mut self,
        generation: u64,
        outcome: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        if self.cli_test_generation != generation {
            return;
        }
        self.cli_version = outcome.clone().ok();
        let text = outcome.unwrap_or_else(|error| error);
        self.show_note(text, None, cx);
    }

    fn edit_ui_state(
        &mut self,
        note: &'static str,
        edit: impl FnOnce(&mut UiState),
        cx: &mut Context<Self>,
    ) -> bool {
        let path = ui_state_path();
        let mut state = UiState::load(&path);
        edit(&mut state);
        match state.save(&path) {
            Ok(()) => {
                self.show_note(note, None, cx);
                true
            }
            Err(error) => {
                self.show_note(format!("Could not save: {error}"), None, cx);
                false
            }
        }
    }

    fn schedule_save(&mut self, cx: &mut Context<Self>) {
        self.save_generation += 1;
        let generation = self.save_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DEBOUNCE).await;
            this.update(cx, |this, _| {
                if this.save_generation == generation {
                    this.flush_pending_save();
                }
            })
            .ok();
        })
        .detach();
    }

    fn flush_pending_save(&mut self) {
        if !self.unsaved {
            return;
        }
        match self.settings.save(&self.path) {
            Ok(()) => self.unsaved = false,
            Err(error) => eprintln!("Could not save settings: {error}"),
        }
    }

    fn category_button(
        &self,
        index: usize,
        name: &'static str,
        selected: bool,
        changed: bool,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(("settings-cat", index))
            .debug_selector(move || format!("settings.cat.{index}"))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.open_category(index, window, cx)
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
            .when(changed, |row| {
                row.child(
                    div()
                        .ml_auto()
                        .size(px(6.))
                        .rounded(px(3.))
                        .bg(palette.accent)
                        .debug_selector(move || format!("settings.cat-dot.{index}")),
                )
            })
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
            SettingId::AwsCliPath => div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(self.select_control(id, palette))
                .child(
                    button("settings-cli-test", "Test", palette)
                        .debug_selector(|| "settings.cli-test".into())
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.run_cli_test(cx))),
                )
                .into_any_element(),
            SettingId::AwsConfigFile => div()
                .debug_selector(move || format!("settings.control.{id:?}"))
                .w(px(260.))
                .child(Input::new(&self.config_input).small())
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
        let query = self.query(cx);
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
                            .child(div().w(px(2.)).h(px(14.)).rounded(px(1.)).when(
                                changed,
                                |bar| {
                                    bar.bg(palette.accent)
                                        .debug_selector(move || format!("settings.changed.{id:?}"))
                                },
                            ))
                            .child(div().text_size(px(13.)).child(highlighted(
                                id.label(),
                                &query,
                                palette,
                            )))
                            .when(changed, |label| {
                                label.child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "settings-reset-{}",
                                            id.label()
                                        )))
                                        .debug_selector(move || format!("settings.reset.{id:?}"))
                                        .cursor_pointer()
                                        .text_size(px(11.))
                                        .text_color(palette.accent)
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, window, cx| {
                                                this.reset(id, window, cx)
                                            },
                                        ))
                                        .child("Reset"),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(palette.dim)
                            .child(highlighted(id.description(), &query, palette)),
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

    fn stored_data_section(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .mt(px(6.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.faint)
                    .child("STORED DATA"),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.dim)
                    .child("CloudDeck never stores credentials. It keeps profile and region names, pinned and recent searches, and window layout in ~/.config/clouddeck. Instance data is never written to disk."),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.))
                    .child(
                        button("settings-reveal-data", "Reveal data folder", palette).on_click(
                            cx.listener(|_, _: &ClickEvent, _, cx| {
                                let folder = context_path();
                                if let Some(dir) = folder.parent() {
                                    cx.reveal_path(dir);
                                }
                            }),
                        ),
                    )
                    .child(
                        button("settings-clear-searches", "Clear recent searches", palette)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                if this.edit_ui_state(
                                    "Recent searches cleared",
                                    |state| state.recent_searches.clear(),
                                    cx,
                                ) {
                                    cx.emit(SettingsEvent::RecentSearchesCleared);
                                }
                            })),
                    )
                    .child(
                        button("settings-forget-scope", "Forget remembered scope", palette)
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                if this.edit_ui_state(
                                    "Remembered scope forgotten",
                                    |state| state.selected_regions.clear(),
                                    cx,
                                ) {
                                    cx.emit(SettingsEvent::RememberedScopeForgotten);
                                }
                            })),
                    ),
            )
            .into_any_element()
    }

    fn keymap_section(&self, palette: &Palette) -> Vec<AnyElement> {
        let info = div()
            .text_size(px(11.))
            .text_color(palette.dim)
            .mb(px(10.))
            .child("Shortcuts shown here match the tooltips and the command palette.")
            .into_any_element();
        let rows = KEYMAP
            .iter()
            .enumerate()
            .map(|(index, (action, shortcut))| keymap_row(index, action, shortcut, "", palette));
        std::iter::once(info).chain(rows).collect()
    }

    fn search_body(
        &self,
        query: &str,
        results: &SettingsSearchResults,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if results.count() == 0 {
            return vec![
                div()
                    .debug_selector(|| "settings.no-match".into())
                    .text_size(px(13.))
                    .text_color(palette.dim)
                    .child(settings_no_match_text(query))
                    .into_any_element(),
            ];
        }
        let mut sections: Vec<AnyElement> = results
            .groups
            .iter()
            .map(|(category, ids)| self.group_section(*category, ids, palette, cx))
            .collect();
        if !results.keymap_rows.is_empty() {
            sections.push(keymap_results(query, &results.keymap_rows, palette));
        }
        sections
    }

    fn about_section(&self, palette: &Palette) -> Vec<AnyElement> {
        let platform = if cfg!(target_os = "macos") {
            "macOS · gpui"
        } else {
            "Linux · gpui"
        };
        [
            ("Version", env!("CARGO_PKG_VERSION").to_owned()),
            (
                "AWS CLI",
                self.cli_version
                    .clone()
                    .unwrap_or_else(|| "unknown".to_owned()),
            ),
            ("Config file", self.settings.aws_config_file.clone()),
            ("Data folder", "~/.config/clouddeck".to_owned()),
            ("Platform", platform.to_owned()),
        ]
        .into_iter()
        .map(|(label, value)| {
            div()
                .flex()
                .items_center()
                .gap(px(16.))
                .py(px(9.))
                .border_b_1()
                .border_color(palette.line)
                .child(
                    div()
                        .w(px(140.))
                        .text_size(px(12.))
                        .text_color(palette.dim)
                        .child(label),
                )
                .child(div().flex_1().min_w(px(0.)).text_size(px(13.)).child(value))
                .into_any_element()
        })
        .collect()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette(cx);
        let query = self.query(cx);
        let searching = !query.trim().is_empty();
        let results = search_settings(&query);
        let visible = results.visible_categories();
        let changed_dots = changed_categories(&self.settings);
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
                    .filter(|(_, name)| !searching || visible.contains(*name))
                    .map(|(index, name)| {
                        let selected = !searching && *name == self.category;
                        self.category_button(
                            index,
                            name,
                            selected,
                            changed_dots.contains(name),
                            &palette,
                            cx,
                        )
                    }),
            );

        #[cfg(test)]
        {
            let drawn = if searching {
                results.groups.clone()
            } else {
                groups_for(self.category)
            };
            self.rendered = drawn
                .iter()
                .flat_map(|(_, ids)| ids.iter().copied())
                .collect();
            self.rendered_changes = self
                .rendered
                .iter()
                .copied()
                .filter(|id| id.is_changed(&self.settings))
                .collect();
            self.rendered_dots = (0..SETTINGS_CATEGORIES.len())
                .filter(|&index| changed_dots.contains(&SETTINGS_CATEGORIES[index]))
                .collect();
            self.rendered_search = searching;
            self.rendered_categories = (0..SETTINGS_CATEGORIES.len())
                .filter(|&index| !searching || visible.contains(&SETTINGS_CATEGORIES[index]))
                .collect();
        }
        let body = if searching {
            self.search_body(&query, &results, &palette, cx)
        } else {
            match self.category {
                "Keymap" => self.keymap_section(&palette),
                "About" => self.about_section(&palette),
                category => {
                    let mut sections: Vec<AnyElement> = groups_for(category)
                        .into_iter()
                        .map(|(group, ids)| self.group_section(group, &ids, &palette, cx))
                        .collect();
                    if category == "AWS & Privacy" {
                        sections.push(self.stored_data_section(&palette, cx));
                    }
                    sections
                }
            }
        };
        let (breadcrumb, heading) = if searching {
            (settings_search_breadcrumb(&query), results.title())
        } else {
            (
                format!("Settings › {}", self.category),
                self.category.to_owned(),
            )
        };
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
                    .when(searching, |row| {
                        row.debug_selector(|| "settings.search-crumb".into())
                    })
                    .child(breadcrumb),
            )
            .child(
                div()
                    .text_size(px(18.))
                    .text_color(palette.text)
                    .mb(px(14.))
                    .child(heading),
            )
            .children(body);

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
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .when_some(self.note.as_ref(), |row, note| {
                        row.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.))
                                .text_size(px(11.))
                                .text_color(palette.dim)
                                .when_some(note.icon, |note_row, icon| {
                                    note_row.child(icon.icon().xsmall().text_color(palette.accent))
                                })
                                .child(note.text.clone()),
                        )
                    })
                    .child(
                        button("settings-restore", "Restore Defaults", &palette)
                            .debug_selector(|| "settings.restore".into())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.restore_defaults(window, cx)
                            })),
                    )
                    .child(
                        button("settings-done", "Done", &palette)
                            .debug_selector(|| "settings.done".into())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, _| {
                                this.flush_pending_save();
                                window.remove_window();
                            })),
                    ),
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
                let weak = view.downgrade();
                window.on_window_should_close(cx, move |_, cx| {
                    let _ = weak.update(cx, |this, _| this.flush_pending_save());
                    true
                });
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
    use std::sync::atomic::{AtomicUsize, Ordering};

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
        view.update(cx, |view, _| view.path = temp_settings_path());
        (view, cx)
    }

    fn temp_settings_path() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let index = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!("clouddeck-settings-{}-{index}", std::process::id()))
            .join("settings.json")
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
    fn a_change_inside_the_debounce_window_is_saved_on_quit(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());
        let path = view.read_with(cx, |view, _| view.path.clone());

        view.update(cx, |view, cx| {
            view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx)
        });
        cx.run_until_parked();
        assert!(!path.exists(), "the save should wait out the debounce");

        cx.cx.update(|cx| cx.shutdown());

        assert_eq!(Settings::load(&path).theme, Theme::Dark);
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[gpui::test]
    fn done_saves_the_change_before_the_window_closes(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());
        let path = view.read_with(cx, |view, _| view.path.clone());

        view.update(cx, |view, cx| {
            view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx)
        });
        click(cx, "settings.done");

        assert_eq!(Settings::load(&path).theme, Theme::Dark);
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
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

    #[gpui::test]
    fn changed_value_shows_markers_until_reset(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());

        view.update(cx, |view, cx| {
            view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx)
        });

        assert!(cx.debug_bounds("settings.changed.Theme").is_some());
        assert!(cx.debug_bounds("settings.reset.Theme").is_some());
        assert!(cx.debug_bounds("settings.cat-dot.0").is_some());

        click(cx, "settings.reset.Theme");

        assert_eq!(
            view.read_with(cx, |view, _| view.settings.theme),
            Theme::FollowSystem
        );
        assert!(!view.read_with(cx, |view, _| {
            view.rendered_changes.contains(&SettingId::Theme)
        }));
        assert!(!view.read_with(cx, |view, _| view.rendered_dots.contains(&0)));
    }

    #[gpui::test]
    fn restore_defaults_resets_every_setting(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());
        view.update(cx, |view, cx| {
            view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx);
            view.toggle(SettingId::ShowUpdatedTime, cx);
        });

        click(cx, "settings.restore");

        assert_eq!(
            view.read_with(cx, |view, _| view.settings.clone()),
            Settings::default()
        );
    }

    #[gpui::test]
    fn keymap_lists_every_shortcut(cx: &mut TestAppContext) {
        let (_view, cx) = rooted(cx, Settings::default());

        click(cx, "settings.cat.4");

        assert!(cx.debug_bounds("settings.keymap.9").is_some());
        assert!(cx.debug_bounds("settings.keymap.10").is_none());
    }

    #[gpui::test]
    fn aws_pane_has_cli_test_and_config_controls(cx: &mut TestAppContext) {
        let (_view, cx) = rooted(cx, Settings::default());

        click(cx, "settings.cat.5");

        assert!(cx.debug_bounds("settings.control.AwsCliPath").is_some());
        assert!(cx.debug_bounds("settings.cli-test").is_some());
        assert!(cx.debug_bounds("settings.control.AwsConfigFile").is_some());
    }

    fn type_query(view: &Entity<SettingsWindow>, cx: &mut VisualTestContext, query: &'static str) {
        let search = view.read_with(cx, |view, _| view.search.clone());
        search.update_in(cx, |state, window, cx| state.set_value(query, window, cx));
        cx.run_until_parked();
    }

    #[gpui::test]
    fn search_shows_matching_settings_and_keymap_rows(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());

        type_query(&view, cx, "refresh");

        assert!(cx.debug_bounds("settings.search-crumb").is_some());
        assert!(cx.debug_bounds("settings.item.AutoRefresh").is_some());
        assert!(cx.debug_bounds("settings.item.RefreshOnFocus").is_some());
        assert!(cx.debug_bounds("settings.keymap.0").is_some());
        assert!(!view.read_with(cx, |view, _| view.rendered.contains(&SettingId::Theme)));
        assert_eq!(
            view.read_with(cx, |view, cx| search_settings(&view.query(cx)).title()),
            "4 results"
        );
    }

    #[gpui::test]
    fn search_with_no_matches_shows_the_empty_message(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());

        type_query(&view, cx, "zzz");

        assert!(cx.debug_bounds("settings.no-match").is_some());
    }

    #[gpui::test]
    fn clearing_the_query_restores_the_normal_pane(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());
        type_query(&view, cx, "refresh");

        type_query(&view, cx, "");

        assert!(!view.read_with(cx, |view, _| view.rendered_search));
        assert!(cx.debug_bounds("settings.item.Theme").is_some());
    }

    #[gpui::test]
    fn search_tree_shows_only_categories_with_results(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());
        type_query(&view, cx, "refresh");

        assert!(!view.read_with(cx, |view, _| view.rendered_categories.contains(&0)));
        assert!(cx.debug_bounds("settings.cat.1").is_some());
        assert!(cx.debug_bounds("settings.cat.4").is_some());

        click(cx, "settings.cat.1");

        assert_eq!(view.read_with(cx, |view, _| view.category), "Fleet");
        assert!(!view.read_with(cx, |view, _| view.rendered_search));
        assert!(cx.debug_bounds("settings.item.AutoRefresh").is_some());
    }

    #[gpui::test]
    fn escape_clears_the_search(cx: &mut TestAppContext) {
        let (view, cx) = rooted(cx, Settings::default());
        type_query(&view, cx, "refresh");
        let search = view.read_with(cx, |view, _| view.search.clone());
        search.update_in(cx, |state, window, cx| state.focus(window, cx));

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();

        assert!(!view.read_with(cx, |view, _| view.rendered_search));
        assert!(cx.debug_bounds("settings.item.Theme").is_some());
    }

    #[test]
    fn custom_aws_cli_path_is_offered_and_selected() {
        let mut settings = Settings::default();
        settings.aws_cli_path = Some("/home/me/bin/aws".into());
        let items = choices(SettingId::AwsCliPath, &settings);
        let selected = current(SettingId::AwsCliPath, &settings);
        assert!(
            items
                .iter()
                .any(|choice| Some(&choice.pick) == selected.as_ref())
        );
        assert_eq!(items.len(), 4);
    }

    #[test]
    fn preset_aws_cli_path_is_not_duplicated() {
        let mut settings = Settings::default();
        settings.aws_cli_path = Some("/usr/local/bin/aws".into());
        assert_eq!(choices(SettingId::AwsCliPath, &settings).len(), 3);
    }

    #[test]
    fn match_ranges_are_case_insensitive_and_trimmed() {
        assert_eq!(
            match_ranges("Refresh interval, refresh", " REFRESH "),
            vec![0..7, 18..25]
        );
        assert!(match_ranges("Theme", "   ").is_empty());
    }
}
