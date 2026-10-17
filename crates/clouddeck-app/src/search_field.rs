//! Query search field with highlighting and autocomplete (design/images/21-27).

use std::ops::Range;
use std::time::SystemTime;

use clouddeck_core::{
    DEFAULT_RECENT_SEARCH_LIMIT, RECENT_SEARCHES_SHOWN, Suggestion, SuggestionKind, Suggestions,
    TermKind, apply_suggestion, compile, known_tag_keys, push_recent, suggest,
};
use gpui::{
    AnyWindowHandle, App, ClickEvent, Context, Entity, Focusable, Font, Global, Hsla, IntoElement,
    KeyBinding, Render, SharedString, StyledText, TextRun, UnderlineStyle, WeakEntity, Window,
    actions, div, prelude::*, px, rems,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Sizable, Size};

use crate::fleet_state::FleetState;
use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, Palette, STATUS_FONT_SIZE};

const SEARCH_WIDTH: f32 = 220.0;
const SEARCH_HEIGHT: f32 = 28.0;
const SEARCH_RADIUS: f32 = 6.0;
const SIDE_PADDING: f32 = 7.0;
const GAP: f32 = 6.0;
const BORDER_WIDTH: f32 = 1.0;
const ICON_WIDTH: f32 = 12.0;
const ERROR_OFFSET: f32 = 4.0;
const CARET_HEIGHT: f32 = 14.0;
// Mirrors gpui-component's Input, which keeps the caret this far inside the text box.
const RIGHT_MARGIN: f32 = 10.0;
const SUGGESTIONS_WIDTH: f32 = 300.0;
const RECENT_WIDTH: f32 = 260.0;
const ERROR_ROW_HEIGHT: f32 = 26.0;
const RECENT_ICON_SIZE: f32 = 13.0;
const PLACEHOLDER: &str = "Search or filter";
const SYNTAX_REMINDER: &str = "Filter with field:value · -not · a,b either · * wildcard";

actions!(
    search_field,
    [
        FocusSearch,
        PreviousSuggestion,
        NextSuggestion,
        AcceptSuggestion,
        SubmitSearch
    ]
);

/// Colour role of a span of the query text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightKind {
    Word,
    FieldName,
    Value,
    Negation,
    Error,
}

/// Byte ranges of `query` with their colour roles; whitespace between tokens is not covered.
pub fn highlight_runs(
    query: &str,
    known_tag_keys: &[String],
) -> Vec<(Range<usize>, HighlightKind)> {
    let compiled = compile(query, known_tag_keys, SystemTime::now());
    let errored: Vec<usize> = compiled.errors().iter().map(|e| e.term_index).collect();
    let mut runs = Vec::new();
    for (index, term) in compiled.terms().iter().enumerate() {
        let token = &term.token;
        if errored.contains(&index) {
            runs.push((token.start..token.end, HighlightKind::Error));
            continue;
        }
        match &term.kind {
            TermKind::Word { .. } => runs.push((token.start..token.end, HighlightKind::Word)),
            TermKind::Unknown { .. } => runs.push((token.start..token.end, HighlightKind::Error)),
            TermKind::Field { name, .. } | TermKind::TagShorthand { name, .. } => {
                let body_start = token.start + usize::from(term.negated);
                let value_start = body_start + name.len() + 1;
                if term.negated {
                    runs.push((token.start..body_start, HighlightKind::Negation));
                }
                runs.push((body_start..value_start, HighlightKind::FieldName));
                runs.push((value_start..token.end, HighlightKind::Value));
            }
        }
    }
    runs.retain(|(range, _)| !range.is_empty());
    runs
}

fn span_colour(kind: HighlightKind, palette: &Palette) -> Hsla {
    match kind {
        HighlightKind::Word | HighlightKind::Value => palette.text,
        HighlightKind::FieldName => palette.accent,
        HighlightKind::Negation => palette.err,
        HighlightKind::Error => palette.warntext,
    }
}

fn text_run(len: usize, font: &Font, color: Hsla, underline: Option<UnderlineStyle>) -> TextRun {
    TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline,
        strikethrough: None,
    }
}

fn text_runs(
    text: &str,
    spans: &[(Range<usize>, HighlightKind)],
    font: &Font,
    palette: &Palette,
) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut at = 0;
    for (range, kind) in spans {
        if range.start > at {
            runs.push(text_run(range.start - at, font, palette.text, None));
        }
        let underline = (*kind == HighlightKind::Error).then(|| UnderlineStyle {
            thickness: px(1.0),
            color: Some(palette.warntext),
            wavy: true,
        });
        runs.push(text_run(
            range.len(),
            font,
            span_colour(*kind, palette),
            underline,
        ));
        at = range.end;
    }
    if text.len() > at {
        runs.push(text_run(text.len() - at, font, palette.text, None));
    }
    runs
}

fn error_row(message: &str, palette: &Palette) -> impl IntoElement {
    div()
        .debug_selector(|| "search.error".into())
        .absolute()
        .top(px(SEARCH_HEIGHT + ERROR_OFFSET))
        .left_0()
        .w(px(SEARCH_WIDTH))
        .flex()
        .items_center()
        .gap(px(GAP))
        .px(px(SIDE_PADDING))
        .py(px(4.0))
        .rounded(px(SEARCH_RADIUS))
        .bg(palette.panel)
        .border_1()
        .border_color(palette.line)
        .text_size(px(STATUS_FONT_SIZE))
        .text_color(palette.warntext)
        .child(
            AppIcon::TriangleAlert
                .icon()
                .xsmall()
                .text_color(palette.warntext),
        )
        .child(message.to_string())
}

fn suggestion_row(
    index: usize,
    item: &Suggestion,
    highlighted: bool,
    palette: &Palette,
) -> impl IntoElement {
    let label_colour = match item.kind {
        SuggestionKind::Field => palette.accent,
        SuggestionKind::Value => palette.text,
    };
    div()
        .debug_selector(move || format!("search.suggestion.{index}"))
        .flex()
        .items_center()
        .gap(px(GAP))
        .px(px(SIDE_PADDING))
        .py(px(3.0))
        .when(highlighted, |row| row.bg(palette.sel))
        .child(div().text_color(label_colour).child(item.label.clone()))
        .when_some(item.hint.clone(), |row, hint| {
            row.child(div().flex_1().text_color(palette.dim).child(hint))
        })
        .when_some(item.count, |row, count| {
            row.child(
                div()
                    .ml_auto()
                    .text_color(palette.faint)
                    .child(count.to_string()),
            )
        })
}

fn unknown_field_row(name: &str, palette: &Palette) -> impl IntoElement {
    div()
        .debug_selector(|| "search.unknown".into())
        .flex()
        .items_center()
        .gap(px(GAP))
        .px(px(SIDE_PADDING))
        .py(px(4.0))
        .text_color(palette.warntext)
        .child(
            AppIcon::TriangleAlert
                .icon()
                .xsmall()
                .text_color(palette.warntext),
        )
        .child(format!(
            "No field named “{name}”. It will be searched as text."
        ))
}

fn suggestion_panel(
    suggestions: &Suggestions,
    highlight: usize,
    top: f32,
    palette: &Palette,
) -> impl IntoElement {
    let mut panel = div()
        .debug_selector(|| "search.suggestions".into())
        .absolute()
        .top(px(top))
        .right_0()
        .w(px(SUGGESTIONS_WIDTH))
        .flex()
        .flex_col()
        .py(px(4.0))
        .rounded(px(SEARCH_RADIUS))
        .bg(palette.panel)
        .border_1()
        .border_color(palette.line)
        .text_size(px(STATUS_FONT_SIZE));
    if let Some(name) = &suggestions.unknown_field {
        panel = panel.child(unknown_field_row(name, palette));
    }
    for (index, item) in suggestions.items.iter().enumerate() {
        panel = panel.child(suggestion_row(index, item, index == highlight, palette));
    }
    if !suggestions.items.is_empty() {
        panel = panel.child(
            div()
                .px(px(SIDE_PADDING))
                .py(px(4.0))
                .text_color(palette.faint)
                .child("↑↓ choose · Tab or ↵ accept · esc clear"),
        );
    }
    panel
}

fn recent_row(
    index: usize,
    query: &str,
    palette: &Palette,
    cx: &Context<SearchField>,
) -> impl IntoElement {
    let query = query.to_string();
    div()
        .id(SharedString::from(format!("search.recent.{index}")))
        .debug_selector(move || format!("search.recent.{index}"))
        .flex()
        .items_center()
        .gap(px(GAP))
        .px(px(SIDE_PADDING))
        .py(px(3.0))
        .text_color(palette.text)
        .child(
            AppIcon::Search
                .icon()
                .with_size(Size::Size(px(RECENT_ICON_SIZE)))
                .text_color(palette.faint),
        )
        .child(query.clone())
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.replace_query(&query, window, cx);
        }))
}

fn recent_panel(
    recent: &[String],
    top: f32,
    palette: &Palette,
    cx: &Context<SearchField>,
) -> impl IntoElement {
    let mut panel = div()
        .debug_selector(|| "search.recent".into())
        .absolute()
        .top(px(top))
        .right_0()
        .w(px(RECENT_WIDTH))
        .flex()
        .flex_col()
        .py(px(4.0))
        .rounded(px(SEARCH_RADIUS))
        .bg(palette.panel)
        .border_1()
        .border_color(palette.line)
        .text_size(px(STATUS_FONT_SIZE));
    if !recent.is_empty() {
        panel = panel.child(
            div()
                .px(px(SIDE_PADDING))
                .py(px(3.0))
                .text_color(palette.faint)
                .child("Recent"),
        );
        for (index, query) in recent.iter().take(RECENT_SEARCHES_SHOWN).enumerate() {
            panel = panel.child(recent_row(index, query, palette, cx));
        }
    }
    panel.child(
        div()
            .debug_selector(|| "search.syntax-hint".into())
            .px(px(SIDE_PADDING))
            .py(px(4.0))
            .text_color(palette.faint)
            .child(SYNTAX_REMINDER),
    )
}

pub fn search_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-f", FocusSearch, None),
        KeyBinding::new("/", FocusSearch, Some("InstanceTable")),
        KeyBinding::new("up", PreviousSuggestion, Some("SearchField")),
        KeyBinding::new("down", NextSuggestion, Some("SearchField")),
        KeyBinding::new("tab", AcceptSuggestion, Some("SearchField")),
        KeyBinding::new("enter", SubmitSearch, Some("SearchField")),
    ]
}

fn suggestions_for(state: &FleetState) -> Suggestions {
    let instances = state.view_model.instances();
    suggest(state.query(), instances, &known_tag_keys(instances))
}

struct FocusSearchTarget {
    window: AnyWindowHandle,
    input: WeakEntity<InputState>,
}

#[derive(Default)]
struct FocusSearchTargets(Vec<FocusSearchTarget>);

impl Global for FocusSearchTargets {}

pub struct SearchField {
    state: Entity<FleetState>,
    pub input: Entity<InputState>,
    scroll_x: f32,
    highlight: usize,
}

impl SearchField {
    pub fn new(state: Entity<FleetState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));

        cx.subscribe(&input, |this, input, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                let text = input.read(cx).value().to_string();
                this.highlight = 0;
                this.state
                    .update(cx, |state, cx| state.set_query(&text, cx));
                cx.notify();
            }
            InputEvent::Focus => cx.notify(),
            InputEvent::Blur => {
                this.remember_query(cx);
                cx.notify();
            }
            _ => {}
        })
        .detach();

        cx.observe_in(&state, window, |this, state, window, cx| {
            let query = state.read(cx).query().to_string();
            if this.input.read(cx).value().as_ref() != query.as_str() {
                this.highlight = 0;
                this.input
                    .update(cx, |input, cx| input.set_value(query, window, cx));
            }
            cx.notify();
        })
        .detach();

        let first_search_field = !cx.has_global::<FocusSearchTargets>();
        let targets = cx.default_global::<FocusSearchTargets>();
        targets.0.retain(|target| target.input.upgrade().is_some());
        targets.0.push(FocusSearchTarget {
            window: window.window_handle(),
            input: input.downgrade(),
        });
        if first_search_field {
            App::on_action(cx, |_: &FocusSearch, cx| {
                let candidates: Vec<_> = cx
                    .global::<FocusSearchTargets>()
                    .0
                    .iter()
                    .rev()
                    .map(|target| (target.window, target.input.clone()))
                    .collect();
                cx.defer(move |cx| {
                    for (window_handle, input_weak) in candidates {
                        let focused = window_handle
                            .update(cx, |_, window, cx| {
                                input_weak
                                    .update(cx, |input, cx| input.focus(window, cx))
                                    .is_ok()
                            })
                            .unwrap_or(false);
                        if focused {
                            break;
                        }
                    }
                });
            });
        }

        Self {
            state,
            input,
            scroll_x: 0.0,
            highlight: 0,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    fn step_highlight(&mut self, forward: bool, cx: &mut Context<Self>) {
        let len = suggestions_for(self.state.read(cx)).items.len();
        if len == 0 {
            cx.propagate();
            return;
        }
        let current = self.highlight.min(len - 1);
        self.highlight = if forward {
            (current + 1) % len
        } else {
            (current + len - 1) % len
        };
        cx.notify();
    }

    fn previous_suggestion(
        &mut self,
        _: &PreviousSuggestion,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.step_highlight(false, cx);
    }

    fn next_suggestion(&mut self, _: &NextSuggestion, _: &mut Window, cx: &mut Context<Self>) {
        self.step_highlight(true, cx);
    }

    fn remember_query(&self, cx: &mut App) {
        let query = self.state.read(cx).query().to_string();
        self.state.update(cx, |state, cx| {
            push_recent(
                &mut state.recent_searches,
                &query,
                DEFAULT_RECENT_SEARCH_LIMIT,
            );
            cx.notify();
        });
    }

    fn replace_query(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.highlight = 0;
        self.input.update(cx, |input, cx| {
            input.set_value(query.to_string(), window, cx)
        });
        self.state
            .update(cx, |state, cx| state.set_query(query, cx));
        cx.notify();
    }

    fn accept_highlighted(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let state = self.state.read(cx);
        let suggestions = suggestions_for(state);
        let index = self
            .highlight
            .min(suggestions.items.len().saturating_sub(1));
        let Some(query) = apply_suggestion(state.query(), &suggestions, index) else {
            return false;
        };
        self.replace_query(&query, window, cx);
        true
    }

    fn accept_suggestion(
        &mut self,
        _: &AcceptSuggestion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.accept_highlighted(window, cx) {
            cx.propagate();
        }
    }

    fn submit_search(&mut self, _: &SubmitSearch, window: &mut Window, cx: &mut Context<Self>) {
        if !self.accept_highlighted(window, cx) {
            self.remember_query(cx);
        }
    }
}

impl Render for SearchField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::tokens::palette(cx);
        let input = self.input.read(cx);
        let focused = input.focus_handle(cx).is_focused(window);
        let typed = input.value().to_string();
        let cursor = input.cursor();
        let state = self.state.read(cx);
        let tag_keys = known_tag_keys(state.view_model.instances());
        let error = state
            .view_model
            .query_errors()
            .first()
            .map(|e| e.message.clone());
        let suggestions = suggestions_for(state);
        let highlight = self
            .highlight
            .min(suggestions.items.len().saturating_sub(1));
        let dropdown_open =
            focused && (!suggestions.items.is_empty() || suggestions.unknown_field.is_some());

        let inset = f32::from(Size::Medium.input_px());
        let viewport =
            SEARCH_WIDTH - 2.0 * BORDER_WIDTH - 2.0 * SIDE_PADDING - ICON_WIDTH - GAP - 2.0 * inset;
        let font = window.text_style().font();
        let (shown, runs) = if typed.is_empty() {
            (
                PLACEHOLDER.to_string(),
                vec![text_run(PLACEHOLDER.len(), &font, palette.faint, None)],
            )
        } else {
            let spans = highlight_runs(&typed, &tag_keys);
            (typed.clone(), text_runs(&typed, &spans, &font, &palette))
        };
        let caret_x = window
            .text_system()
            .shape_line(shown.clone().into(), px(FONT_SIZE), &runs, None)
            .x_for_index(cursor);
        let caret_at = f32::from(caret_x);
        if typed.is_empty() {
            self.scroll_x = 0.0;
        } else if caret_at - RIGHT_MARGIN < self.scroll_x {
            self.scroll_x = caret_at - RIGHT_MARGIN;
        } else if caret_at + RIGHT_MARGIN > self.scroll_x + viewport {
            self.scroll_x = caret_at + RIGHT_MARGIN - viewport;
        }
        self.scroll_x = self.scroll_x.max(0.0);
        let shift = self.scroll_x;

        let caret = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(caret_x)
            .flex()
            .items_center()
            .child(div().w(px(1.0)).h(px(CARET_HEIGHT)).bg(palette.text));
        let overlay = div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .overflow_hidden()
            .px(px(inset))
            .child(
                div()
                    .relative()
                    .left(px(-shift))
                    .text_size(px(FONT_SIZE))
                    .line_height(rems(1.25))
                    .child(StyledText::new(shown).with_runs(runs))
                    .when(focused, |text| text.child(caret)),
            );
        let input_field = Input::new(&self.input)
            .appearance(false)
            .bordered(false)
            .text_size(px(FONT_SIZE))
            .opacity(0.0);
        let text_box = div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .relative()
            .child(overlay)
            .child(input_field);

        let field = div()
            .debug_selector(|| "search.field".into())
            .key_context("SearchField")
            .on_action(cx.listener(Self::previous_suggestion))
            .on_action(cx.listener(Self::next_suggestion))
            .on_action(cx.listener(Self::accept_suggestion))
            .on_action(cx.listener(Self::submit_search))
            .flex()
            .items_center()
            .gap(px(GAP))
            .w(px(SEARCH_WIDTH))
            .h(px(SEARCH_HEIGHT))
            .px(px(SIDE_PADDING))
            .rounded(px(SEARCH_RADIUS))
            .bg(palette.surf)
            .border_1()
            .border_color(palette.line)
            .text_size(px(FONT_SIZE))
            .text_color(palette.faint)
            .child(AppIcon::Search.icon().xsmall().text_color(palette.faint))
            .child(text_box)
            .when(!focused, |field| {
                field.child(
                    div()
                        .debug_selector(|| "search.hint".into())
                        .text_size(px(STATUS_FONT_SIZE))
                        .child("⌘F"),
                )
            });

        let error_offset = if error.is_some() {
            ERROR_ROW_HEIGHT + ERROR_OFFSET
        } else {
            0.0
        };
        let panel = dropdown_open.then(|| {
            suggestion_panel(
                &suggestions,
                highlight,
                SEARCH_HEIGHT + ERROR_OFFSET + error_offset,
                &palette,
            )
        });
        let recent = (focused && typed.is_empty()).then(|| {
            recent_panel(
                &state.recent_searches,
                SEARCH_HEIGHT + ERROR_OFFSET,
                &palette,
                cx,
            )
        });

        div()
            .relative()
            .w(px(SEARCH_WIDTH))
            .child(field)
            .when_some(error, |outer, message| {
                outer.child(error_row(&message, &palette))
            })
            .when_some(panel, |outer, panel| outer.child(panel))
            .when_some(recent, |outer, recent| outer.child(recent))
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use clouddeck_core::{EC2Instance, EC2InstanceTag, MockComputeService};
    use gpui::{Entity, TestAppContext, VisualTestContext};

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

    fn loaded_view_model() -> InstanceListViewModel {
        let instances = vec![
            instance("i-1", "api-server", "running"),
            instance("i-2", "bastion", "stopped"),
            instance("i-3", "worker", "stopped"),
        ];
        let mut vm = InstanceListViewModel::new(Box::new(MockComputeService::new(instances, None)));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(vm.load_instances());
        vm
    }

    fn three_state_view_model() -> InstanceListViewModel {
        let instances = vec![
            instance("i-1", "api-server", "running"),
            instance("i-2", "bastion", "stopped"),
            instance("i-3", "batcher", "pending"),
        ];
        let mut vm = InstanceListViewModel::new(Box::new(MockComputeService::new(instances, None)));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(vm.load_instances());
        vm
    }

    fn rooted_search(
        cx: &mut TestAppContext,
        focus_on_open: bool,
    ) -> (
        Entity<FleetState>,
        Entity<SearchField>,
        &mut VisualTestContext,
    ) {
        rooted_search_with(cx, focus_on_open, loaded_view_model())
    }

    fn rooted_search_with(
        cx: &mut TestAppContext,
        focus_on_open: bool,
        view_model: InstanceListViewModel,
    ) -> (
        Entity<FleetState>,
        Entity<SearchField>,
        &mut VisualTestContext,
    ) {
        let state = cx.new(|_| FleetState::new(view_model, "default".to_string(), true));
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let state_for_window = state.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            cx.bind_keys(search_key_bindings());
            let search = cx.new(|cx| SearchField::new(state_for_window.clone(), window, cx));
            if focus_on_open {
                search.update(cx, |search, cx| search.focus(window, cx));
            }
            *slot_for_window.borrow_mut() = Some(search.clone());
            gpui_component::Root::new(search, window, cx)
        });
        let search = slot
            .borrow()
            .as_ref()
            .expect("search field entity should be created")
            .clone();
        (state, search, cx)
    }

    fn input_text(search: &Entity<SearchField>, cx: &mut VisualTestContext) -> String {
        let input = search.read_with(cx, |search, _| search.input.clone());
        input.read_with(cx, |input, _| input.value().to_string())
    }

    #[gpui::test]
    fn renders_the_search_field_with_its_hint(cx: &mut TestAppContext) {
        let (_state, _search, cx) = rooted_search(cx, false);

        assert!(cx.debug_bounds("search.field").is_some());
        assert!(cx.debug_bounds("search.hint").is_some());
    }

    #[gpui::test]
    fn hint_is_hidden_while_the_input_is_focused(cx: &mut TestAppContext) {
        let (_state, _search, cx) = rooted_search(cx, true);

        assert!(cx.debug_bounds("search.field").is_some());
        assert!(cx.debug_bounds("search.hint").is_none());
    }

    #[gpui::test]
    fn focus_search_falls_back_to_an_open_window_when_the_newest_is_closed(
        cx: &mut TestAppContext,
    ) {
        let (state, search, cx) = rooted_search(cx, false);
        let (_root, newer) = cx.add_window_view(move |window, cx| {
            let search = cx.new(|cx| SearchField::new(state.clone(), window, cx));
            gpui_component::Root::new(search, window, cx)
        });
        newer.update(|window, _| window.remove_window());
        newer.run_until_parked();

        cx.dispatch_action(FocusSearch);
        cx.run_until_parked();

        let input = search.read_with(cx, |search, _| search.input.clone());
        let focused = cx.update(|window, cx| input.read(cx).focus_handle(cx).is_focused(window));
        assert!(
            focused,
            "FocusSearch should focus the open window's search field"
        );
    }

    #[gpui::test]
    fn typing_sets_the_fleet_query(cx: &mut TestAppContext) {
        let (state, _search, cx) = rooted_search(cx, false);

        let field = cx
            .debug_bounds("search.field")
            .expect("search field renders");
        cx.simulate_click(field.center(), Default::default());
        cx.simulate_input("state:stopped");
        cx.run_until_parked();

        assert_eq!(
            state.read_with(cx, |state, _| state.query().to_string()),
            "state:stopped"
        );
    }

    #[gpui::test]
    fn external_query_updates_the_input_text(cx: &mut TestAppContext) {
        let (state, search, cx) = rooted_search(cx, false);

        state.update(cx, |state, cx| state.set_query("owner:esteban", cx));
        cx.run_until_parked();

        assert_eq!(input_text(&search, cx), "owner:esteban");
    }

    fn owner_keys() -> Vec<String> {
        vec!["Owner".to_string()]
    }

    #[test]
    fn plain_word_is_one_word_run() {
        assert_eq!(
            highlight_runs("web", &owner_keys()),
            vec![(0..3, HighlightKind::Word)]
        );
    }

    #[test]
    fn negated_field_splits_into_negation_name_and_value() {
        assert_eq!(
            highlight_runs("-state:stopped", &owner_keys()),
            vec![
                (0..1, HighlightKind::Negation),
                (1..7, HighlightKind::FieldName),
                (7..14, HighlightKind::Value),
            ]
        );
    }

    #[test]
    fn tag_shorthand_and_word_skip_the_whitespace_between_them() {
        assert_eq!(
            highlight_runs("owner:esteban web", &owner_keys()),
            vec![
                (0..6, HighlightKind::FieldName),
                (6..13, HighlightKind::Value),
                (14..17, HighlightKind::Word),
            ]
        );
    }

    #[test]
    fn unknown_field_and_invalid_age_are_error_runs() {
        assert_eq!(
            highlight_runs("colour:red", &owner_keys()),
            vec![(0..10, HighlightKind::Error)]
        );
        assert_eq!(
            highlight_runs("age:30d", &owner_keys()),
            vec![(0..7, HighlightKind::Error)]
        );
    }

    #[gpui::test]
    fn invalid_query_shows_the_error_row(cx: &mut TestAppContext) {
        let (_state, _search, cx) = rooted_search(cx, true);

        cx.simulate_input("colour:red");
        cx.run_until_parked();

        assert!(cx.debug_bounds("search.error").is_some());
    }

    #[gpui::test]
    fn valid_query_hides_the_error_row(cx: &mut TestAppContext) {
        let (_state, _search, cx) = rooted_search(cx, true);

        cx.simulate_input("web");
        cx.run_until_parked();

        assert!(cx.debug_bounds("search.error").is_none());
    }

    #[gpui::test]
    fn typing_a_field_prefix_opens_the_suggestions(cx: &mut TestAppContext) {
        let (_state, _search, cx) = rooted_search_with(cx, true, three_state_view_model());

        cx.simulate_input("st");
        cx.run_until_parked();

        assert!(cx.debug_bounds("search.suggestions").is_some());
        assert!(cx.debug_bounds("search.suggestion.0").is_some());
    }

    #[gpui::test]
    fn enter_accepts_the_highlighted_field(cx: &mut TestAppContext) {
        let (state, _search, cx) = rooted_search_with(cx, true, three_state_view_model());

        cx.simulate_input("st");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert_eq!(
            state.read_with(cx, |state, _| state.query().to_string()),
            "state:"
        );
    }

    #[gpui::test]
    fn down_and_tab_accept_the_second_state_value(cx: &mut TestAppContext) {
        let (state, _search, cx) = rooted_search_with(cx, true, three_state_view_model());

        cx.simulate_input("state:");
        cx.run_until_parked();
        assert!(cx.debug_bounds("search.suggestion.2").is_some());

        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();

        assert_eq!(
            state.read_with(cx, |state, _| state.query().to_string()),
            "state:stopped "
        );
    }

    #[gpui::test]
    fn down_wraps_around_to_the_first_suggestion(cx: &mut TestAppContext) {
        let (state, _search, cx) = rooted_search_with(cx, true, three_state_view_model());

        cx.simulate_input("state:");
        cx.run_until_parked();
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("down");
        cx.simulate_keystrokes("tab");
        cx.run_until_parked();

        assert_eq!(
            state.read_with(cx, |state, _| state.query().to_string()),
            "state:running "
        );
    }

    #[gpui::test]
    fn unknown_field_shows_the_warning_row(cx: &mut TestAppContext) {
        let (_state, _search, cx) = rooted_search_with(cx, true, three_state_view_model());

        cx.simulate_input("colour:r");
        cx.run_until_parked();

        assert!(cx.debug_bounds("search.unknown").is_some());
    }

    #[gpui::test]
    fn empty_query_shows_no_suggestions(cx: &mut TestAppContext) {
        // gpui 0.2.2 keeps debug_bounds across frames; this window never showed the list.
        let (state, _search, cx) = rooted_search_with(cx, true, three_state_view_model());
        cx.run_until_parked();

        assert_eq!(
            state.read_with(cx, |state, _| state.query().to_string()),
            ""
        );
        assert!(cx.debug_bounds("search.suggestions").is_none());
    }

    fn owner_view_model() -> InstanceListViewModel {
        let mut tagged = instance("i-1", "api-server", "running");
        tagged.tags.push(EC2InstanceTag {
            key: "Owner".to_string(),
            value: "platform".to_string(),
        });
        let mut vm =
            InstanceListViewModel::new(Box::new(MockComputeService::new(vec![tagged], None)));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        rt.block_on(vm.load_instances());
        vm
    }

    #[gpui::test]
    fn focused_empty_field_lists_the_newest_recent_searches(cx: &mut TestAppContext) {
        let (state, _search, cx) = rooted_search_with(cx, true, three_state_view_model());
        state.update(cx, |state, cx| {
            state.recent_searches = ["a", "b", "c", "d", "e"].map(String::from).to_vec();
            cx.notify();
        });
        cx.run_until_parked();

        assert!(cx.debug_bounds("search.recent").is_some());
        assert!(cx.debug_bounds("search.recent.3").is_some());
        assert!(cx.debug_bounds("search.recent.4").is_none());
    }

    #[gpui::test]
    fn clicking_a_recent_search_sets_the_query(cx: &mut TestAppContext) {
        let (state, _search, cx) = rooted_search_with(cx, true, three_state_view_model());
        state.update(cx, |state, cx| {
            state.recent_searches = ["a", "b", "c", "d", "e"].map(String::from).to_vec();
            cx.notify();
        });
        cx.run_until_parked();

        let row = cx
            .debug_bounds("search.recent.1")
            .expect("second recent search renders");
        cx.simulate_click(row.center(), Default::default());
        cx.run_until_parked();

        assert_eq!(
            state.read_with(cx, |state, _| state.query().to_string()),
            "b"
        );
    }

    #[gpui::test]
    fn enter_with_no_dropdown_records_the_query(cx: &mut TestAppContext) {
        let (state, _search, cx) = rooted_search_with(cx, true, owner_view_model());

        cx.simulate_input("owner:esteban");
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();

        assert_eq!(
            state.read_with(cx, |state, _| state.recent_searches.first().cloned()),
            Some("owner:esteban".to_string())
        );
    }
}
