//! First-run welcome window (design/images/60).

use clouddeck_core::{
    AwsProfile, ProfileInput, ProfileRow, WorkspaceContext, context_path, filter_profile_rows,
    load_profiles, move_highlight, profile_picker_empty_text, profile_rows,
};
use gpui::{
    App, Bounds, ClickEvent, Context, Entity, EventEmitter, FontWeight, Render, ScrollHandle,
    SharedString, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, div, point,
    prelude::*, px, size, white,
};
use gpui_component::input::{Enter, Input, InputEvent, InputState, MoveDown, MoveUp};
use gpui_component::{Root, Sizable, Size};

use crate::icons::AppIcon;
use crate::shell;
use crate::tokens::{FONT_SIZE, Palette, palette};

const WINDOW_WIDTH: f32 = 800.0;
const WINDOW_HEIGHT: f32 = 460.0;
const SIDE_WIDTH: f32 = 260.0;
const SIDE_PADDING: f32 = 24.0;
const CLOUD_ICON_SIZE: f32 = 64.0;
const TITLE_FONT_SIZE: f32 = 28.0;
const NAME_FONT_SIZE: f32 = 13.0;
const SUBTITLE_FONT_SIZE: f32 = 11.0;
const BUTTON_HEIGHT: f32 = 44.0;
const BUTTON_RADIUS: f32 = 8.0;
const BADGE_SIZE: f32 = 34.0;
const BADGE_FONT_SIZE: f32 = 11.0;
const ROW_RADIUS: f32 = 8.0;
const DEFAULT_REGION: &str = "us-east-1";
const NO_PROFILES_TEXT: &str = "No AWS profiles found in ~/.aws/config.";
const FOOTER_TEXT: &str = "Profiles are read from ~/.aws/config. Press ↵ to open the selected profile. Credentials stay with the AWS CLI.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WelcomeEvent {
    OpenProfile(String),
    ConnectToAws,
    SignInWithSso,
    ExploreDemo,
}

pub struct WelcomeWindow {
    pub profiles: Vec<ProfileInput>,
    filter: Entity<InputState>,
    highlighted: usize,
    scroll: ScrollHandle,
    message: Option<String>,
}

impl EventEmitter<WelcomeEvent> for WelcomeWindow {}

/// Whether the welcome window replaces the main window at launch.
pub fn should_show_welcome(demo: bool, saved: Option<&WorkspaceContext>) -> bool {
    !demo && saved.is_none()
}

fn profile_inputs(profiles: Vec<AwsProfile>) -> Vec<ProfileInput> {
    profiles
        .into_iter()
        .map(|profile| ProfileInput {
            name: profile.name,
            account_id: None,
            region: profile.region,
        })
        .collect()
}

fn context_for_profile(profiles: &[ProfileInput], name: &str) -> WorkspaceContext {
    let region = profiles
        .iter()
        .find(|profile| profile.name == name)
        .and_then(|profile| profile.region.clone())
        .unwrap_or_else(|| DEFAULT_REGION.into());
    WorkspaceContext {
        profile: name.into(),
        region,
    }
}

impl WelcomeWindow {
    pub fn new(profiles: Vec<ProfileInput>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search profiles…"));
        cx.subscribe(&filter, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.highlighted = 0;
                this.scroll.scroll_to_item(0);
                cx.notify();
            }
        })
        .detach();
        cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.reload_profiles(cx);
            }
        })
        .detach();
        filter.update(cx, |input, cx| input.focus(window, cx));
        Self {
            profiles,
            filter,
            highlighted: 0,
            scroll: ScrollHandle::new(),
            message: None,
        }
    }

    fn visible_rows(&self, cx: &App) -> Vec<ProfileRow> {
        filter_profile_rows(&profile_rows(&self.profiles), &self.filter.read(cx).value())
    }

    fn reload_profiles(&mut self, cx: &mut Context<Self>) {
        match load_profiles() {
            Ok(profiles) => {
                self.profiles = profile_inputs(profiles);
                self.message = None;
                let len = self.visible_rows(cx).len();
                self.highlighted = self.highlighted.min(len.saturating_sub(1));
            }
            Err(error) => self.message = Some(format!("Could not read AWS profiles: {error}")),
        }
        cx.notify();
    }

    fn step_highlight(&mut self, down: bool, cx: &mut Context<Self>) {
        let len = self.visible_rows(cx).len();
        self.highlighted = move_highlight(self.highlighted, len, down);
        self.scroll.scroll_to_item(self.highlighted);
        cx.notify();
    }

    fn open_highlighted(&mut self, cx: &mut Context<Self>) {
        let name = self
            .visible_rows(cx)
            .get(self.highlighted)
            .map(|row| row.name.clone());
        if let Some(name) = name {
            cx.emit(WelcomeEvent::OpenProfile(name));
        }
    }
}

fn action_button(
    selector: &'static str,
    icon: AppIcon,
    label: &'static str,
    event: WelcomeEvent,
    palette: &Palette,
    cx: &Context<WelcomeWindow>,
) -> impl IntoElement {
    div()
        .id(selector)
        .debug_selector(move || selector.into())
        .w_full()
        .flex()
        .items_center()
        .gap(px(10.0))
        .h(px(BUTTON_HEIGHT))
        .px(px(12.0))
        .rounded(px(BUTTON_RADIUS))
        .bg(palette.chrome)
        .text_size(px(NAME_FONT_SIZE))
        .text_color(palette.text)
        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.emit(event.clone());
        }))
        .child(icon.icon().text_color(palette.dim))
        .child(label)
}

fn profile_row(
    ix: usize,
    row: &ProfileRow,
    highlighted: bool,
    palette: &Palette,
    cx: &Context<WelcomeWindow>,
) -> impl IntoElement {
    let name = row.name.clone();
    div()
        .id(SharedString::from(format!("welcome.profile.{ix}")))
        .debug_selector(move || format!("welcome.profile.{ix}"))
        .flex()
        .items_center()
        .gap(px(12.0))
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(ROW_RADIUS))
        .when(highlighted, |item| item.bg(palette.sel))
        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
            this.highlighted = ix;
            if event.click_count() >= 2 {
                cx.emit(WelcomeEvent::OpenProfile(name.clone()));
            }
            cx.notify();
        }))
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .w(px(BADGE_SIZE))
                .h(px(BADGE_SIZE))
                .rounded_full()
                .bg(palette.badge)
                .text_color(white())
                .text_size(px(BADGE_FONT_SIZE))
                .font_weight(FontWeight::BOLD)
                .child(row.badge.clone()),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(NAME_FONT_SIZE))
                        .text_color(palette.runtext)
                        .child(row.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(SUBTITLE_FONT_SIZE))
                        .text_color(palette.dim)
                        .child(row.subtitle.clone()),
                ),
        )
}

impl Render for WelcomeWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette(cx);
        let visible = self.visible_rows(cx);
        let filter_text = self.filter.read(cx).value();
        let highlighted = self.highlighted;

        let body = if visible.is_empty() {
            let text = if self.profiles.is_empty() {
                NO_PROFILES_TEXT.to_string()
            } else {
                profile_picker_empty_text(&filter_text)
            };
            div()
                .debug_selector(|| "welcome.empty".into())
                .flex_1()
                .px(px(8.0))
                .py(px(12.0))
                .text_size(px(FONT_SIZE))
                .text_color(palette.dim)
                .child(text)
                .into_any_element()
        } else {
            div()
                .id("welcome.list")
                .track_scroll(&self.scroll)
                .overflow_y_scroll()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .children(
                    visible
                        .iter()
                        .enumerate()
                        .map(|(ix, row)| profile_row(ix, row, ix == highlighted, &palette, cx)),
                )
                .into_any_element()
        };

        let add = div()
            .id("welcome.add")
            .debug_selector(|| "welcome.add".into())
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .w(px(24.0))
            .h(px(32.0))
            .rounded(px(BUTTON_RADIUS))
            .text_color(palette.dim)
            .hover(|button| button.bg(palette.hover))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.emit(WelcomeEvent::ConnectToAws);
            }))
            .child(AppIcon::Plus.icon().text_color(palette.dim));

        let search = div()
            .debug_selector(|| "welcome.searchbox".into())
            .flex_1()
            .flex()
            .items_center()
            .gap(px(8.0))
            .h(px(32.0))
            .px(px(10.0))
            .rounded(px(BUTTON_RADIUS))
            .bg(palette.surf)
            .border_1()
            .border_color(palette.line)
            .child(AppIcon::Search.icon().text_color(palette.faint))
            .child(
                div()
                    .debug_selector(|| "welcome.search".into())
                    .flex_1()
                    .child(Input::new(&self.filter).appearance(false)),
            );

        let search_row = div()
            .debug_selector(|| "welcome.searchrow".into())
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(add)
            .child(search);

        let footer = div()
            .text_size(px(SUBTITLE_FONT_SIZE))
            .text_color(palette.dim)
            .child(FOOTER_TEXT);

        let right = div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .h_full()
            .p(px(16.0))
            .gap(px(12.0))
            .child(search_row)
            .child(body)
            .children(self.message.clone().map(|message| {
                div()
                    .text_size(px(SUBTITLE_FONT_SIZE))
                    .text_color(palette.err)
                    .child(message)
            }))
            .child(footer);

        let side = div()
            .flex()
            .flex_col()
            .items_center()
            .flex_none()
            .w(px(SIDE_WIDTH))
            .h_full()
            .p(px(SIDE_PADDING))
            .gap(px(8.0))
            .bg(palette.side)
            .child(
                div().mt(px(40.0)).child(
                    AppIcon::Cloud
                        .icon()
                        .with_size(Size::Size(px(CLOUD_ICON_SIZE)))
                        .text_color(palette.accent),
                ),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .text_size(px(TITLE_FONT_SIZE))
                    .font_weight(FontWeight::BOLD)
                    .text_color(palette.text)
                    .child("CloudDeck"),
            )
            .child(
                div()
                    .text_size(px(SUBTITLE_FONT_SIZE))
                    .text_color(palette.dim)
                    .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
            )
            .child(div().flex_1())
            .child(action_button(
                "welcome.connect",
                AppIcon::Plug,
                "Connect to AWS…",
                WelcomeEvent::ConnectToAws,
                &palette,
                cx,
            ))
            .child(action_button(
                "welcome.sso",
                AppIcon::Key,
                "Sign in with SSO…",
                WelcomeEvent::SignInWithSso,
                &palette,
                cx,
            ))
            .child(action_button(
                "welcome.demo",
                AppIcon::Server,
                "Explore demo fleet",
                WelcomeEvent::ExploreDemo,
                &palette,
                cx,
            ));

        div()
            .key_context("Welcome")
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                this.step_highlight(false, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                this.step_highlight(true, cx);
                cx.stop_propagation();
            }))
            .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                this.open_highlighted(cx);
                cx.stop_propagation();
            }))
            .size_full()
            .flex()
            .bg(palette.win)
            .child(side)
            .child(right)
    }
}

fn route(
    welcome: WindowHandle<Root>,
    view: &Entity<WelcomeWindow>,
    event: &WelcomeEvent,
    cx: &mut App,
) {
    let opened = match event {
        WelcomeEvent::OpenProfile(name) => {
            let context = context_for_profile(&view.read(cx).profiles, name);
            if let Err(error) = context.save(&context_path()) {
                view.update(cx, |view, cx| {
                    view.message = Some(format!("Could not remember this profile: {error}"));
                    cx.notify();
                });
                return;
            }
            shell::open_main_window(cx, false, shell::DemoScenario::Happy)
        }
        WelcomeEvent::ConnectToAws | WelcomeEvent::SignInWithSso => {
            shell::open_main_window_with_setup(cx)
        }
        WelcomeEvent::ExploreDemo => shell::open_main_window(cx, true, shell::DemoScenario::Happy),
    };
    if opened.is_some() {
        let _ = welcome.update(cx, |_, window, _| window.remove_window());
    }
}

pub fn open_welcome_window(cx: &mut App) -> Option<WindowHandle<Root>> {
    open_welcome_window_sized(cx, None)
}

/// Opens the welcome window, at `size_override` (content size) when given.
pub fn open_welcome_window_sized(
    cx: &mut App,
    size_override: Option<(f32, f32)>,
) -> Option<WindowHandle<Root>> {
    let (profiles, message) = match load_profiles() {
        Ok(profiles) => (profile_inputs(profiles), None),
        Err(error) => (
            vec![],
            Some(format!("Could not read AWS profiles: {error}")),
        ),
    };
    let (width, height) = size_override.unwrap_or((WINDOW_WIDTH, WINDOW_HEIGHT));
    let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
    let handle = cx
        .open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("CloudDeck")),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(15.0))),
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some(crate::main_window::APP_ID.to_string()),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| {
                    let mut welcome = WelcomeWindow::new(profiles, window, cx);
                    welcome.message = message;
                    welcome
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .ok()?;
    let view = handle
        .update(cx, |root, _, _| {
            root.view().clone().downcast::<WelcomeWindow>().ok()
        })
        .ok()
        .flatten()?;
    cx.subscribe(&view, move |view, event: &WelcomeEvent, cx| {
        route(handle, &view, event, cx)
    })
    .detach();
    Some(handle)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use gpui::{TestAppContext, VisualTestContext};

    type Events = Rc<RefCell<Vec<WelcomeEvent>>>;

    fn profiles() -> Vec<ProfileInput> {
        vec![
            ProfileInput {
                name: "prod-readonly".into(),
                account_id: None,
                region: Some("us-east-1".into()),
            },
            ProfileInput {
                name: "staging-admin".into(),
                account_id: None,
                region: Some("us-west-2".into()),
            },
            ProfileInput {
                name: "sandbox".into(),
                account_id: None,
                region: Some("eu-west-1".into()),
            },
        ]
    }

    /// Opens the welcome window with `query` already in the filter so the first drawn frame
    /// is the filtered one: gpui 0.2.2 never clears `debug_bounds` between frames.
    fn open_welcome<'a>(
        cx: &'a mut TestAppContext,
        profiles: Vec<ProfileInput>,
        query: &'static str,
    ) -> (Entity<WelcomeWindow>, Events, &'a mut VisualTestContext) {
        let slot = Rc::new(RefCell::new(None));
        let slot_for_window = slot.clone();
        let (_root, cx) = cx.add_window_view(move |window, cx| {
            gpui_component::init(cx);
            let welcome = cx.new(|cx| WelcomeWindow::new(profiles, window, cx));
            welcome.update(cx, |welcome, cx| {
                welcome
                    .filter
                    .update(cx, |input, cx| input.set_value(query, window, cx));
            });
            *slot_for_window.borrow_mut() = Some(welcome.clone());
            Root::new(welcome, window, cx)
        });
        let welcome = slot
            .borrow()
            .as_ref()
            .expect("welcome entity should be created")
            .clone();
        let events: Events = Rc::new(RefCell::new(Vec::new()));
        let sink = events.clone();
        cx.update(|_, cx| {
            cx.subscribe(&welcome, move |_, event: &WelcomeEvent, _| {
                sink.borrow_mut().push(event.clone())
            })
            .detach();
        });
        (welcome, events, cx)
    }

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should be rendered"));
        cx.simulate_click(bounds.center(), Default::default());
        cx.run_until_parked();
    }

    #[gpui::test]
    fn renders_the_actions_and_the_profile_rows(cx: &mut TestAppContext) {
        let (_, _, cx) = open_welcome(cx, profiles(), "");

        assert!(cx.debug_bounds("welcome.connect").is_some());
        assert!(cx.debug_bounds("welcome.sso").is_some());
        assert!(cx.debug_bounds("welcome.demo").is_some());
        assert!(cx.debug_bounds("welcome.search").is_some());
        assert!(cx.debug_bounds("welcome.profile.0").is_some());
        assert!(cx.debug_bounds("welcome.profile.2").is_some());
        assert!(cx.debug_bounds("welcome.profile.3").is_none());
        assert!(cx.debug_bounds("welcome.empty").is_none());
    }

    #[gpui::test]
    fn a_filter_keeps_only_matching_profiles(cx: &mut TestAppContext) {
        let (_, _, cx) = open_welcome(cx, profiles(), "stag");

        assert!(cx.debug_bounds("welcome.profile.0").is_some());
        assert!(cx.debug_bounds("welcome.profile.1").is_none());
    }

    #[gpui::test]
    fn no_profiles_renders_the_empty_state(cx: &mut TestAppContext) {
        let (_, _, cx) = open_welcome(cx, vec![], "");

        assert!(cx.debug_bounds("welcome.empty").is_some());
        assert!(cx.debug_bounds("welcome.profile.0").is_none());
    }

    #[gpui::test]
    fn a_filter_matching_nothing_renders_the_empty_state(cx: &mut TestAppContext) {
        let (_, _, cx) = open_welcome(cx, profiles(), "zz");

        assert!(cx.debug_bounds("welcome.empty").is_some());
        assert!(cx.debug_bounds("welcome.profile.0").is_none());
    }

    #[gpui::test]
    fn down_then_enter_opens_the_second_profile(cx: &mut TestAppContext) {
        let (_, events, cx) = open_welcome(cx, profiles(), "");

        cx.simulate_keystrokes("down enter");

        assert_eq!(
            *events.borrow(),
            vec![WelcomeEvent::OpenProfile("staging-admin".into())]
        );
    }

    #[gpui::test]
    fn enter_with_no_profiles_opens_nothing(cx: &mut TestAppContext) {
        let (_, events, cx) = open_welcome(cx, vec![], "");

        cx.simulate_keystrokes("enter");

        assert!(events.borrow().is_empty());
    }

    #[gpui::test]
    fn clicking_a_row_highlights_it_without_opening(cx: &mut TestAppContext) {
        let (welcome, events, cx) = open_welcome(cx, profiles(), "");

        click(cx, "welcome.profile.1");

        assert_eq!(welcome.read_with(cx, |welcome, _| welcome.highlighted), 1);
        assert!(events.borrow().is_empty());
    }

    #[gpui::test]
    fn clicking_the_demo_button_explores_the_demo(cx: &mut TestAppContext) {
        let (_, events, cx) = open_welcome(cx, profiles(), "");

        click(cx, "welcome.demo");

        assert_eq!(*events.borrow(), vec![WelcomeEvent::ExploreDemo]);
    }

    #[gpui::test]
    fn clicking_the_add_button_opens_the_connect_flow(cx: &mut TestAppContext) {
        let (_, events, cx) = open_welcome(cx, profiles(), "");

        click(cx, "welcome.add");

        assert_eq!(*events.borrow(), vec![WelcomeEvent::ConnectToAws]);
    }

    #[gpui::test]
    fn clicking_connect_and_sso_emit_their_events(cx: &mut TestAppContext) {
        let (_, events, cx) = open_welcome(cx, profiles(), "");

        click(cx, "welcome.connect");
        click(cx, "welcome.sso");

        assert_eq!(
            *events.borrow(),
            vec![WelcomeEvent::ConnectToAws, WelcomeEvent::SignInWithSso]
        );
    }

    #[test]
    fn welcome_shows_only_on_a_first_real_run() {
        let saved = WorkspaceContext {
            profile: "prod".into(),
            region: "us-east-1".into(),
        };
        assert!(should_show_welcome(false, None));
        assert!(!should_show_welcome(true, None));
        assert!(!should_show_welcome(false, Some(&saved)));
    }

    #[test]
    fn opening_a_profile_uses_its_region_or_the_default() {
        let inputs = vec![
            ProfileInput {
                name: "prod".into(),
                account_id: None,
                region: Some("eu-west-1".into()),
            },
            ProfileInput {
                name: "bare".into(),
                account_id: None,
                region: None,
            },
        ];

        assert_eq!(
            context_for_profile(&inputs, "prod"),
            WorkspaceContext {
                profile: "prod".into(),
                region: "eu-west-1".into(),
            }
        );
        assert_eq!(context_for_profile(&inputs, "bare").region, DEFAULT_REGION);
    }
}
