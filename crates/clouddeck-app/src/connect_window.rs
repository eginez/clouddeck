//! The Connect window: one sign-in screen for any provider, drawn from its auth
//! methods and their settings (see clouddeck-core's `connect_form`).

use std::collections::BTreeMap;
use std::sync::Arc;

use clouddeck_core::extension_host::ProviderRegistry;
use clouddeck_core::{
    ConnectError, ConnectForm, ConnectionId, Provider, SettingDef, SettingKind, SettingValue,
    SignInAction, connect_methods, connect_settings, submit,
};
use gpui::{
    App, Bounds, ClickEvent, ClipboardItem, Context, Entity, EventEmitter, Render, SharedString,
    Subscription, TitlebarOptions, Window, WindowBounds, WindowHandle, WindowOptions, div, point,
    prelude::*, px, size,
};
use gpui_component::Root;
use gpui_component::input::{InputEvent, InputState};
use gpui_component::select::{SelectEvent, SelectState};

use crate::app_providers;
use crate::provider_call::{off_thread, with_call};
use crate::provider_sign_in::{Progress, drive};
use crate::setting_rows::{
    ProviderChoice, RowParts, button, choice_field, choice_state, display_text, folder_field, link,
    setting_row, switch, text_field, text_state,
};
use crate::tokens::{FONT_SIZE, Palette, palette};

pub(crate) const WINDOW_SIZE: (f32, f32) = (560.0, 640.0);
const BUTTON_RADIUS: f32 = 8.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectEvent {
    SignedIn { provider: String },
    ExploreDemo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    Idle,
    Connecting,
    Waiting { code: String, url: String },
}

pub struct ConnectWindow {
    providers: Vec<Arc<dyn Provider>>,
    registry: Arc<ProviderRegistry>,
    chosen: usize,
    pinned: Option<(String, ConnectionId)>,
    form: ConnectForm,
    inputs: Vec<(String, Entity<InputState>)>,
    choices: Vec<(String, Entity<SelectState<Vec<ProviderChoice>>>)>,
    row_errors: BTreeMap<String, String>,
    phase: Phase,
    attempt: u64,
    subscriptions: Vec<Subscription>,
}

impl EventEmitter<ConnectEvent> for ConnectWindow {}

impl ConnectWindow {
    fn new(
        providers: Vec<Arc<dyn Provider>>,
        registry: Arc<ProviderRegistry>,
        provider_id: &str,
        connection: Option<ConnectionId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let chosen = providers
            .iter()
            .position(|provider| provider.info().id == provider_id)
            .expect("open_window only opens a provider it found");
        let placeholder = ConnectForm::new(
            &providers[chosen].info(),
            Vec::new(),
            Vec::new(),
            BTreeMap::new(),
        );
        let mut this = Self {
            providers,
            registry,
            chosen,
            pinned: connection.map(|connection| (provider_id.to_string(), connection)),
            form: placeholder,
            inputs: Vec::new(),
            choices: Vec::new(),
            row_errors: BTreeMap::new(),
            phase: Phase::Idle,
            attempt: 0,
            subscriptions: Vec::new(),
        };
        this.load(chosen, window, cx);
        this
    }

    fn load(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.chosen = index;
        self.attempt += 1;
        self.phase = Phase::Idle;
        self.row_errors.clear();
        self.inputs.clear();
        self.choices.clear();
        self.subscriptions.clear();

        let provider = self.providers[index].clone();
        let info = provider.info();
        let settings = connect_settings(provider.as_ref());
        let saved: BTreeMap<String, SettingValue> = settings
            .iter()
            .filter_map(|def| {
                self.registry
                    .setting_value(&info.id, &def.key)
                    .map(|value| (def.key.clone(), value))
            })
            .collect();
        self.form = ConnectForm::new(
            &info,
            connect_methods(provider.as_ref()),
            settings.clone(),
            saved,
        );

        for def in settings {
            let text = display_text(self.form.value(&def.key).cloned());
            match &def.kind {
                SettingKind::Choice(options) => {
                    let selected = text;
                    let state = choice_state(options, &selected, window, cx);
                    let key = def.key.clone();
                    self.subscriptions.push(cx.subscribe_in(
                        &state,
                        window,
                        move |this: &mut Self,
                              _,
                              event: &SelectEvent<Vec<ProviderChoice>>,
                              _,
                              cx| {
                            if let SelectEvent::Confirm(Some(value)) = event {
                                this.form.set_value(&key, SettingValue::Text(value.clone()));
                                cx.notify();
                            }
                        },
                    ));
                    self.choices.push((def.key.clone(), state));
                }
                SettingKind::Bool => {}
                _ => {
                    let state = text_state(text, def.secret, window, cx);
                    let key = def.key.clone();
                    let parse = def.clone();
                    self.subscriptions.push(cx.subscribe_in(
                        &state,
                        window,
                        move |this: &mut Self, state, event: &InputEvent, _, cx| {
                            if matches!(event, InputEvent::Change) {
                                let text = state.read(cx).value().to_string();
                                this.edit_text(&key, &parse, &text, cx);
                            }
                        },
                    ));
                    self.inputs.push((def.key.clone(), state));
                }
            }
        }
        cx.notify();
    }

    fn edit_text(&mut self, key: &str, def: &SettingDef, text: &str, cx: &mut Context<Self>) {
        match def.parse(text) {
            Ok(value) => {
                self.row_errors.remove(key);
                self.form.set_value(key, value);
            }
            Err(error) => {
                self.row_errors.insert(key.to_string(), error.0);
            }
        }
        cx.notify();
    }

    fn choose(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index != self.chosen && index < self.providers.len() {
            self.load(index, window, cx);
        }
    }

    fn select_method(&mut self, index: usize, cx: &mut Context<Self>) {
        self.form.select(index);
        cx.notify();
    }

    fn toggle(&mut self, key: &str, cx: &mut Context<Self>) {
        let on = matches!(self.form.value(key), Some(SettingValue::Bool(true)));
        self.form.set_value(key, SettingValue::Bool(!on));
        cx.notify();
    }

    fn browse(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        let key = key.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |this, window, cx| {
                let text = path.to_string_lossy().into_owned();
                this.form.set_value(&key, SettingValue::Text(text.clone()));
                this.row_errors.remove(&key);
                if let Some((_, state)) = this.inputs.iter().find(|(k, _)| *k == key) {
                    state.update(cx, |state, cx| state.set_value(text, window, cx));
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn clear(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.form.set_value(key, SettingValue::Text(String::new()));
        self.row_errors.remove(key);
        if let Some((_, state)) = self.inputs.iter().find(|(k, _)| k == key) {
            state.update(cx, |state, cx| state.set_value(String::new(), window, cx));
        }
        cx.notify();
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        if self.phase != Phase::Idle {
            return;
        }
        if let Some(message) = self.row_errors.values().next() {
            self.form.error = Some(ConnectError {
                message: message.clone(),
                advice: None,
            });
            cx.notify();
            return;
        }
        let writes = match self.form.writes() {
            Ok(writes) => writes,
            Err(error) => {
                self.form.error = Some(error);
                cx.notify();
                return;
            }
        };
        let Some(method) = self.form.selected_method().cloned() else {
            return;
        };

        self.attempt += 1;
        let attempt = self.attempt;
        self.phase = Phase::Connecting;
        self.form.error = None;
        cx.notify();

        let provider = self.providers[self.chosen].clone();
        let registry = self.registry.clone();
        let extension = provider.info().id;
        let connection = self
            .pinned
            .as_ref()
            .filter(|(id, _)| *id == extension)
            .map(|(_, connection)| connection.clone());
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
                            writes,
                            |key, value| registry.set_setting(&extension, key, value),
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
                cx.emit(ConnectEvent::SignedIn { provider });
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
                self.form.error = Some(error.into());
            }
        }
        cx.notify();
        true
    }

    fn fail(&mut self, attempt: u64, error: ConnectError, cx: &mut Context<Self>) {
        if attempt == self.attempt {
            self.phase = Phase::Idle;
            self.form.error = Some(error);
            cx.notify();
        }
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.phase == Phase::Idle {
            window.remove_window();
        } else {
            self.attempt += 1;
            self.phase = Phase::Idle;
            cx.notify();
        }
    }

    fn control(
        &self,
        def: &SettingDef,
        palette: &Palette,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let key = def.key.clone();
        let selector = format!("connect.control.{key}");
        match &def.kind {
            SettingKind::Bool => {
                let on = matches!(self.form.value(&key), Some(SettingValue::Bool(true)));
                switch(
                    selector,
                    on,
                    palette,
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle(&key, cx)),
                )
            }
            SettingKind::Choice(_) => match self.choices.iter().find(|(k, _)| *k == key) {
                Some((_, state)) => choice_field(state, selector, palette),
                None => div().into_any_element(),
            },
            SettingKind::Folder => match self.inputs.iter().find(|(k, _)| *k == key) {
                Some((_, state)) => {
                    let browse_key = key.clone();
                    let clear_key = key.clone();
                    let browse = link(
                        format!("connect-browse-{key}"),
                        format!("connect.browse.{key}"),
                        "Browse",
                        palette,
                        cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.browse(&browse_key, window, cx)
                        }),
                    );
                    let clear = link(
                        format!("connect-clear-{key}"),
                        format!("connect.clear.{key}"),
                        "Clear",
                        palette,
                        cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.clear(&clear_key, window, cx)
                        }),
                    );
                    folder_field(state, selector, browse, clear)
                }
                None => div().into_any_element(),
            },
            kind => match self.inputs.iter().find(|(k, _)| *k == key) {
                Some((_, state)) => text_field(state, kind, selector, palette),
                None => div().into_any_element(),
            },
        }
    }

    fn setting_rows(&self, palette: &Palette, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let rows: Vec<SettingDef> = self.form.rows().into_iter().cloned().collect();
        rows.iter()
            .map(|def| {
                let control = self.control(def, palette, cx);
                setting_row(
                    RowParts {
                        scope: "connect",
                        item: format!("connect.row.{}", def.key),
                        row: def.key.clone(),
                        def,
                        query: "",
                        changed: false,
                        error: self.row_errors.get(&def.key).cloned(),
                        missing: false,
                        reset: None,
                        control,
                    },
                    palette,
                )
            })
            .collect()
    }

    fn provider_chips(&self, palette: &Palette, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .flex()
            .gap(px(8.))
            .children(self.providers.iter().enumerate().map(|(index, provider)| {
                let info = provider.info();
                let id = info.id.clone();
                let selected = index == self.chosen;
                div()
                    .id(SharedString::from(format!("connect-provider-{id}")))
                    .debug_selector(move || format!("connect.provider.{id}"))
                    .px(px(12.))
                    .py(px(4.))
                    .rounded(px(BUTTON_RADIUS))
                    .border_1()
                    .border_color(if selected {
                        palette.accent
                    } else {
                        palette.line
                    })
                    .text_size(px(FONT_SIZE))
                    .text_color(if selected {
                        palette.accent
                    } else {
                        palette.text
                    })
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.choose(index, window, cx)
                    }))
                    .child(info.name)
            }))
            .into_any_element()
    }

    fn method_list(&self, palette: &Palette, cx: &mut Context<Self>) -> gpui::AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .children(self.form.methods.iter().enumerate().map(|(index, method)| {
                let selected = index == self.form.selected;
                div()
                    .id(SharedString::from(format!("connect-method-{index}")))
                    .debug_selector(move || format!("connect.method.{index}"))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.select_method(index, cx)
                        }),
                    )
                    .child(
                        div()
                            .size(px(12.))
                            .rounded(px(6.))
                            .border_1()
                            .border_color(palette.accent)
                            .when(selected, |dot| dot.bg(palette.accent)),
                    )
                    .child(
                        div()
                            .text_size(px(FONT_SIZE))
                            .text_color(palette.text)
                            .child(method.label.clone()),
                    )
            }))
            .into_any_element()
    }

    fn error_panel(&self, palette: &Palette) -> Option<gpui::AnyElement> {
        let error = self.form.error.as_ref()?;
        Some(
            div()
                .debug_selector(|| "connect.error".to_string())
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(
                    div()
                        .text_size(px(FONT_SIZE))
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
    ) -> gpui::AnyElement {
        let open_url = url.to_string();
        let copy = code.to_string();
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .debug_selector(|| "connect.code".to_string())
                    .text_size(px(22.))
                    .text_color(palette.text)
                    .child(code.to_string()),
            )
            .child(
                div().text_size(px(11.)).text_color(palette.dim).child(
                    "Enter this code on the sign-in page. CloudDeck waits for the approval.",
                ),
            )
            .child(
                div()
                    .flex()
                    .gap(px(12.))
                    .child(link(
                        "connect-open".to_string(),
                        "connect.open".to_string(),
                        "Open sign-in page",
                        palette,
                        cx.listener(move |_, _: &ClickEvent, _, cx| {
                            cx.open_url(&open_url);
                        }),
                    ))
                    .child(link(
                        "connect-copy".to_string(),
                        "connect.copy".to_string(),
                        "Copy code",
                        palette,
                        cx.listener(move |_, _: &ClickEvent, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                        }),
                    )),
            )
            .into_any_element()
    }
}

#[cfg(test)]
impl ConnectWindow {
    pub(crate) fn show_error(&mut self, error: ConnectError) {
        self.form.error = Some(error);
    }

    pub(crate) fn show_waiting(&mut self, code: &str, url: &str) {
        self.phase = Phase::Waiting {
            code: code.to_string(),
            url: url.to_string(),
        };
    }
}

impl Render for ConnectWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = palette(cx);
        let busy = self.phase != Phase::Idle;
        let connect_label = if self.phase == Phase::Connecting {
            "Connecting..."
        } else {
            "Connect"
        };
        let description = self
            .form
            .selected_method()
            .and_then(|method| method.description.clone());

        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .p(px(20.))
            .child(
                div()
                    .text_size(px(18.))
                    .text_color(palette.text)
                    .child(self.form.title.clone()),
            )
            .child(self.provider_chips(&palette, cx));
        if self.form.methods.len() > 1 {
            body = body.child(self.method_list(&palette, cx));
        }
        if let Some(description) = description {
            body = body.child(
                div()
                    .text_size(px(11.))
                    .text_color(palette.dim)
                    .child(description),
            );
        }
        body = body.children(self.setting_rows(&palette, cx));
        if let Some(error) = self.error_panel(&palette) {
            body = body.child(error);
        }
        if let Phase::Waiting { code, url } = &self.phase {
            body = body.child(self.device_panel(code, url, &palette, cx));
        }

        let connect = button("connect.connect", connect_label, &palette)
            .debug_selector(|| "connect.connect".to_string())
            .when(!busy, |button| {
                button.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.connect(cx)))
            });
        let cancel = button("connect.cancel", "Cancel", &palette)
            .debug_selector(|| "connect.cancel".to_string())
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel(window, cx)));
        let demo = link(
            "connect-demo".to_string(),
            "connect.demo".to_string(),
            "Explore demo fleet",
            &palette,
            cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(ConnectEvent::ExploreDemo)),
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .justify_between()
            .bg(palette.win)
            .child(body)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .p(px(20.))
                    .child(div().flex().gap(px(8.)).child(connect).child(cancel))
                    .child(demo),
            )
    }
}

/// Opens the Connect window for `provider_id`. `on_event` runs after the window has
/// closed itself on a sign-in or demo choice.
pub fn open_connect_window(
    cx: &mut App,
    provider_id: &str,
    connection: Option<ConnectionId>,
    on_event: impl Fn(ConnectEvent, &mut App) + 'static,
) -> Option<WindowHandle<Root>> {
    let providers: Vec<Arc<dyn Provider>> = app_providers::providers(cx)
        .into_iter()
        .filter(|provider| provider.auth().is_some())
        .collect();
    let registry = app_providers::registry(cx);
    open_window(
        cx,
        providers,
        registry,
        provider_id,
        connection,
        |_, _| {},
        on_event,
    )
}

/// Opens a window for `providers` with `prepare` applied to its view before the first frame.
#[cfg(test)]
pub(crate) fn open_window_for(
    cx: &mut App,
    providers: Vec<Arc<dyn Provider>>,
    registry: Arc<ProviderRegistry>,
    provider_id: &str,
    prepare: impl FnOnce(&mut ConnectWindow, &mut Context<ConnectWindow>) + 'static,
) -> Option<WindowHandle<Root>> {
    open_window(
        cx,
        providers,
        registry,
        provider_id,
        None,
        prepare,
        |_, _| {},
    )
}

fn open_window(
    cx: &mut App,
    providers: Vec<Arc<dyn Provider>>,
    registry: Arc<ProviderRegistry>,
    provider_id: &str,
    connection: Option<ConnectionId>,
    prepare: impl FnOnce(&mut ConnectWindow, &mut Context<ConnectWindow>) + 'static,
    on_event: impl Fn(ConnectEvent, &mut App) + 'static,
) -> Option<WindowHandle<Root>> {
    if !providers
        .iter()
        .any(|provider| provider.info().id == provider_id)
    {
        return None;
    }
    let provider_id = provider_id.to_string();
    let bounds = Bounds::centered(None, size(px(WINDOW_SIZE.0), px(WINDOW_SIZE.1)), cx);
    let handle = cx
        .open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some(SharedString::from("Connect")),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(14.0), px(15.0))),
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                app_id: Some(crate::main_window::APP_ID.to_string()),
                ..Default::default()
            },
            move |window, cx| {
                let view = cx.new(|cx| {
                    let mut view = ConnectWindow::new(
                        providers,
                        registry,
                        &provider_id,
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
            root.view().clone().downcast::<ConnectWindow>().ok()
        })
        .ok()
        .flatten()?;
    cx.subscribe(&view, move |_, event: &ConnectEvent, cx| {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
        on_event(event.clone(), cx);
    })
    .detach();
    Some(handle)
}
