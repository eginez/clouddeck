//! Query search field with highlighting and autocomplete (design/images/21-27).

use clouddeck_core::{EscapeAction, EscapeContext, escape_action};
use gpui::{
    App, Context, Entity, Focusable, IntoElement, KeyBinding, Render, Window, WindowAppearance,
    actions, div, prelude::*, px,
};
use gpui_component::Sizable;
use gpui_component::input::{Input, InputEvent, InputState};

use crate::fleet_state::FleetState;
use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, Palette, STATUS_FONT_SIZE};

const SEARCH_WIDTH: f32 = 220.0;
const SEARCH_HEIGHT: f32 = 28.0;
const SEARCH_RADIUS: f32 = 6.0;
const SIDE_PADDING: f32 = 7.0;
const GAP: f32 = 6.0;

actions!(search_field, [FocusSearch, ClearSearch]);

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
}

impl SearchField {
    pub fn new(state: Entity<FleetState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search or filter"));

        cx.subscribe(&input, |this, input, event: &InputEvent, cx| match event {
            InputEvent::Change => {
                let text = input.read(cx).value().to_string();
                this.state
                    .update(cx, |state, cx| state.set_query(&text, cx));
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

        Self { state, input }
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
        let focused = self.input.read(cx).focus_handle(cx).is_focused(window);

        div()
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
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(Input::new(&self.input).appearance(false).bordered(false)),
            )
            .when(!focused, |field| {
                field.child(
                    div()
                        .debug_selector(|| "search.hint".into())
                        .text_size(px(STATUS_FONT_SIZE))
                        .child("⌘F"),
                )
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
}
