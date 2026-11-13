use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Pick {
    Theme(Theme),
    Density(Density),
    LaunchBehavior(LaunchBehavior),
    DefaultSort(DefaultSort),
    RefreshInterval(u32),
    RecentSearchLimit(u32),
}

impl Pick {
    pub(super) fn apply(self, settings: &mut Settings) {
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
pub(super) struct Note {
    pub(super) text: SharedString,
    pub(super) icon: Option<AppIcon>,
}

#[derive(Clone)]
pub(super) struct Choice {
    title: SharedString,
    pub(super) pick: Pick,
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

pub(super) fn choice(title: impl Into<SharedString>, pick: Pick) -> Choice {
    Choice {
        title: title.into(),
        pick,
    }
}

pub(super) fn provider_match_count(hits: &[SettingsCategory]) -> usize {
    hits.iter().map(|category| category.settings.len()).sum()
}

pub(super) fn field_key(extension: &str, key: &str) -> FieldKey {
    (extension.to_owned(), key.to_owned())
}

pub(super) fn about_row(label: &'static str, value: impl IntoElement, palette: &Palette) -> AnyElement {
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

pub(super) fn minutes_label(minutes: u32) -> String {
    if minutes == 1 {
        "1 minute".to_owned()
    } else {
        format!("{minutes} minutes")
    }
}

pub(super) fn choices(id: SettingId) -> Vec<Choice> {
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

pub(super) fn current(id: SettingId, settings: &Settings) -> Option<Pick> {
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

pub(super) fn flag(settings: &Settings, id: SettingId) -> Option<bool> {
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

pub(super) fn flag_mut(settings: &mut Settings, id: SettingId) -> Option<&mut bool> {
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

pub(super) fn groups_for(category: &str) -> Vec<(&'static str, Vec<SettingId>)> {
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

pub(super) fn provider_groups(defs: &[SettingDef]) -> Vec<(Option<&str>, Vec<&SettingDef>)> {
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

pub(super) fn section(heading: Option<&str>, rows: Vec<AnyElement>, palette: &Palette) -> AnyElement {
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

pub(super) fn host_platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::Mac
    } else {
        Platform::Linux
    }
}

pub(super) fn keymap_row(
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

pub(super) fn keymap_results(
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
