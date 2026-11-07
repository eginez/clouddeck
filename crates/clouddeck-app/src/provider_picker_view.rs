//! Provider picker popover, opened from the provider mark in the context bar.

use clouddeck_core::ProviderMenuRow;
use gpui::{ClickEvent, Context, EventEmitter, Render, SharedString, Window, div, prelude::*, px};

use crate::icons::AppIcon;
use crate::tokens::{FONT_SIZE, ROW_HEIGHT};

const PANEL_WIDTH: f32 = 240.0;
const PANEL_RADIUS: f32 = 8.0;
const PANEL_PADDING: f32 = 6.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderPickerEvent {
    Picked(String),
}

pub struct ProviderPicker {
    rows: Vec<ProviderMenuRow>,
}

impl EventEmitter<ProviderPickerEvent> for ProviderPicker {}

impl ProviderPicker {
    pub fn new(rows: Vec<ProviderMenuRow>) -> Self {
        Self { rows }
    }
}

impl Render for ProviderPicker {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = crate::tokens::palette(cx);
        div()
            .debug_selector(|| "bar.provider-picker".into())
            .w(px(PANEL_WIDTH))
            .flex()
            .flex_col()
            .p(px(PANEL_PADDING))
            .bg(palette.surf)
            .border_1()
            .border_color(palette.line)
            .rounded(px(PANEL_RADIUS))
            .shadow_lg()
            .overflow_hidden()
            .children(self.rows.iter().map(|row| {
                let selector = format!("bar.provider.{}", row.id);
                let id = row.id.clone();
                div()
                    .id(SharedString::from(selector.clone()))
                    .debug_selector(move || selector.clone())
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .min_h(px(ROW_HEIGHT))
                    .px(px(8.0))
                    .rounded(px(6.0))
                    .when(row.active, |item| item.bg(palette.sel))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(ProviderPickerEvent::Picked(id.clone()));
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(FONT_SIZE))
                                    .text_color(palette.text)
                                    .child(row.name.clone()),
                            )
                            .when(!row.signed_in, |column| {
                                column.child(
                                    div()
                                        .text_size(px(FONT_SIZE - 2.0))
                                        .text_color(palette.dim)
                                        .child("Not signed in"),
                                )
                            }),
                    )
                    .when(row.active, |item| {
                        item.child(AppIcon::Check.icon().text_color(palette.accent))
                    })
            }))
    }
}
