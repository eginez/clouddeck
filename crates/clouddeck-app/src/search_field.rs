//! Query search field with highlighting and autocomplete (design/images/21-27).

use std::ops::Range;
use std::time::SystemTime;

use clouddeck_core::{
    EscapeAction, EscapeContext, TermKind, compile, escape_action, known_tag_keys,
};
use gpui::{
    App, Context, Entity, Focusable, Font, Hsla, IntoElement, KeyBinding, Render, StyledText,
    TextRun, UnderlineStyle, Window, WindowAppearance, actions, div, prelude::*, px, rems,
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
const PLACEHOLDER: &str = "Search or filter";

actions!(search_field, [FocusSearch, ClearSearch]);

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

pub fn search_key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-f", FocusSearch, None),
        KeyBinding::new("/", FocusSearch, Some("InstanceTable")),
        KeyBinding::new("escape", ClearSearch, Some("SearchField")),
    ]
}

pub struct SearchField {
    state: Entity<FleetState>,
    pub input: Entity<InputState>,
    scroll_x: f32,
}

impl SearchField {
    pub fn new(state: Entity<FleetState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));

        cx.subscribe(&input, |this, input, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                let text = input.read(cx).value().to_string();
                this.state
                    .update(cx, |state, cx| state.set_query(&text, cx));
                cx.notify();
            }
            InputEvent::Focus | InputEvent::Blur => cx.notify(),
            _ => {}
        })
        .detach();

        cx.observe_in(&state, window, |this, state, window, cx| {
            let query = state.read(cx).query().to_string();
            if this.input.read(cx).value().as_ref() != query.as_str() {
                this.input
                    .update(cx, |input, cx| input.set_value(query, window, cx));
            }
            cx.notify();
        })
        .detach();

        let window_handle = window.window_handle();
        let input_weak = input.downgrade();
        App::on_action(cx, move |_: &FocusSearch, cx| {
            let input_weak = input_weak.clone();
            cx.defer(move |cx| {
                let _ = window_handle.update(cx, |_, window, cx| {
                    let _ = input_weak.update(cx, |input, cx| input.focus(window, cx));
                });
            });
        });

        Self {
            state,
            input,
            scroll_x: 0.0,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }

    fn clear(&mut self, _: &ClearSearch, window: &mut Window, cx: &mut Context<Self>) {
        let query_empty = self.state.read(cx).query().is_empty();
        let action = escape_action(&EscapeContext {
            search_focused: true,
            query_empty,
            ..Default::default()
        });
        if action != EscapeAction::ClearSearch {
            cx.propagate();
            return;
        }
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.state.update(cx, |state, cx| state.set_query("", cx));
    }
}

impl Render for SearchField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = matches!(
            window.appearance(),
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        );
        let palette = Palette::for_dark(dark);
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
            .on_action(cx.listener(Self::clear))
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

        div()
            .relative()
            .w(px(SEARCH_WIDTH))
            .child(field)
            .when_some(error, |outer, message| {
                outer.child(error_row(&message, &palette))
            })
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use clouddeck_core::{EC2Instance, EC2InstanceTag, MockComputeService};
    use gpui::{Entity, Focusable, TestAppContext, VisualTestContext};

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

    fn rooted_search(
        cx: &mut TestAppContext,
        focus_on_open: bool,
    ) -> (
        Entity<FleetState>,
        Entity<SearchField>,
        &mut VisualTestContext,
    ) {
        let state = cx.new(|_| FleetState::new(loaded_view_model(), "default".to_string(), true));
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
    fn cmd_f_focuses_the_input_and_escape_clears_the_query(cx: &mut TestAppContext) {
        let (state, search, cx) = rooted_search(cx, false);

        cx.simulate_keystrokes("cmd-f");
        let input = search.read_with(cx, |search, _| search.input.clone());
        let focused = cx.update(|window, cx| input.read(cx).focus_handle(cx).is_focused(window));
        assert!(focused, "cmd-f should focus the search input");

        cx.simulate_input("state:stopped");
        cx.run_until_parked();
        assert_eq!(
            state.read_with(cx, |state, _| state.query().to_string()),
            "state:stopped"
        );

        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        assert_eq!(
            state.read_with(cx, |state, _| state.query().to_string()),
            ""
        );
        assert_eq!(input_text(&search, cx), "");
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
}
