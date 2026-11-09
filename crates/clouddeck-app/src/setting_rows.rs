//! Setting rows shared by the Settings window's provider categories and the
//! Connect window: one label-and-control row per declared setting.

use std::path::Path;

use clouddeck_core::{SettingDef, SettingKind, SettingOption, SettingValue};
use gpui::{
    AnyElement, App, ClickEvent, Div, Entity, HighlightStyle, SharedString, Stateful, StyledText,
    Window, div, prelude::*, px, white,
};
use gpui_component::IndexPath;
use gpui_component::Sizable;
use gpui_component::input::{Input, InputState};
use gpui_component::select::{Select, SelectItem, SelectState};

use crate::tokens::Palette;

#[derive(Clone)]
pub(crate) struct ProviderChoice {
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

pub(crate) fn display_text(value: Option<SettingValue>) -> String {
    value.map(|value| value.host_text()).unwrap_or_default()
}

pub(crate) fn folder_is_missing(host_text: Option<String>) -> bool {
    host_text.is_some_and(|path| !path.is_empty() && !Path::new(&path).is_dir())
}

pub(crate) fn text_state(
    text: String,
    secret: bool,
    window: &mut Window,
    cx: &mut App,
) -> Entity<InputState> {
    cx.new(|cx| {
        InputState::new(window, cx)
            .default_value(text)
            .masked(secret)
    })
}

pub(crate) fn choice_state(
    options: &[SettingOption],
    selected: &str,
    window: &mut Window,
    cx: &mut App,
) -> Entity<SelectState<Vec<ProviderChoice>>> {
    let items: Vec<ProviderChoice> = options
        .iter()
        .map(|option| ProviderChoice {
            title: option.label.clone().into(),
            value: option.value.clone(),
        })
        .collect();
    let selected = items
        .iter()
        .position(|item| item.value == selected)
        .map(IndexPath::new);
    cx.new(|cx| SelectState::new(items, selected, window, cx))
}

pub(crate) fn unit_suffix(kind: &SettingKind) -> Option<&'static str> {
    match kind {
        SettingKind::DurationSecs { .. } => Some("seconds"),
        _ => None,
    }
}

pub(crate) fn text_field(
    state: &Entity<InputState>,
    kind: &SettingKind,
    selector: String,
    palette: &Palette,
) -> AnyElement {
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

pub(crate) fn choice_field(
    state: &Entity<SelectState<Vec<ProviderChoice>>>,
    selector: String,
    palette: &Palette,
) -> AnyElement {
    div()
        .debug_selector(move || selector)
        .w(px(190.))
        .text_color(palette.text)
        .child(Select::new(state).menu_width(px(220.)))
        .into_any_element()
}

pub(crate) fn folder_field(
    state: &Entity<InputState>,
    selector: String,
    browse: AnyElement,
    clear: AnyElement,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.))
        .child(
            div()
                .debug_selector(move || selector)
                .w(px(260.))
                .child(Input::new(state).small()),
        )
        .child(browse)
        .child(clear)
        .into_any_element()
}

pub(crate) fn switch(
    selector: String,
    on: bool,
    palette: &Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
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
        .id(SharedString::from(selector.clone()))
        .debug_selector(move || selector)
        .cursor_pointer()
        .on_click(on_click)
        .child(track)
        .into_any_element()
}

pub(crate) fn link(
    id: String,
    selector: String,
    label: &'static str,
    palette: &Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(SharedString::from(id))
        .debug_selector(move || selector)
        .cursor_pointer()
        .text_size(px(11.))
        .text_color(palette.accent)
        .on_click(on_click)
        .child(label)
        .into_any_element()
}

pub(crate) fn button(id: &'static str, label: &'static str, palette: &Palette) -> Stateful<Div> {
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

pub(crate) fn highlighted(text: &str, query: &str, palette: &Palette) -> StyledText {
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

fn fold_case(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

pub(crate) fn match_ranges(text: &str, query: &str) -> Vec<std::ops::Range<usize>> {
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

/// One setting's row. `scope` prefixes the debug selectors of its parts
/// (`{scope}.changed.{row}`, `{scope}.error.{row}`, ...); `item` is the row's own selector.
pub(crate) struct RowParts<'a> {
    pub scope: &'static str,
    pub item: String,
    pub row: String,
    pub def: &'a SettingDef,
    pub query: &'a str,
    pub changed: bool,
    pub error: Option<String>,
    pub missing: bool,
    pub reset: Option<AnyElement>,
    pub control: AnyElement,
}

pub(crate) fn setting_row(parts: RowParts<'_>, palette: &Palette) -> AnyElement {
    let RowParts {
        scope,
        item,
        row,
        def,
        query,
        changed,
        error,
        missing,
        reset,
        control,
    } = parts;
    let changed_selector = format!("{scope}.changed.{row}");
    div()
        .debug_selector(move || item)
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
                        .child(
                            div()
                                .w(px(2.))
                                .h(px(14.))
                                .rounded(px(1.))
                                .when(changed, |bar| {
                                    bar.bg(palette.accent)
                                        .debug_selector(move || changed_selector)
                                }),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .child(highlighted(&def.label, query, palette)),
                        )
                        .children(reset),
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
                    let selector = format!("{scope}.error.{row}");
                    column.child(
                        div()
                            .debug_selector(move || selector)
                            .text_size(px(11.))
                            .text_color(palette.err)
                            .child(error),
                    )
                })
                .when(missing, |column| {
                    let selector = format!("{scope}.missing.{row}");
                    column.child(
                        div()
                            .debug_selector(move || selector)
                            .text_size(px(11.))
                            .text_color(palette.warn)
                            .child("Folder not found"),
                    )
                }),
        )
        .child(control)
        .into_any_element()
}
