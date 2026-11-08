//! Settings window (design/images/50-59).

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use clouddeck_core::extension_host::{
    ProviderRegistry, SettingsCategory, search_provider_settings,
};
use clouddeck_core::{
    DEFAULT_AWS_CONFIG_FILE, DefaultSort, Density, KEYMAP, LaunchBehavior, Platform,
    RECENT_SEARCH_LIMITS, REFRESH_INTERVALS, SETTINGS_CATEGORIES, SettingDef, SettingId,
    SettingKind, SettingValue, Settings, SettingsSearchResults, Theme, UiState, aws_config_setting,
    changed_categories, context_path, search_settings, settings_no_match_text, settings_path,
    settings_results_title, settings_search_breadcrumb, shortcut_label, ui_state_path,
};
use gpui::{
    AnyElement, AnyView, App, Bounds, ClickEvent, Context, Div, Entity, EventEmitter, Global,
    HighlightStyle, IntoElement, ParentElement, Render, SharedString, Stateful, Styled, StyledText,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, div, img,
    point, prelude::*, px, size, white,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::select::{Select, SelectEvent, SelectItem, SelectState};
use gpui_component::tooltip::Tooltip;
use gpui_component::{IndexPath, Root, Sizable};

use crate::app_menus;
use crate::icons::AppIcon;
use crate::tokens::{Palette, palette};

const SAVE_DEBOUNCE: Duration = Duration::from_millis(300);
const NOTE_DURATION: Duration = Duration::from_secs(2);
pub(crate) const WINDOW_SIZE: (f32, f32) = (820.0, 560.0);

const FOOTER_HINT: &str =
    "Changes apply immediately. A blue bar marks values that differ from the default.";

const CATEGORY_ICONS: [AppIcon; 9] = [
    AppIcon::PanelLeft,
    AppIcon::Server,
    AppIcon::Filter,
    AppIcon::Search,
    AppIcon::Key,
    AppIcon::Cloud,
    AppIcon::Ellipsis,
    AppIcon::Plug,
    AppIcon::CircleAlert,
];
const PROVIDER_CATEGORY_ICON: AppIcon = AppIcon::Plug;

fn category_icon(index: usize) -> AppIcon {
    CATEGORY_ICONS
        .get(index)
        .copied()
        .unwrap_or(PROVIDER_CATEGORY_ICON)
}

type FieldKey = (String, String);

#[derive(Debug, Clone, PartialEq)]
pub enum SettingsEvent {
    Changed(Settings),
    RecentSearchesCleared,
    RememberedScopeForgotten,
}

pub struct SettingsWindow {
    pub settings: Settings,
    pub category: &'static str,
    providers: Arc<ProviderRegistry>,
    provider_categories: Vec<SettingsCategory>,
    provider: Option<String>,
    provider_inputs: Vec<(FieldKey, Entity<InputState>)>,
    provider_selects: Vec<(FieldKey, Entity<SelectState<Vec<ProviderChoice>>>)>,
    setting_errors: BTreeMap<FieldKey, String>,
    search: Entity<InputState>,
    selects: Vec<(SettingId, Entity<SelectState<Vec<Choice>>>)>,
    _subscriptions: Vec<Subscription>,
    path: PathBuf,
    unsaved: bool,
    save_generation: u64,
    note: Option<Note>,
    note_generation: u64,
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
    RefreshInterval(u32),
    RecentSearchLimit(u32),
}

impl Pick {
    fn apply(self, settings: &mut Settings) {
        match self {
            Pick::Theme(value) => settings.theme = value,
            Pick::Density(value) => settings.density = value,
            Pick::LaunchBehavior(value) => settings.on_launch = value,
            Pick::DefaultSort(value) => settings.default_sort = value,
            Pick::RefreshInterval(value) => settings.refresh_interval_minutes = value,
            Pick::RecentSearchLimit(value) => settings.recent_search_limit = value,
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

#[derive(Clone)]
struct ProviderChoice {
    title: SharedString,
    value: String,
}

impl SelectItem for ProviderChoice {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &String {
        &self.value
    }
}

fn provider_match_count(hits: &[SettingsCategory]) -> usize {
    hits.iter().map(|category| category.settings.len()).sum()
}

fn field_key(extension: &str, key: &str) -> FieldKey {
    (extension.to_owned(), key.to_owned())
}

fn display_text(value: Option<SettingValue>) -> String {
    value.map(|value| value.host_text()).unwrap_or_default()
}

fn unit_suffix(kind: &SettingKind) -> Option<&'static str> {
    match kind {
        SettingKind::DurationSecs { .. } => Some("seconds"),
        _ => None,
    }
}

fn button(id: &'static str, label: &'static str, palette: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
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

fn about_row(label: &'static str, value: impl IntoElement, palette: &Palette) -> AnyElement {
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
        SettingId::RefreshInterval => REFRESH_INTERVALS
            .iter()
            .map(|&minutes| choice(minutes_label(minutes), Pick::RefreshInterval(minutes)))
            .collect(),
        SettingId::RecentSearchLimit => RECENT_SEARCH_LIMITS
            .iter()
            .map(|&limit| choice(limit.to_string(), Pick::RecentSearchLimit(limit)))
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
        SettingId::RefreshInterval => Pick::RefreshInterval(settings.refresh_interval_minutes),
        SettingId::RecentSearchLimit => Pick::RecentSearchLimit(settings.recent_search_limit),
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
        SettingId::CheckForUpdates => Some(settings.check_for_updates),
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
        SettingId::CheckForUpdates => Some(&mut settings.check_for_updates),
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

fn provider_groups(defs: &[SettingDef]) -> Vec<(Option<&str>, Vec<&SettingDef>)> {
    let mut groups: Vec<(Option<&str>, Vec<&SettingDef>)> = Vec::new();
    for def in defs {
        let group = def.group.as_deref();
        match groups.iter_mut().find(|(name, _)| *name == group) {
            Some((_, defs)) => defs.push(def),
            None => groups.push((group, vec![def])),
        }
    }
    groups
}

fn section(heading: Option<&str>, rows: Vec<AnyElement>, palette: &Palette) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .mb(px(18.))
        .when_some(heading, |column, heading| {
            column.child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.faint)
                    .mb(px(4.))
                    .child(heading.to_uppercase()),
            )
        })
        .children(rows)
        .into_any_element()
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
    pub fn new(
        settings: Settings,
        providers: Arc<ProviderRegistry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search settings")
                .clean_on_escape()
        });
        let mut selects = Vec::new();
        let mut subscriptions = vec![cx.observe(&search, |_, _, cx| cx.notify())];
        subscriptions.push(cx.on_app_quit(|this: &mut Self, _| {
            this.flush_pending_save();
            std::future::ready(())
        }));
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
                        this.choose(id, pick.clone(), cx);
                    }
                },
            ));
            selects.push((id, state));
        }
        let provider_categories = providers.settings_categories();
        let mut provider_inputs = Vec::new();
        let mut provider_selects = Vec::new();
        for category in &provider_categories {
            for def in &category.settings {
                let key = field_key(&category.extension, &def.key);
                match &def.kind {
                    SettingKind::Choice(options) => {
                        let items: Vec<ProviderChoice> = options
                            .iter()
                            .map(|option| ProviderChoice {
                                title: option.label.clone().into(),
                                value: option.value.clone(),
                            })
                            .collect();
                        let selected = display_text(providers.setting_value(&key.0, &key.1));
                        let selected = items
                            .iter()
                            .position(|item| item.value == selected)
                            .map(IndexPath::new);
                        let state = cx.new(|cx| SelectState::new(items, selected, window, cx));
                        let extension = category.extension.clone();
                        let def = def.clone();
                        subscriptions.push(cx.subscribe_in(
                            &state,
                            window,
                            move |this: &mut Self,
                                  _,
                                  event: &SelectEvent<Vec<ProviderChoice>>,
                                  _,
                                  cx| {
                                if let SelectEvent::Confirm(Some(value)) = event {
                                    this.edit_provider_choice(&extension, &def, value, cx);
                                }
                            },
                        ));
                        provider_selects.push((key, state));
                    }
                    SettingKind::Bool => {}
                    _ => {
                        let text = display_text(providers.setting_value(&key.0, &key.1));
                        let masked = def.secret;
                        let state = cx.new(|cx| {
                            InputState::new(window, cx)
                                .default_value(text)
                                .masked(masked)
                        });
                        let extension = category.extension.clone();
                        let def = def.clone();
                        subscriptions.push(cx.subscribe_in(
                            &state,
                            window,
                            move |this: &mut Self, state, event: &InputEvent, _, cx| {
                                if matches!(event, InputEvent::Change) {
                                    let text = state.read(cx).value().to_string();
                                    this.edit_provider_text(&extension, &def, &text, cx);
                                }
                            },
                        ));
                        provider_inputs.push((key, state));
                    }
                }
            }
        }
        Self {
            settings,
            category: SETTINGS_CATEGORIES[0],
            providers,
            provider_categories,
            provider: None,
            provider_inputs,
            provider_selects,
            setting_errors: BTreeMap::new(),
            search,
            selects,
            _subscriptions: subscriptions,
            path: settings_path(),
            unsaved: false,
            save_generation: 0,
            note: None,
            note_generation: 0,
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

    fn provider_changed(&self, category: &SettingsCategory) -> bool {
        category.settings.iter().any(|def| {
            self.providers
                .setting_changed(&category.extension, &def.key)
        })
    }

    fn edit_provider_text(
        &mut self,
        extension: &str,
        def: &SettingDef,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        match def.parse(text) {
            Ok(value) => self.store_provider_value(extension, &def.key, value, cx),
            Err(error) => self.reject_provider_value(extension, &def.key, error.0, cx),
        }
    }

    fn edit_provider_choice(
        &mut self,
        extension: &str,
        def: &SettingDef,
        value: &str,
        cx: &mut Context<Self>,
    ) {
        self.edit_provider_text(extension, def, value, cx);
    }

    fn toggle_provider_flag(&mut self, extension: &str, key: &str, cx: &mut Context<Self>) {
        let on = matches!(
            self.providers.setting_value(extension, key),
            Some(SettingValue::Bool(true))
        );
        self.store_provider_value(extension, key, SettingValue::Bool(!on), cx);
    }

    fn store_provider_value(
        &mut self,
        extension: &str,
        key: &str,
        value: SettingValue,
        cx: &mut Context<Self>,
    ) {
        let field = field_key(extension, key);
        if self.providers.setting_value(extension, key).as_ref() == Some(&value) {
            self.setting_errors.remove(&field);
        } else {
            match self.providers.set_setting(extension, key, value) {
                Ok(()) => {
                    self.setting_errors.remove(&field);
                }
                Err(error) => {
                    self.setting_errors.insert(field, error.0);
                }
            }
        }
        cx.notify();
    }

    fn reject_provider_value(
        &mut self,
        extension: &str,
        key: &str,
        message: String,
        cx: &mut Context<Self>,
    ) {
        self.setting_errors
            .insert(field_key(extension, key), message);
        cx.notify();
    }

    fn reset_provider_setting(
        &mut self,
        extension: &str,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.providers.reset_setting(extension, key) {
            Ok(()) => {
                self.setting_errors.remove(&field_key(extension, key));
            }
            Err(error) => {
                self.setting_errors
                    .insert(field_key(extension, key), error.0);
            }
        }
        self.sync_provider_controls(extension, Some(key), window, cx);
        cx.notify();
    }

    fn restore_provider_defaults(
        &mut self,
        extension: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.providers.reset_extension_settings(extension) {
            Ok(()) => self
                .setting_errors
                .retain(|(owner, _), _| owner.as_str() != extension),
            Err(error) => {
                self.show_note(format!("Could not restore: {}", error.0), None, cx);
                return;
            }
        }
        self.sync_provider_controls(extension, None, window, cx);
        self.show_note_for("Defaults restored", Some(AppIcon::Check), cx);
    }

    /// Re-reads the controls of one extension, or of one of its keys, after a reset.
    /// Secrets are skipped: they have no stored value to show.
    fn sync_provider_controls(
        &mut self,
        extension: &str,
        key: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let in_scope =
            |owner: &str, field: &str| owner == extension && key.is_none_or(|key| key == field);
        let secret = |field: &str| {
            self.provider_categories
                .iter()
                .find(|category| category.extension == extension)
                .and_then(|category| category.settings.iter().find(|def| def.key == field))
                .is_some_and(|def| def.secret)
        };
        for ((owner, field), state) in &self.provider_inputs {
            if !in_scope(owner, field) || secret(field) {
                continue;
            }
            let text = display_text(self.providers.setting_value(owner, field));
            state.update(cx, |input, cx| {
                if input.value() != text {
                    input.set_value(text, window, cx);
                }
            });
        }
        for ((owner, field), state) in &self.provider_selects {
            if !in_scope(owner, field) {
                continue;
            }
            let value = display_text(self.providers.setting_value(owner, field));
            state.update(cx, |state, cx| state.set_selected_value(&value, window, cx));
        }
    }

    /// Layout-check entry point: shows a provider's category as a sidebar click would.
    #[cfg(test)]
    pub(crate) fn show_provider(
        &mut self,
        extension: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(index) = self
            .provider_categories
            .iter()
            .position(|category| category.extension == extension)
        {
            self.open_category(SETTINGS_CATEGORIES.len() + index, window, cx);
        }
    }

    /// Layout-check entry point: types into a provider's text field.
    #[cfg(test)]
    pub(crate) fn type_provider_text(
        &mut self,
        extension: &str,
        key: &str,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let field = field_key(extension, key);
        if let Some((_, input)) = self
            .provider_inputs
            .iter()
            .find(|(owner, _)| *owner == field)
        {
            let text = text.to_owned();
            input.update(cx, |input, cx| input.set_value(text, window, cx));
        }
    }

    /// Layout-check entry point: types into the search field.
    #[cfg(test)]
    pub(crate) fn type_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let query = query.to_owned();
        self.search
            .update(cx, |state, cx| state.set_value(query, window, cx));
    }

    fn open_category(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        match SETTINGS_CATEGORIES.get(index) {
            Some(name) => {
                self.category = name;
                self.provider = None;
            }
            None => {
                self.provider = self
                    .provider_categories
                    .get(index - SETTINGS_CATEGORIES.len())
                    .map(|category| category.extension.clone());
            }
        }
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

    fn reset(&mut self, id: SettingId, window: &mut Window, cx: &mut Context<Self>) {
        if id.is_disabled(&self.settings) {
            return;
        }
        self.change(cx, |settings| id.reset(settings));
        self.sync_controls(window, cx);
    }

    fn restore_defaults(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(extension) = self.provider.clone() {
            self.restore_provider_defaults(&extension, window, cx);
            return;
        }
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

    fn edit_ui_state(
        &mut self,
        note: &'static str,
        edit: impl FnOnce(&mut UiState),
        cx: &mut Context<Self>,
    ) -> bool {
        match edit_ui_state_file(&ui_state_path(), edit) {
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
        name: &str,
        selected: bool,
        changed: bool,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let icon = category_icon(index);
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
            .child(
                div()
                    .flex_none()
                    .debug_selector(move || format!("settings.cat-icon.{index}"))
                    .child(icon.icon().xsmall()),
            )
            .child(
                div()
                    .debug_selector(move || format!("settings.cat-label.{index}"))
                    .child(name.to_owned()),
            )
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

    fn control(&self, id: SettingId, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        match flag(&self.settings, id) {
            Some(on) => self.switch(id, on, palette, cx),
            None => self.select_control(id, palette),
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

    fn provider_switch(
        &self,
        extension: &str,
        key: &str,
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
        let extension_owned = extension.to_owned();
        let key_owned = key.to_owned();
        let selector = format!("settings.provider.control.{extension}.{key}");
        div()
            .id(SharedString::from(format!(
                "settings-switch-{extension}-{key}"
            )))
            .debug_selector(move || selector)
            .cursor_pointer()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.toggle_provider_flag(&extension_owned, &key_owned, cx)
            }))
            .child(track)
            .into_any_element()
    }

    fn provider_control(
        &self,
        extension: &str,
        def: &SettingDef,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selector = format!("settings.provider.control.{extension}.{}", def.key);
        let field = field_key(extension, &def.key);
        match &def.kind {
            SettingKind::Bool => {
                let on = matches!(
                    self.providers.setting_value(extension, &def.key),
                    Some(SettingValue::Bool(true))
                );
                self.provider_switch(extension, &def.key, on, palette, cx)
            }
            SettingKind::Choice(_) => {
                match self.provider_selects.iter().find(|(key, _)| *key == field) {
                    Some((_, state)) => div()
                        .debug_selector(move || selector)
                        .w(px(190.))
                        .text_color(palette.text)
                        .child(Select::new(state).menu_width(px(220.)))
                        .into_any_element(),
                    None => div().into_any_element(),
                }
            }
            kind => match self.provider_inputs.iter().find(|(key, _)| *key == field) {
                Some((_, state)) => {
                    let width = match kind {
                        SettingKind::Number { .. } | SettingKind::DurationSecs { .. } => 120.,
                        _ => 260.,
                    };
                    let input = div()
                        .debug_selector(move || selector)
                        .w(px(width))
                        .child(Input::new(state).small());
                    match unit_suffix(kind) {
                        Some(unit) => div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(input)
                            .child(div().text_size(px(11.)).text_color(palette.dim).child(unit))
                            .into_any_element(),
                        None => input.into_any_element(),
                    }
                }
                None => div().into_any_element(),
            },
        }
    }

    fn provider_setting_row(
        &self,
        extension: &str,
        def: &SettingDef,
        query: &str,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row = format!("{extension}.{}", def.key);
        let changed = self.providers.setting_changed(extension, &def.key);
        let error = self
            .setting_errors
            .get(&field_key(extension, &def.key))
            .cloned();
        let extension_owned = extension.to_owned();
        let key_owned = def.key.clone();
        div()
            .debug_selector({
                let row = row.clone();
                move || format!("settings.provider.item.{row}")
            })
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.))
            .py(px(9.))
            .border_b_1()
            .border_color(palette.line)
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
                                    let row = row.clone();
                                    bar.bg(palette.accent).debug_selector(move || {
                                        format!("settings.provider.changed.{row}")
                                    })
                                },
                            ))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .child(highlighted(&def.label, query, palette)),
                            )
                            .when(changed, |label| {
                                let row = row.clone();
                                label.child(
                                    div()
                                        .id(SharedString::from(format!("settings-reset-{row}")))
                                        .debug_selector(move || {
                                            format!("settings.provider.reset.{row}")
                                        })
                                        .cursor_pointer()
                                        .text_size(px(11.))
                                        .text_color(palette.accent)
                                        .on_click(cx.listener(
                                            move |this, _: &ClickEvent, window, cx| {
                                                this.reset_provider_setting(
                                                    &extension_owned,
                                                    &key_owned,
                                                    window,
                                                    cx,
                                                )
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
                            .child(highlighted(
                                def.description.as_deref().unwrap_or_default(),
                                query,
                                palette,
                            )),
                    )
                    .when_some(error, |column, error| {
                        let row = row.clone();
                        column.child(
                            div()
                                .debug_selector(move || format!("settings.provider.error.{row}"))
                                .text_size(px(11.))
                                .text_color(palette.err)
                                .child(error),
                        )
                    }),
            )
            .child(self.provider_control(extension, def, palette, cx))
            .into_any_element()
    }

    fn provider_body(
        &self,
        category: &SettingsCategory,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        provider_groups(&category.settings)
            .into_iter()
            .map(|(group, defs)| {
                let rows = defs
                    .into_iter()
                    .map(|def| self.provider_setting_row(&category.extension, def, "", palette, cx))
                    .collect();
                section(group, rows, palette)
            })
            .collect()
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
                                    |state| state.dimension_selections.clear(),
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
        provider_hits: &[SettingsCategory],
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if results.count() + provider_match_count(provider_hits) == 0 {
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
            .map(|(category, ids)| self.group_section(category, ids, palette, cx))
            .collect();
        for category in provider_hits {
            let rows = category
                .settings
                .iter()
                .map(|def| self.provider_setting_row(&category.extension, def, query, palette, cx))
                .collect();
            sections.push(section(Some(&category.name), rows, palette));
        }
        if !results.keymap_rows.is_empty() {
            sections.push(keymap_results(query, &results.keymap_rows, palette));
        }
        sections
    }

    fn plugins_section(&self, palette: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .text_color(palette.dim)
                        .child("Install, enable and remove bundled, installed and dev plugins."),
                )
                .child(
                    button("plugins.open", "Open Plugins…", palette)
                        .debug_selector(|| "plugins.open".into())
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            cx.dispatch_action(&app_menus::OpenPlugins);
                        })),
                )
                .into_any_element(),
        ]
    }

    fn about_section(&self, palette: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let platform = if cfg!(target_os = "macos") {
            "macOS · gpui"
        } else {
            "Linux · gpui"
        };
        let version = div()
            .debug_selector(|| "about.version".into())
            .child(env!("CARGO_PKG_VERSION"));
        let icon = img(crate::assets::APP_ICON_PATH)
            .size(px(96.))
            .debug_selector(|| "about.icon".into());
        let mut rows = vec![
            div()
                .flex()
                .justify_center()
                .py(px(12.))
                .child(icon)
                .into_any_element(),
            about_row("Version", version, palette),
        ];
        rows.extend(
            [
                (
                    "Config file",
                    aws_config_setting().unwrap_or_else(|| DEFAULT_AWS_CONFIG_FILE.to_owned()),
                ),
                ("Data folder", "~/.config/clouddeck".to_owned()),
                ("Platform", platform.to_owned()),
            ]
            .into_iter()
            .map(|(label, value)| about_row(label, value, palette)),
        );
        rows.push(
            div()
                .pt(px(12.))
                .flex()
                .gap(px(8.))
                .child(
                    button("about.check-updates", "Check for Updates…", palette)
                        .debug_selector(|| "about.check-updates".into())
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            cx.dispatch_action(&app_menus::CheckForUpdates);
                        })),
                )
                .child(
                    button("about.open", "About CloudDeck…", palette)
                        .debug_selector(|| "about.open".into())
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            cx.dispatch_action(&app_menus::About);
                        })),
                )
                .into_any_element(),
        );
        rows
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette(cx);
        let query = self.query(cx);
        let searching = !query.trim().is_empty();
        let results = search_settings(&query);
        let provider_hits = if searching {
            search_provider_settings(&self.provider_categories, &query)
        } else {
            Vec::new()
        };
        let visible = results.visible_categories();
        let provider_visible: Vec<&str> = provider_hits
            .iter()
            .map(|category| category.extension.as_str())
            .collect();
        let selected_provider = if searching {
            None
        } else {
            self.provider.as_deref().and_then(|extension| {
                self.provider_categories
                    .iter()
                    .find(|category| category.extension == extension)
            })
        };
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
                        let selected =
                            !searching && self.provider.is_none() && *name == self.category;
                        self.category_button(
                            index,
                            name,
                            selected,
                            changed_dots.contains(name),
                            &palette,
                            cx,
                        )
                    }),
            )
            .children(
                self.provider_categories
                    .iter()
                    .enumerate()
                    .filter(|(_, category)| {
                        !searching || provider_visible.contains(&category.extension.as_str())
                    })
                    .map(|(offset, category)| {
                        let index = SETTINGS_CATEGORIES.len() + offset;
                        let selected = selected_provider
                            .is_some_and(|selected| selected.extension == category.extension);
                        self.category_button(
                            index,
                            &category.name,
                            selected,
                            self.provider_changed(category),
                            &palette,
                            cx,
                        )
                    }),
            );

        #[cfg(test)]
        {
            let drawn = if searching {
                results.groups.clone()
            } else if selected_provider.is_some() {
                Vec::new()
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
            self.search_body(&query, &results, &provider_hits, &palette, cx)
        } else if let Some(category) = selected_provider {
            self.provider_body(category, &palette, cx)
        } else {
            match self.category {
                "Keymap" => self.keymap_section(&palette),
                "About" => self.about_section(&palette, cx),
                "Plugins" => self.plugins_section(&palette, cx),
                category => {
                    let mut sections: Vec<AnyElement> = groups_for(category)
                        .into_iter()
                        .map(|(group, ids)| self.group_section(group, &ids, &palette, cx))
                        .collect();
                    if category == "Privacy" {
                        sections.push(self.stored_data_section(&palette, cx));
                    }
                    sections
                }
            }
        };
        let (breadcrumb, heading) = if searching {
            let count = results.count() + provider_match_count(&provider_hits);
            (
                settings_search_breadcrumb(&query),
                settings_results_title(count),
            )
        } else if let Some(category) = selected_provider {
            (
                format!("Settings › {}", category.name),
                category.name.clone(),
            )
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
            .debug_selector(|| "settings.footer".into())
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .h(px(44.))
            .px(px(20.))
            .border_t_1()
            .border_color(palette.line)
            .child(
                div()
                    .id("settings-hint")
                    .debug_selector(|| "settings.hint".into())
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(11.))
                    .text_color(palette.dim)
                    .tooltip(move |_, cx| AnyView::from(cx.new(|_| Tooltip::new(FOOTER_HINT))))
                    .child(FOOTER_HINT),
            )
            .child(
                div()
                    .flex_none()
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

pub fn open_settings_window(
    cx: &mut App,
    settings: Settings,
    providers: Arc<ProviderRegistry>,
) -> Option<WindowHandle<Root>> {
    if let Some(handle) = cx.try_global::<OpenSettingsWindow>().map(|open| open.0)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Some(handle);
    }
    let handle = open_window_for(cx, settings, providers, WINDOW_SIZE)?;
    cx.set_global(OpenSettingsWindow(handle));
    Some(handle)
}

/// Always opens a new window, bypassing the single-instance check.
pub(crate) fn open_window_for(
    cx: &mut App,
    settings: Settings,
    providers: Arc<ProviderRegistry>,
    window_size: (f32, f32),
) -> Option<WindowHandle<Root>> {
    let bounds = Bounds::centered(None, size(px(window_size.0), px(window_size.1)), cx);
    cx.open_window(
        WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::from("Settings")),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.0), px(15.0))),
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            app_id: Some(crate::main_window::APP_ID.to_string()),
            ..Default::default()
        },
        move |window, cx| {
            let view = cx.new(|cx| SettingsWindow::new(settings, providers, window, cx));
            let weak = view.downgrade();
            window.on_window_should_close(cx, move |_, cx| {
                let _ = weak.update(cx, |this, _| this.flush_pending_save());
                true
            });
            cx.new(|cx| Root::new(view, window, cx))
        },
    )
    .ok()
}

fn edit_ui_state_file(path: &Path, edit: impl FnOnce(&mut UiState)) -> std::io::Result<()> {
    let mut state = UiState::try_load(path)?;
    edit(&mut state);
    state.save(path)
}

/// The window's root view, for layout checks that drive it without clicks.
#[cfg(test)]
pub(crate) fn window_view(
    window: WindowHandle<Root>,
    cx: &mut App,
) -> Option<Entity<SettingsWindow>> {
    window
        .update(cx, |root, _, _| {
            root.view().clone().downcast::<SettingsWindow>().ok()
        })
        .ok()
        .flatten()
}

/// Two extensions with settings and one without, discovered from `dir` as enabled external extensions.
#[cfg(test)]
pub(crate) fn provider_fixture(dir: &Path) -> ProviderRegistry {
    use std::fs;

    const SAMPLE: &str = r#"id = "sample"
name = "Sample"
version = "0.1.0"
schema_version = 1
api_version = 5

[providers.sample]
name = "Sample"

[[settings]]
key = "region_label"
label = "Region label"
description = "Shown beside each region"
group = "Location"
kind = "text"
default = "us-east-1"

[[settings]]
key = "config_path"
label = "Config path"
group = "Location"
kind = "path"
default = "~/.config/sample"

[[settings]]
key = "verbose"
label = "Verbose logging"
group = "Logging"
kind = "bool"
default = false

[[settings]]
key = "retries"
label = "Retries"
group = "Logging"
kind = "number"
default = 3
min = 0
max = 10

[[settings]]
key = "timeout_secs"
label = "Request timeout"
group = "Logging"
kind = "duration_secs"
default = 30
min = 5
max = 120

[[settings]]
key = "mode"
label = "Mode"
kind = "choice"
default = "fast"
options = [{ value = "fast", label = "Fast" }, { value = "safe", label = "Safe" }]

[[settings]]
key = "token"
label = "API token"
kind = "secret"
"#;
    const TOY: &str = r#"id = "toy"
name = "Toy"
version = "0.1.0"
schema_version = 1
api_version = 5

[providers.toy]
name = "Toy"

[[settings]]
key = "display_name"
label = "Display name"
kind = "text"
default = "Default"
"#;
    const PLAIN: &str = r#"id = "plain"
name = "Plain"
version = "0.1.0"
schema_version = 1
api_version = 5

[providers.plain]
name = "Plain"
"#;
    let root = dir.join("extensions");
    let state_root = dir.join("extension-state");
    fs::create_dir_all(&state_root).unwrap();
    for (id, manifest) in [("sample", SAMPLE), ("toy", TOY), ("plain", PLAIN)] {
        let folder = root.join(id);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("extension.toml"), manifest).unwrap();
        fs::write(state_root.join(format!("{id}.json")), r#"{"enabled":true}"#).unwrap();
    }
    let mut registry = ProviderRegistry::default()
        .with_settings_file(dir.join("settings.json"))
        .with_external(true);
    registry.discover(&root, &state_root).unwrap();
    registry
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
        rooted_with(cx, settings, ProviderRegistry::default())
    }

    fn provider_window(
        cx: &mut TestAppContext,
    ) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
        let dir = temp_settings_path().parent().unwrap().to_path_buf();
        rooted_with(cx, Settings::default(), provider_fixture(&dir))
    }

    fn rooted_with(
        cx: &mut TestAppContext,
        settings: Settings,
        providers: ProviderRegistry,
    ) -> (Entity<SettingsWindow>, &mut VisualTestContext) {
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let providers = Arc::new(providers);
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let view = cx.new(|cx| SettingsWindow::new(settings, providers, window, cx));
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
    fn extensions_with_settings_get_a_category_after_the_builtins(cx: &mut TestAppContext) {
        let (view, cx) = provider_window(cx);

        let names = view.read_with(cx, |view, _| {
            view.provider_categories
                .iter()
                .map(|category| category.name.clone())
                .collect::<Vec<_>>()
        });
        assert_eq!(names, ["Sample", "Toy"]);
        assert!(cx.debug_bounds("settings.cat.9").is_some());
        assert!(cx.debug_bounds("settings.cat.10").is_some());
        assert_eq!(category_icon(9), AppIcon::Plug);
    }

    #[gpui::test]
    fn provider_category_shows_one_row_per_declared_setting(cx: &mut TestAppContext) {
        let (view, cx) = provider_window(cx);
        click(cx, "settings.cat.9");

        assert_eq!(
            view.read_with(cx, |view, _| view.provider.clone())
                .as_deref(),
            Some("sample")
        );
        for row in [
            "settings.provider.item.sample.region_label",
            "settings.provider.item.sample.config_path",
            "settings.provider.item.sample.verbose",
            "settings.provider.item.sample.retries",
            "settings.provider.item.sample.timeout_secs",
            "settings.provider.item.sample.mode",
            "settings.provider.item.sample.token",
        ] {
            assert!(cx.debug_bounds(row).is_some(), "{row}");
        }
        assert!(
            cx.debug_bounds("settings.provider.control.sample.token")
                .is_some()
        );
    }

    #[gpui::test]
    fn editing_a_provider_text_setting_stores_it_in_the_registry(cx: &mut TestAppContext) {
        let (view, cx) = provider_window(cx);
        view.update_in(cx, |view, window, cx| {
            view.show_provider("sample", window, cx)
        });
        cx.run_until_parked();

        view.update_in(cx, |view, window, cx| {
            view.type_provider_text("sample", "region_label", "Europe", window, cx)
        });
        cx.run_until_parked();

        assert_eq!(
            view.read_with(cx, |view, _| view
                .providers
                .setting_value("sample", "region_label")),
            Some(SettingValue::Text("Europe".into()))
        );
        assert!(
            cx.debug_bounds("settings.provider.changed.sample.region_label")
                .is_some()
        );
    }

    #[gpui::test]
    fn invalid_provider_value_shows_an_error_and_is_not_saved(cx: &mut TestAppContext) {
        let (view, cx) = provider_window(cx);
        view.update_in(cx, |view, window, cx| {
            view.show_provider("sample", window, cx)
        });
        cx.run_until_parked();

        view.update_in(cx, |view, window, cx| {
            view.type_provider_text("sample", "retries", "99", window, cx)
        });
        cx.run_until_parked();

        assert!(
            cx.debug_bounds("settings.provider.error.sample.retries")
                .is_some()
        );
        assert!(!view.read_with(cx, |view, _| {
            view.providers.setting_changed("sample", "retries")
        }));
    }

    #[gpui::test]
    fn search_finds_provider_settings_and_clicking_the_hit_opens_its_category(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = provider_window(cx);

        type_query(&view, cx, "timeout");

        assert!(
            cx.debug_bounds("settings.provider.item.sample.timeout_secs")
                .is_some()
        );
        click(cx, "settings.cat.9");
        assert_eq!(
            view.read_with(cx, |view, _| view.provider.clone())
                .as_deref(),
            Some("sample")
        );
        assert!(
            cx.debug_bounds("settings.provider.item.sample.region_label")
                .is_some()
        );
    }

    #[gpui::test]
    fn restore_defaults_resets_only_the_selected_provider(cx: &mut TestAppContext) {
        let (view, cx) = provider_window(cx);
        view.update_in(cx, |view, window, cx| view.show_provider("toy", window, cx));
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            view.type_provider_text("toy", "display_name", "Hi", window, cx)
        });
        view.update_in(cx, |view, window, cx| {
            view.show_provider("sample", window, cx)
        });
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            view.type_provider_text("sample", "region_label", "Europe", window, cx)
        });
        cx.run_until_parked();

        click(cx, "settings.restore");

        assert!(!view.read_with(cx, |view, _| {
            view.providers.setting_changed("sample", "region_label")
        }));
        assert!(view.read_with(cx, |view, _| {
            view.providers.setting_changed("toy", "display_name")
        }));
    }

    #[gpui::test]
    fn restore_defaults_with_a_search_active_leaves_built_in_settings_alone(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = provider_window(cx);
        view.update_in(cx, |view, window, cx| {
            view.show_provider("sample", window, cx)
        });
        cx.run_until_parked();
        view.update_in(cx, |view, window, cx| {
            view.type_provider_text("sample", "region_label", "Europe", window, cx)
        });
        view.update(cx, |view, cx| {
            view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx)
        });
        type_query(&view, cx, "timeout");
        let before = view.read_with(cx, |view, _| view.settings.clone());

        click(cx, "settings.restore");

        assert_eq!(view.read_with(cx, |view, _| view.settings.clone()), before);
        assert!(!view.read_with(cx, |view, _| {
            view.providers.setting_changed("sample", "region_label")
        }));
    }

    #[gpui::test]
    fn every_category_row_has_the_same_icon_gap(cx: &mut TestAppContext) {
        let (_view, cx) = rooted(cx, Settings::default());
        let pairs = [
            ("settings.cat-icon.0", "settings.cat-label.0"),
            ("settings.cat-icon.1", "settings.cat-label.1"),
            ("settings.cat-icon.2", "settings.cat-label.2"),
            ("settings.cat-icon.3", "settings.cat-label.3"),
            ("settings.cat-icon.4", "settings.cat-label.4"),
            ("settings.cat-icon.5", "settings.cat-label.5"),
            ("settings.cat-icon.6", "settings.cat-label.6"),
            ("settings.cat-icon.7", "settings.cat-label.7"),
            ("settings.cat-icon.8", "settings.cat-label.8"),
        ];
        for (icon_selector, label_selector) in pairs {
            let icon = cx.debug_bounds(icon_selector).expect("icon rendered");
            let label = cx.debug_bounds(label_selector).expect("label rendered");
            let gap = f32::from(label.origin.x - (icon.origin.x + icon.size.width));
            assert!((gap - 8.0).abs() < 0.5, "{label_selector}: gap {gap}px");
        }
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
            "settings.cat.8",
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
            cx.update(|cx| {
                open_settings_window(
                    cx,
                    Settings::default(),
                    Arc::new(ProviderRegistry::default()),
                )
            })
            .is_some()
        );
        assert!(
            cx.update(|cx| {
                open_settings_window(
                    cx,
                    Settings::default(),
                    Arc::new(ProviderRegistry::default()),
                )
            })
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
    fn about_pane_shows_version_and_update_button(cx: &mut TestAppContext) {
        let (_view, cx) = rooted(cx, Settings::default());

        click(cx, "settings.cat.8");

        assert!(cx.debug_bounds("about.version").is_some());
        assert!(cx.debug_bounds("about.check-updates").is_some());
        let icon = cx.debug_bounds("about.icon").expect("about icon rendered");
        assert!(icon.size.width > px(0.) && icon.size.height > px(0.));
    }

    #[gpui::test]
    fn about_pane_opens_the_about_window(cx: &mut TestAppContext) {
        let (_view, cx) = rooted(cx, Settings::default());
        cx.update(|_, app| app_menus::install(app));
        click(cx, "settings.cat.8");
        let before = cx.update(|_, app| app.windows().len());

        click(cx, "about.open");

        assert_eq!(cx.update(|_, app| app.windows().len()), before + 1);
    }

    #[gpui::test]
    fn plugins_pane_opens_the_plugins_window(cx: &mut TestAppContext) {
        let (_view, cx) = rooted(cx, Settings::default());
        cx.update(|_, app| app_menus::install(app));
        click(cx, "settings.cat.7");
        let before = cx.update(|_, app| app.windows().len());

        click(cx, "plugins.open");

        assert_eq!(cx.update(|_, app| app.windows().len()), before + 1);
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
    fn match_ranges_are_case_insensitive_and_trimmed() {
        assert_eq!(
            match_ranges("Refresh interval, refresh", " REFRESH "),
            vec![0..7, 18..25]
        );
        assert!(match_ranges("Theme", "   ").is_empty());
    }

    #[test]
    fn edit_ui_state_file_creates_missing_file() {
        let path = temp_settings_path().with_file_name("ui-state.json");
        edit_ui_state_file(&path, |state| state.sidebar_open = false).unwrap();
        assert!(!UiState::load(&path).sidebar_open);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn edit_ui_state_file_leaves_corrupt_file_untouched() {
        let path = temp_settings_path().with_file_name("ui-state.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"{not json").unwrap();
        let mut called = false;
        assert!(edit_ui_state_file(&path, |_| called = true).is_err());
        assert!(!called);
        assert_eq!(std::fs::read(&path).unwrap(), b"{not json");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn duration_settings_show_seconds_unit() {
        let kind = SettingKind::DurationSecs {
            min: None,
            max: None,
        };
        assert_eq!(unit_suffix(&kind), Some("seconds"));
    }

    fn setting(key: &str, group: Option<&str>) -> SettingDef {
        SettingDef {
            key: key.to_string(),
            label: key.to_string(),
            description: None,
            group: group.map(str::to_string),
            kind: SettingKind::Text,
            default: None,
            secret: false,
        }
    }

    fn category(settings: Vec<SettingDef>) -> SettingsCategory {
        SettingsCategory {
            extension: "aws".to_string(),
            name: "AWS".to_string(),
            settings,
        }
    }

    #[test]
    fn provider_match_count_sums_settings_across_categories() {
        let hits = [
            category(vec![setting("a", None), setting("b", None)]),
            category(Vec::new()),
            category(vec![setting("c", None)]),
        ];
        assert_eq!(provider_match_count(&hits), 3);
        assert_eq!(provider_match_count(&[]), 0);
    }

    #[test]
    fn field_key_pairs_extension_with_setting_key() {
        assert_eq!(
            field_key("aws", "region"),
            ("aws".to_string(), "region".to_string())
        );
    }

    #[test]
    fn display_text_is_empty_without_a_value() {
        assert_eq!(display_text(None), "");
        assert_eq!(display_text(Some(SettingValue::Bool(true))), "true");
        assert_eq!(display_text(Some(SettingValue::DurationSecs(45))), "45");
    }

    #[test]
    fn provider_groups_keep_first_appearance_order() {
        let defs = [
            setting("a", Some("Network")),
            setting("b", None),
            setting("c", Some("Network")),
            setting("d", Some("Auth")),
        ];
        let groups: Vec<(Option<&str>, Vec<&str>)> = provider_groups(&defs)
            .into_iter()
            .map(|(name, defs)| (name, defs.iter().map(|def| def.key.as_str()).collect()))
            .collect();
        assert_eq!(
            groups,
            vec![
                (Some("Network"), vec!["a", "c"]),
                (None, vec!["b"]),
                (Some("Auth"), vec!["d"]),
            ]
        );
    }

    #[test]
    fn host_platform_matches_build_target() {
        let expected = if cfg!(target_os = "macos") {
            Platform::Mac
        } else {
            Platform::Linux
        };
        assert_eq!(host_platform(), expected);
    }
}
