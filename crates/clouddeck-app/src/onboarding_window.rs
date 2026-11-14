//! The onboarding window: the first-run screen for every provider. Connections, sign-in
//! methods and onboarding text all come from the provider API; this module only draws them.

use std::sync::Arc;
use std::time::SystemTime;

use clouddeck_core::{
    AuthMethod, AuthStatus, Call, ConnectError, Connection, ConnectionId, Provider, ProviderError,
    Selection, SignInAction, connect_methods, submit,
};
use gpui::{
    AnyElement, App, Bounds, ClickEvent, ClipboardItem, Context, Entity, EventEmitter, FontWeight,
    Hsla, Render, SharedString, Subscription, TitlebarOptions, Window, WindowBounds, WindowHandle,
    WindowOptions, div, point, prelude::*, px, size,
};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{Root, Sizable};

use crate::app_providers;
use crate::provider_call::{off_thread, with_call};
use crate::provider_sign_in::{Progress, drive};
use crate::setting_rows::link;
use crate::tokens::{Palette, palette};

pub(crate) const WINDOW_SIZE: (f32, f32) = (1000.0, 600.0);
const RAIL_WIDTH: f32 = 260.0;
const FOOTER_HEIGHT: f32 = 52.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnboardingEvent {
    Open {
        provider: String,
        connection: ConnectionId,
    },
    SignedIn {
        provider: String,
    },
    ExploreDemo,
}

#[derive(Debug, Clone)]
enum Listing {
    Detecting,
    Found(Vec<Row>),
    Empty,
    Failed(ProviderError),
}

#[derive(Debug, Clone)]
struct Row {
    connection: Connection,
    status: Option<AuthStatus>,
    identity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    Idle,
    Signing,
    Waiting { code: String, url: String },
    Opening { name: String },
}

pub struct OnboardingWindow {
    providers: Vec<Arc<dyn Provider>>,
    chosen: usize,
    listings: Vec<Listing>,
    generations: Vec<u64>,
    selected: Option<ConnectionId>,
    search: Entity<InputState>,
    query: String,
    phase: Phase,
    error: Option<ConnectError>,
    attempt: u64,
    subscriptions: Vec<Subscription>,
}

impl EventEmitter<OnboardingEvent> for OnboardingWindow {}

fn list(provider: &dyn Provider, call: &Call<'_>) -> Listing {
    match provider.connections(call) {
        Err(error) => Listing::Failed(error),
        Ok(connections) if connections.is_empty() => Listing::Empty,
        Ok(connections) => {
            let mut rows: Vec<Row> = connections
                .into_iter()
                .map(|connection| {
                    let status = provider
                        .auth()
                        .map(|auth| auth.auth_status(call, Some(&connection.id)));
                    Row {
                        connection,
                        status,
                        identity: None,
                    }
                })
                .collect();
            if let (Some(auth), Some(row)) = (
                provider.auth(),
                rows.iter_mut()
                    .find(|row| matches!(row.status, Some(AuthStatus::SignedIn { .. }))),
            ) {
                row.identity = auth
                    .identity(call, &row.connection.id, &Selection::new())
                    .ok();
            }
            Listing::Found(rows)
        }
    }
}

impl OnboardingWindow {
    fn new(
        providers: Vec<Arc<dyn Provider>>,
        provider_id: Option<&str>,
        connection: Option<ConnectionId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let chosen = provider_id
            .and_then(|id| providers.iter().position(|provider| provider.info().id == id))
            .unwrap_or(0);
        let count = providers.len();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let mut this = Self {
            providers,
            chosen,
            listings: vec![Listing::Detecting; count],
            generations: vec![0; count],
            selected: connection,
            search: search.clone(),
            query: String::new(),
            phase: Phase::Idle,
            error: None,
            attempt: 0,
            subscriptions: Vec::new(),
        };
        this.subscriptions.push(cx.subscribe_in(
            &search,
            window,
            |this: &mut Self, state, event: &InputEvent, _, cx| match event {
                InputEvent::Change => {
                    this.query = state.read(cx).value().to_string();
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => this.open_selected(cx),
                _ => {}
            },
        ));
        for index in 0..count {
            this.detect(index, cx);
        }
        this
    }

    fn detect(&mut self, index: usize, cx: &mut Context<Self>) {
        self.generations[index] += 1;
        let generation = self.generations[index];
        self.listings[index] = Listing::Detecting;
        let provider = self.providers[index].clone();
        cx.spawn(async move |this, cx| {
            let listing = off_thread(cx, move || with_call(|call| list(provider.as_ref(), call)))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generations[index] == generation {
                    this.listings[index] = listing;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn choose(&mut self, index: usize, cx: &mut Context<Self>) {
        if index != self.chosen && index < self.providers.len() {
            if self.phase == Phase::Signing || matches!(self.phase, Phase::Waiting { .. }) {
                self.cancel_sign_in(cx);
            }
            self.chosen = index;
            self.selected = None;
            self.error = None;
            cx.notify();
        }
    }

    fn rows(&self) -> &[Row] {
        match &self.listings[self.chosen] {
            Listing::Found(rows) => rows,
            _ => &[],
        }
    }

    fn selected_row(&self) -> Option<&Row> {
        let rows = self.rows();
        self.selected
            .as_ref()
            .and_then(|id| rows.iter().find(|row| row.connection.id == *id))
            .or_else(|| rows.first())
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        if self.phase != Phase::Idle {
            return;
        }
        let Some(row) = self.selected_row() else {
            return;
        };
        let name = row.connection.display_name.clone();
        let connection = row.connection.id.clone();
        let provider = self.providers[self.chosen].info().id;
        self.phase = Phase::Opening { name };
        cx.emit(OnboardingEvent::Open {
            provider,
            connection,
        });
        cx.notify();
    }

    fn back_to_onboarding(&mut self, cx: &mut Context<Self>) {
        self.phase = Phase::Idle;
        cx.notify();
    }

    fn sign_in(&mut self, cx: &mut Context<Self>) {
        if self.phase != Phase::Idle {
            return;
        }
        let provider = self.providers[self.chosen].clone();
        let Some(method) = connect_methods(provider.as_ref())
            .into_iter()
            .find(|method| method.header_button.is_some())
        else {
            return;
        };
        let connection = self.selected_row().map(|row| row.connection.id.clone());

        self.attempt += 1;
        let attempt = self.attempt;
        self.phase = Phase::Signing;
        self.error = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let first = off_thread(cx, {
                let provider = provider.clone();
                move || {
                    with_call(|call| {
                        submit(
                            provider.as_ref(),
                            call,
                            &method,
                            connection,
                            Vec::new(),
                            |_, _| Ok(()),
                        )
                    })
                }
            })
            .await;
            let step = match first {
                Ok(step) => step,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| this.fail(attempt, error, cx));
                    return;
                }
            };
            drive(
                provider,
                step,
                move |progress, cx| {
                    this.update(cx, |this, cx| this.progress(attempt, progress, cx))
                        .unwrap_or(false)
                },
                cx,
            )
            .await;
        })
        .detach();
    }

    fn progress(&mut self, attempt: u64, progress: Progress, cx: &mut Context<Self>) -> bool {
        if attempt != self.attempt {
            return false;
        }
        match progress {
            Progress::Action(SignInAction::Finish) => {
                self.phase = Phase::Idle;
                let provider = self.providers[self.chosen].info().id;
                cx.emit(OnboardingEvent::SignedIn { provider });
            }
            Progress::Action(SignInAction::OpenBrowser { code, url, .. }) => {
                self.phase = Phase::Waiting {
                    code: code.unwrap_or_default(),
                    url,
                };
            }
            Progress::Action(SignInAction::NeedsInput { .. }) => self.phase = Phase::Idle,
            Progress::Failed(error) => {
                self.phase = Phase::Idle;
                self.error = Some(error.into());
            }
        }
        cx.notify();
        true
    }

    fn fail(&mut self, attempt: u64, error: ConnectError, cx: &mut Context<Self>) {
        if attempt == self.attempt {
            self.phase = Phase::Idle;
            self.error = Some(error);
            cx.notify();
        }
    }

    fn cancel_sign_in(&mut self, cx: &mut Context<Self>) {
        self.attempt += 1;
        self.phase = Phase::Idle;
        cx.notify();
    }

    fn header_method(&self) -> Option<AuthMethod> {
        connect_methods(self.providers[self.chosen].as_ref())
            .into_iter()
            .find(|method| method.header_button.is_some())
    }

    fn noun(&self) -> String {
        self.providers[self.chosen].info().connection_label.to_lowercase()
    }

    fn rail(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let rows = self
            .providers
            .iter()
            .enumerate()
            .map(|(index, provider)| self.provider_row(index, provider, palette, cx));
        div()
            .debug_selector(|| "onboarding.rail".to_string())
            .w(px(RAIL_WIDTH))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .justify_between()
            .bg(palette.side)
            .border_r_1()
            .border_color(palette.line2)
            .pt(px(18.))
            .px(px(12.))
            .pb(px(14.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(
                        div()
                            .px(px(8.))
                            .child(
                                div()
                                    .text_size(px(20.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(palette.text)
                                    .child("CloudDeck"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(palette.dim)
                                    .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                            ),
                    )
                    .child(
                        div()
                            .px(px(8.))
                            .text_size(px(10.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.faint)
                            .child("PROVIDERS"),
                    )
                    .child(div().flex().flex_col().gap(px(2.)).children(rows)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .h(px(54.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.))
                            .border_1()
                            .border_color(palette.line)
                            .text_size(px(13.))
                            .text_color(palette.dim)
                            .child("+ Add provider…"),
                    )
                    .child(
                        div()
                            .id("onboarding-manage-extensions")
                            .debug_selector(|| "onboarding.manage".to_string())
                            .px(px(8.))
                            .cursor_pointer()
                            .text_size(px(12.))
                            .text_color(palette.dim)
                            .on_click(|_: &ClickEvent, _, cx| {
                                let _ = crate::plugins_window::open(cx);
                            })
                            .child("Manage extensions…"),
                    ),
            )
            .into_any_element()
    }

    fn provider_row(
        &self,
        index: usize,
        provider: &Arc<dyn Provider>,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let info = provider.info();
        let id = info.id.clone();
        let selected = index == self.chosen;
        let (dot, status) = self.rail_status(index, &info.connection_label, palette);
        div()
            .id(SharedString::from(format!("onboarding-provider-{id}")))
            .debug_selector(move || format!("onboarding.provider.{id}"))
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(8.))
            .py(px(7.))
            .rounded(px(7.))
            .cursor_pointer()
            .when(selected, |row| row.bg(palette.sel))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.choose(index, cx)))
            .child(
                div()
                    .size(px(30.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .bg(palette.badge)
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.win)
                    .child(initials(&info.name)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w(px(0.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text)
                            .text_ellipsis()
                            .child(info.name.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.))
                            .text_size(px(11.))
                            .text_color(palette.dim)
                            .children(dot.map(|color| {
                                div().size(px(7.)).rounded_full().bg(color).into_any_element()
                            }))
                            .child(status),
                    ),
            )
            .into_any_element()
    }

    fn rail_status(
        &self,
        index: usize,
        connection_label: &str,
        palette: &Palette,
    ) -> (Option<Hsla>, String) {
        let noun = connection_label.to_lowercase();
        match &self.listings[index] {
            Listing::Detecting => (None, "Looking…".to_string()),
            Listing::Found(rows) => (
                Some(palette.run),
                format!("{} {}", rows.len(), plural(&noun, rows.len())),
            ),
            Listing::Empty => (None, format!("No {} found", noun)),
            Listing::Failed(error) if error.hint.is_some() => {
                (Some(palette.err), "Config error".to_string())
            }
            Listing::Failed(_) => (Some(palette.err), "Couldn't load".to_string()),
        }
    }

    fn pane(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let info = self.providers[self.chosen].info();
        let noun = self.noun();
        let title = info
            .onboarding
            .title
            .clone()
            .unwrap_or_else(|| info.name.clone());
        let subtitle = info.onboarding.subtitle.clone().unwrap_or_else(|| {
            format!(
                "Choose a {noun} to open. CloudDeck only reads; it never changes your account."
            )
        });
        let footer_note = info.onboarding.footer.clone().unwrap_or_default();

        let mut header = div()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(12.))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .min_w(px(0.))
                    .child(
                        div()
                            .text_size(px(17.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(palette.dim)
                            .child(subtitle),
                    ),
            );
        if let Some(method) = self.header_method() {
            let label = method.header_button.clone().unwrap_or_default();
            let busy = self.phase != Phase::Idle;
            header = header.child(
                pill_button("onboarding-header", "onboarding.header", label, palette)
                    .when(!busy, |button| {
                        button.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.sign_in(cx)
                        }))
                    }),
            );
        }

        let body = match &self.phase {
            Phase::Opening { name } => self.opening(name, palette, cx),
            _ => self.listing(palette, cx),
        };

        let mut top = div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .px(px(22.))
            .pt(px(18.))
            .pb(px(12.))
            .child(header);
        if let Some(identity) = self.rows().iter().find_map(|row| row.identity.as_deref()) {
            top = top.child(identity_banner(identity, palette));
        }
        if let Some(error) = self.error_panel(palette) {
            top = top.child(error);
        }
        if let Phase::Waiting { code, url } = &self.phase {
            top = top.child(self.device_panel(code, url, palette, cx));
        }
        top = top.child(body);

        div()
            .debug_selector(|| "onboarding.pane".to_string())
            .flex_1()
            .min_w(px(0.))
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.win)
            .child(top)
            .child(self.footer(footer_note, palette, cx))
            .into_any_element()
    }

    fn listing(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        match &self.listings[self.chosen] {
            Listing::Detecting => self.detecting(palette),
            Listing::Found(_) => self.found(palette, cx),
            Listing::Empty => centred(
                "onboarding.empty",
                format!("No {} found", plural(&self.noun(), 2)),
                "Set one up with the provider's command-line tool, then check again.".to_string(),
                None,
                palette,
                self.check_again(palette, cx),
            ),
            Listing::Failed(error) => match &error.hint {
                Some(hint) => centred(
                    "onboarding.failed",
                    hint.title.clone(),
                    hint.detail.clone().unwrap_or_default(),
                    hint.command.clone(),
                    palette,
                    self.check_again(palette, cx),
                ),
                None => centred(
                    "onboarding.failed",
                    format!("Couldn't load {}", plural(&self.noun(), 2)),
                    error.message.clone(),
                    None,
                    palette,
                    self.check_again(palette, cx),
                ),
            },
        }
    }

    fn check_again(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let chosen = self.chosen;
        pill_button("onboarding-check", "onboarding.check-again", "Check again", palette)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.detect(chosen, cx)))
            .into_any_element()
    }

    fn detecting(&self, palette: &Palette) -> AnyElement {
        div()
            .debug_selector(|| "onboarding.detecting".to_string())
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(palette.dim)
                    .child(format!("Looking for {}s…", self.noun())),
            )
            .children((0..5).map(|_| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .px(px(10.))
                    .py(px(7.))
                    .child(div().size(px(34.)).rounded_full().bg(palette.line2))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.))
                            .child(div().w(px(160.)).h(px(10.)).rounded(px(3.)).bg(palette.line2))
                            .child(div().w(px(220.)).h(px(8.)).rounded(px(3.)).bg(palette.line2)),
                    )
                    .into_any_element()
            }))
            .into_any_element()
    }

    fn found(&self, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let query = self.query.to_lowercase();
        let selected = self.selected_row().map(|row| row.connection.id.clone());
        let rows: Vec<AnyElement> = self
            .rows()
            .iter()
            .filter(|row| {
                query.is_empty()
                    || row.connection.display_name.to_lowercase().contains(&query)
                    || row
                        .connection
                        .detail
                        .as_deref()
                        .is_some_and(|detail| detail.to_lowercase().contains(&query))
            })
            .map(|row| {
                let is_selected = selected.as_ref() == Some(&row.connection.id);
                self.connection_row(row, is_selected, palette, cx)
            })
            .collect();
        div()
            .flex_1()
            .min_h(px(0.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .debug_selector(|| "onboarding.search".to_string())
                    .h(px(30.))
                    .w_full()
                    .child(Input::new(&self.search).small()),
            )
            .child(
                div()
                    .id("onboarding-rows")
                    .debug_selector(|| "onboarding.rows".to_string())
                    .flex_1()
                    .min_h(px(0.))
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .overflow_y_scroll()
                    .children(rows),
            )
            .into_any_element()
    }

    fn connection_row(
        &self,
        row: &Row,
        selected: bool,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connection = &row.connection;
        let id = connection.id.clone();
        let selector = format!("onboarding.row.{}", connection.id.as_str());
        let mut status = div().flex().items_center().gap(px(8.)).flex_none();
        if let Some(line) = status_text(row.status.as_ref(), palette) {
            status = status.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .text_size(px(11.))
                    .text_color(line.color)
                    .children(line.dot.then(|| {
                        div()
                            .size(px(7.))
                            .rounded_full()
                            .bg(line.color)
                            .into_any_element()
                    }))
                    .child(line.text),
            );
        }
        status = status.children(connection.tags.iter().map(|tag| {
            div()
                .h(px(18.))
                .flex()
                .items_center()
                .px(px(7.))
                .rounded(px(9.))
                .bg(palette.tagbg)
                .text_size(px(10.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(palette.dim)
                .child(tag.clone())
                .into_any_element()
        }));
        div()
            .id(SharedString::from(format!("onboarding-row-{}", connection.id.as_str())))
            .debug_selector(move || selector)
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(10.))
            .py(px(7.))
            .rounded(px(7.))
            .cursor_pointer()
            .when(selected, |row| row.bg(palette.sel))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.selected = Some(id.clone());
                cx.notify();
            }))
            .child(
                div()
                    .size(px(34.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(if selected { palette.accent } else { palette.badge })
                    .text_size(px(11.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(palette.win)
                    .child(initials(&connection.display_name)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(palette.text)
                            .text_ellipsis()
                            .child(connection.display_name.clone()),
                    )
                    .children(connection.detail.clone().map(|detail| {
                        div()
                            .text_size(px(11.))
                            .text_color(palette.dim)
                            .text_ellipsis()
                            .child(detail)
                    })),
            )
            .child(status)
            .into_any_element()
    }

    fn opening(&self, name: &str, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let back = pill_button("onboarding-back", "onboarding.back", "Back to onboarding", palette)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.back_to_onboarding(cx)));
        centred(
            "onboarding.opening",
            format!("Opening {name}"),
            "The main window opens here and this window closes.".to_string(),
            None,
            palette,
            back.into_any_element(),
        )
    }

    fn error_panel(&self, palette: &Palette) -> Option<AnyElement> {
        let error = self.error.as_ref()?;
        Some(
            div()
                .debug_selector(|| "onboarding.error".to_string())
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(palette.err)
                        .child(error.message.clone()),
                )
                .children(error.advice.clone().map(|advice| {
                    div()
                        .text_size(px(11.))
                        .text_color(palette.dim)
                        .child(advice)
                }))
                .into_any_element(),
        )
    }

    fn device_panel(
        &self,
        code: &str,
        url: &str,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let open_url = url.to_string();
        let copy = code.to_string();
        div()
            .debug_selector(|| "onboarding.code-panel".to_string())
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .debug_selector(|| "onboarding.code".to_string())
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(22.))
                    .text_color(palette.text)
                    .child(code.to_string()),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.dim)
                    .child("Enter this code on the sign-in page. CloudDeck waits for the approval."),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(link(
                        "onboarding-open-sign-in".to_string(),
                        "onboarding.open-sign-in".to_string(),
                        "Open sign-in page",
                        palette,
                        cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&open_url)),
                    ))
                    .child(link(
                        "onboarding-copy-code".to_string(),
                        "onboarding.copy-code".to_string(),
                        "Copy code",
                        palette,
                        cx.listener(move |_, _: &ClickEvent, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                        }),
                    ))
                    .child(
                        pill_button(
                            "onboarding-cancel-sign-in",
                            "onboarding.cancel-sign-in",
                            "Cancel",
                            palette,
                        )
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.cancel_sign_in(cx)
                        })),
                    ),
            )
            .into_any_element()
    }

    fn footer(&self, note: String, palette: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let action: AnyElement = match &self.listings[self.chosen] {
            _ if matches!(self.phase, Phase::Opening { .. }) => div().into_any_element(),
            Listing::Detecting => pill_button(
                "onboarding-cancel",
                "onboarding.cancel",
                "Cancel",
                palette,
            )
            .on_click(cx.listener(|_, _: &ClickEvent, window, _| window.remove_window()))
            .into_any_element(),
            Listing::Found(_) => {
                let enabled = self.selected_row().is_some() && self.phase == Phase::Idle;
                div()
                    .id("onboarding-open")
                    .debug_selector(|| "onboarding.open".to_string())
                    .flex_none()
                    .px(px(14.))
                    .py(px(5.))
                    .rounded(px(6.))
                    .bg(palette.accent)
                    .text_size(px(12.))
                    .text_color(palette.win)
                    .when(!enabled, |button| button.opacity(0.45))
                    .when(enabled, |button| {
                        button.cursor_pointer().on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| this.open_selected(cx),
                        ))
                    })
                    .child("Open ↵")
                    .into_any_element()
            }
            Listing::Empty | Listing::Failed(_) => link(
                "onboarding-demo".to_string(),
                "onboarding.demo".to_string(),
                "Explore demo fleet",
                palette,
                cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(OnboardingEvent::ExploreDemo)),
            ),
        };
        div()
            .debug_selector(|| "onboarding.footer".to_string())
            .h(px(FOOTER_HEIGHT))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.))
            .px(px(22.))
            .border_t_1()
            .border_color(palette.line2)
            .child(
                div()
                    .min_w(px(0.))
                    .text_size(px(11.))
                    .text_color(palette.dim)
                    .text_ellipsis()
                    .child(note),
            )
            .child(action)
            .into_any_element()
    }
}

impl Render for OnboardingWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette(cx);
        div()
            .size_full()
            .flex()
            .bg(palette.win)
            .text_color(palette.text)
            .child(self.rail(&palette, cx))
            .child(self.pane(&palette, cx))
    }
}

fn identity_banner(identity: &str, palette: &Palette) -> AnyElement {
    div()
        .debug_selector(|| "onboarding.identity".to_string())
        .px(px(10.))
        .py(px(6.))
        .rounded(px(6.))
        .bg(palette.surf)
        .border_1()
        .border_color(palette.line)
        .text_size(px(12.))
        .text_color(palette.dim)
        .child(format!("Signed in as {identity}"))
        .into_any_element()
}

fn plural(noun: &str, count: usize) -> String {
    if count == 1 {
        noun.to_string()
    } else {
        format!("{noun}s")
    }
}

fn initials(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let letters: String = match words.as_slice() {
        [] => String::new(),
        [only] => only.chars().take(2).collect(),
        [first, second, ..] => [first, second]
            .iter()
            .filter_map(|word| word.chars().next())
            .collect(),
    };
    letters.to_uppercase()
}

struct StatusLine {
    text: String,
    color: Hsla,
    dot: bool,
}

fn status_text(status: Option<&AuthStatus>, palette: &Palette) -> Option<StatusLine> {
    let line = |text: String, color: Hsla, dot: bool| StatusLine { text, color, dot };
    match status? {
        AuthStatus::SignedOut => Some(line("Signed out".into(), palette.warntext, true)),
        AuthStatus::SignedIn {
            expires: Some(expires),
        } => {
            let remaining = expires.duration_since(SystemTime::now()).ok()?.as_secs();
            let text = if remaining >= 3600 {
                format!("Expires in {} h", remaining / 3600)
            } else {
                format!("Expires in {} min", remaining / 60)
            };
            Some(line(text, palette.dim, false))
        }
        AuthStatus::SignedIn { expires: None } => None,
        AuthStatus::Expired => Some(line("Expired".into(), palette.err, true)),
        AuthStatus::Unknown => Some(line(
            "Couldn't check sign-in".into(),
            palette.warntext,
            true,
        )),
    }
}

fn centred(
    selector: &'static str,
    heading: String,
    body: String,
    command: Option<String>,
    palette: &Palette,
    action: AnyElement,
) -> AnyElement {
    div()
        .debug_selector(move || selector.to_string())
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(10.))
        .child(
            div()
                .text_size(px(15.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(palette.text)
                .child(heading),
        )
        .children((!body.is_empty()).then(|| {
            div()
                .max_w(px(420.))
                .text_size(px(12.))
                .text_color(palette.dim)
                .child(body)
                .into_any_element()
        }))
        .children(command.map(|command| {
            div()
                .h(px(32.))
                .flex()
                .items_center()
                .px(px(12.))
                .rounded(px(6.))
                .bg(palette.panel)
                .font_family("monospace")
                .text_size(px(12.))
                .text_color(palette.text)
                .child(command)
                .into_any_element()
        }))
        .child(action)
        .into_any_element()
}

fn pill_button(
    id: &'static str,
    selector: &'static str,
    label: impl Into<SharedString>,
    palette: &Palette,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(move || selector.to_string())
        .flex_none()
        .px(px(12.))
        .py(px(5.))
        .rounded(px(6.))
        .border_1()
        .border_color(palette.line)
        .bg(palette.surf)
        .text_size(px(12.))
        .text_color(palette.text)
        .cursor_pointer()
        .child(label.into())
}

/// Opens the onboarding window with `provider_id` selected (the first provider when `None`)
/// and `connection` pre-selected. `on_event` runs after the window has closed itself.
pub fn open_onboarding_window(
    cx: &mut App,
    provider_id: Option<&str>,
    connection: Option<ConnectionId>,
    on_event: impl Fn(OnboardingEvent, &mut App) + 'static,
) -> Option<WindowHandle<Root>> {
    let providers = app_providers::providers(cx);
    open_window(
        cx,
        providers,
        provider_id,
        connection,
        WINDOW_SIZE,
        |_, _| {},
        on_event,
    )
}

/// Opens a `size` window for `providers` with `prepare` applied to its view before the first frame.
#[cfg(test)]
pub(crate) fn open_window_for(
    cx: &mut App,
    providers: Vec<Arc<dyn Provider>>,
    provider_id: Option<&str>,
    size: (f32, f32),
    prepare: impl FnOnce(&mut OnboardingWindow, &mut Context<OnboardingWindow>) + 'static,
) -> Option<WindowHandle<Root>> {
    open_window(cx, providers, provider_id, None, size, prepare, |_, _| {})
}

#[cfg(test)]
impl OnboardingWindow {
    pub(crate) fn show_detecting(&mut self) {
        self.generations[self.chosen] += 1;
        self.listings[self.chosen] = Listing::Detecting;
    }

    pub(crate) fn show_opening(&mut self, name: &str) {
        self.phase = Phase::Opening {
            name: name.to_string(),
        };
    }
}

fn open_window(
    cx: &mut App,
    providers: Vec<Arc<dyn Provider>>,
    provider_id: Option<&str>,
    connection: Option<ConnectionId>,
    window_size: (f32, f32),
    prepare: impl FnOnce(&mut OnboardingWindow, &mut Context<OnboardingWindow>) + 'static,
    on_event: impl Fn(OnboardingEvent, &mut App) + 'static,
) -> Option<WindowHandle<Root>> {
    if providers.is_empty() {
        return None;
    }
    if let Some(id) = provider_id
        && !providers.iter().any(|provider| provider.info().id == id)
    {
        return None;
    }
    let provider_id = provider_id.map(str::to_string);
    let bounds = Bounds::centered(
        None,
        size(px(window_size.0), px(window_size.1)),
        cx,
    );
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
                    let mut view = OnboardingWindow::new(
                        providers,
                        provider_id.as_deref(),
                        connection,
                        window,
                        cx,
                    );
                    prepare(&mut view, cx);
                    view
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .ok()?;
    let view = handle
        .update(cx, |root, _, _| {
            root.view().clone().downcast::<OnboardingWindow>().ok()
        })
        .ok()
        .flatten()?;
    cx.subscribe(&view, move |_, event: &OnboardingEvent, cx| {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        on_event(event.clone(), cx);
    })
    .detach();
    Some(handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_sign_in::Progress;
    use crate::test_support::{auth_provider, identified_provider, plain_provider, wait_until};
    use clouddeck_core::SignInAction;
    use gpui::{TestAppContext, VisualTestContext};

    #[test]
    fn initials_come_from_the_first_letters_of_the_first_two_words() {
        assert_eq!(initials("Amazon Web Services"), "AW");
        assert_eq!(initials("prod-readonly"), "PR");
        assert_eq!(initials("sandbox"), "SA");
        assert_eq!(initials(""), "");
    }

    #[test]
    fn plural_adds_an_s_only_for_more_than_one() {
        assert_eq!(plural("profile", 1), "profile");
        assert_eq!(plural("profile", 6), "profiles");
        assert_eq!(plural("profile", 0), "profiles");
    }

    #[gpui::test]
    fn window_opens_only_for_a_known_provider(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        cx.update(|app| {
            let plain = vec![plain_provider("plain", &["default"])];
            assert!(open_window_for(app, plain.clone(), Some("missing"), WINDOW_SIZE, |_, _| {}).is_none());
            assert!(open_window_for(app, plain, Some("plain"), WINDOW_SIZE, |_, _| {}).is_some());

            let signing_in = vec![auth_provider("fake", &["default"])];
            assert!(open_window_for(app, signing_in, Some("fake"), WINDOW_SIZE, |_, _| {}).is_some());
        });
    }

    #[gpui::test]
    fn cancelling_a_waiting_flow_stops_its_late_progress(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let providers = vec![auth_provider("fake", &["default"])];
        let handle = cx
            .update(|app| open_window_for(app, providers, Some("fake"), WINDOW_SIZE, |_, _| {}))
            .expect("onboarding window opens");
        let view = handle
            .update(cx, |root, _, _| {
                root.view().clone().downcast::<OnboardingWindow>().ok()
            })
            .expect("window is open")
            .expect("root is the onboarding window");
        let cx = VisualTestContext::from_window(handle.into(), cx).into_mut();

        view.update(cx, |view, _| {
            view.phase = Phase::Waiting {
                code: "ABCD-1234".to_string(),
                url: "https://example.test/device".to_string(),
            };
        });
        let attempt = view.read_with(cx, |view, _| view.attempt);
        view.update(cx, |view, cx| view.cancel_sign_in(cx));

        view.read_with(cx, |view, _| {
            assert_eq!(view.phase, Phase::Idle);
            assert_ne!(view.attempt, attempt);
        });
        let continues = view.update(cx, |view, cx| {
            view.progress(attempt, Progress::Action(SignInAction::Finish), cx)
        });
        assert!(!continues);
    }

    #[gpui::test]
    fn identity_banner_shows_when_the_account_resolves(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let providers = vec![identified_provider("fake", &["default"], "123456789012")];
        let handle = cx
            .update(|app| open_window_for(app, providers, Some("fake"), WINDOW_SIZE, |_, _| {}))
            .expect("onboarding window opens");
        let view = handle
            .update(cx, |root, _, _| {
                root.view().clone().downcast::<OnboardingWindow>().ok()
            })
            .expect("window is open")
            .expect("root is the onboarding window");
        wait_until(cx, |cx| {
            view.read_with(cx, |view, _| {
                matches!(view.listings[view.chosen], Listing::Found(_))
            })
        });
        let cx = VisualTestContext::from_window(handle.into(), cx).into_mut();
        cx.run_until_parked();

        assert!(cx.debug_bounds("onboarding.identity").is_some());
    }

    #[gpui::test]
    fn identity_banner_is_absent_when_the_account_fails(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let providers = vec![auth_provider("fake", &["default"])];
        let handle = cx
            .update(|app| open_window_for(app, providers, Some("fake"), WINDOW_SIZE, |_, _| {}))
            .expect("onboarding window opens");
        let view = handle
            .update(cx, |root, _, _| {
                root.view().clone().downcast::<OnboardingWindow>().ok()
            })
            .expect("window is open")
            .expect("root is the onboarding window");
        wait_until(cx, |cx| {
            view.read_with(cx, |view, _| {
                matches!(view.listings[view.chosen], Listing::Found(_))
            })
        });
        let cx = VisualTestContext::from_window(handle.into(), cx).into_mut();
        cx.run_until_parked();

        assert!(cx.debug_bounds("onboarding.identity").is_none());
    }
}
