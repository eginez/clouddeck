//! Port of `CloudContextStatusBar.swift` (the Swift CloudDeck oracle's
//! bottom bar showing the active cloud provider/profile/region) to gpui.
//!
//! This bead ports layout and static/mock context data only. The
//! `CloudContextQuickPicker` popover that the profile/region chips launch
//! is ported in `quick_picker.rs` (its pure mock-data/filtering logic) and
//! rendered here, since selecting a row needs to mutate this view's own
//! `active_quick_picker`/`profile`/`region` state — the hook point this
//! bead originally left for it.
//! See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/Shared/CloudContextStatusBar.swift`
//! for the oracle this was ported from.

use gpui::{
    BoxShadow, Context, InteractiveElement, IntoElement, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, point, prelude::FluentBuilder, px,
};

use crate::quick_picker::{
    filtered_profiles, filtered_regions, footer_message, mock_profiles, picker_title,
    recent_profiles, region_row_title, search_prompt,
};
use crate::theme::{SurgicalTheme, SurgicalTypography, radius};

/// `AppState.fleetRegionIdentifier` in the Swift oracle: the sentinel
/// region value meaning "curated fleet view" rather than a real AWS
/// region.
pub const FLEET_REGION_IDENTIFIER: &str = "fleet";

/// Port of the oracle's inline `switch appState.contextStatusState`
/// (an `AppState`-owned enum, not defined in this file, but consumed by
/// it for `statusIndicatorColor`/`statusText`).
// `Reloading`/`SetupRequired`/`Error` are not yet constructed anywhere:
// `CloudContextStatusBar::default()` only ever sets `Ready`, since driving
// these from live `AppState`/service reload state is a follow-on wiring
// bead, not this one. Kept as full parity with the oracle's
// `ContextStatusState` enum.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextStatusState {
    Ready,
    Reloading,
    SetupRequired,
    Error(String),
}

/// Port of `CloudContextQuickPickerKind`: which chip's quick-picker (a
/// separate bead) is currently toggled open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloudContextQuickPickerKind {
    Profile,
    Region,
}

/// Port of `CloudContextStatusBar`. Context data (`provider`, `profile`,
/// `region`, `status`) is static/mock for this bead; live wiring to
/// `AppState` happens in a view-model bead.
pub struct CloudContextStatusBar {
    pub provider: String,
    pub profile: Option<String>,
    pub region: String,
    pub status: ContextStatusState,
    /// Which chip's picker is toggled open. Toggled by clicking the
    /// profile/region chips; the `CloudContextQuickPicker` popover renders
    /// beneath the status bar while this is `Some`.
    pub active_quick_picker: Option<CloudContextQuickPickerKind>,
    /// Port of `CloudContextQuickPicker.searchText`. Kept here rather than
    /// on a separate picker entity so click handlers on the popover's rows
    /// can mutate `profile`/`region`/`active_quick_picker` directly.
    pub quick_picker_search_text: String,
}

impl Default for CloudContextStatusBar {
    fn default() -> Self {
        Self {
            provider: "AWS".into(),
            profile: None,
            region: String::new(),
            status: ContextStatusState::Ready,
            active_quick_picker: None,
            quick_picker_search_text: String::new(),
        }
    }
}

impl CloudContextStatusBar {
    /// Port of `profileChip`'s title fallback: `appState.selectedProfileName
    /// ?? "Profile"`.
    fn profile_chip_title(&self) -> String {
        self.profile.clone().unwrap_or_else(|| "Profile".into())
    }

    /// Port of `regionChipTitle`.
    fn region_chip_title(&self) -> String {
        if self.region.is_empty() {
            "Region".into()
        } else if self.region == FLEET_REGION_IDENTIFIER {
            "Fleet".into()
        } else {
            self.region.clone()
        }
    }

    /// Port of `statusIndicatorColor`.
    fn status_indicator_color(&self) -> gpui::Hsla {
        match self.status {
            ContextStatusState::Ready => SurgicalTheme::text_secondary(),
            ContextStatusState::Reloading => SurgicalTheme::warning(),
            ContextStatusState::SetupRequired => SurgicalTheme::warning(),
            ContextStatusState::Error(_) => SurgicalTheme::error(),
        }
    }

    /// Port of `statusText`.
    fn status_text(&self) -> String {
        match &self.status {
            ContextStatusState::Ready => "Ready".into(),
            ContextStatusState::Reloading => "Refreshing...".into(),
            ContextStatusState::SetupRequired => "Setup Required".into(),
            ContextStatusState::Error(message) => message.clone(),
        }
    }

    /// Port of `toggleQuickPicker(_:)`.
    fn toggle_quick_picker(&mut self, kind: CloudContextQuickPickerKind) {
        self.active_quick_picker = if self.active_quick_picker == Some(kind) {
            None
        } else {
            Some(kind)
        };
        self.quick_picker_search_text.clear();
    }

    /// Port of `dismiss()` as passed into `CloudContextQuickPicker`.
    fn dismiss_quick_picker(&mut self) {
        self.active_quick_picker = None;
        self.quick_picker_search_text.clear();
    }

    /// Port of `profileButton`'s action:
    /// `appState.applyQuickProfileSelection(profile.name); dismiss()`.
    fn select_profile(&mut self, name: String) {
        self.profile = Some(name);
        self.dismiss_quick_picker();
    }

    /// Port of `regionButton`'s action:
    /// `appState.applyQuickRegionSelection(region); dismiss()`.
    fn select_region(&mut self, region: String) {
        self.region = region;
        self.dismiss_quick_picker();
    }

    /// Port of `CloudContextQuickPicker.searchText`'s binding. Not yet
    /// called anywhere: the quick picker's search field is display-only
    /// for now (no editable text-input widget exists in this codebase yet
    /// — see `instance_list.rs`'s doc comment on the same limitation).
    #[allow(dead_code)]
    pub fn set_quick_picker_search_text(&mut self, text: impl Into<String>) {
        self.quick_picker_search_text = text.into();
    }
}

/// Port of `actionChip`'s `.shadow(color: SurgicalTheme.shadow, radius: 2,
/// x: 0, y: 1)`. gpui has no SF Symbols equivalent for the chip icons, so
/// callers substitute a short text glyph for `systemImage`.
fn chip_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: SurgicalTheme::shadow(),
        offset: point(px(0.0), px(1.0)),
        blur_radius: px(2.0),
        spread_radius: px(0.0),
    }]
}

/// Port of `.surgicalGlassPanel()`'s `.shadow(color: SurgicalTheme.shadow,
/// radius: 16, x: 0, y: 8)` on the quick picker's outer panel.
fn popover_shadow() -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: SurgicalTheme::shadow(),
        offset: point(px(0.0), px(8.0)),
        blur_radius: px(16.0),
        spread_radius: px(0.0),
    }]
}

/// Port of `profileButton`/`regionButton`'s selection dot + row background,
/// shared between the profile and region row builders below.
fn quick_picker_row(
    selector: String,
    is_selected: bool,
    emphasized: bool,
    is_profile_row: bool,
    title: String,
    subtitle: Option<String>,
) -> gpui::Stateful<gpui::Div> {
    // Port of `profileButton`'s `.opacity(0.24)` / `regionButton`'s
    // `.opacity(0.20)` unselected dot opacity — this differs by kind, not
    // by whether the row is the emphasized "Current" row.
    let dot_color = {
        let mut c = SurgicalTheme::primary();
        if !is_selected {
            c.a *= if is_profile_row { 0.24 } else { 0.20 };
        }
        c
    };
    let background = if is_selected {
        SurgicalTheme::surface_high()
    } else if emphasized {
        SurgicalTheme::surface_container()
    } else {
        SurgicalTheme::surface_low()
    };

    let mut row = div()
        .id(gpui::SharedString::from(selector.clone()))
        .debug_selector(move || selector.clone().into())
        .flex()
        .flex_row()
        .items_center()
        .gap(px(9.0))
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(radius::STANDARD))
        .bg(background)
        .child(div().w(px(7.0)).h(px(7.0)).rounded(px(3.5)).bg(dot_color));

    row = if let Some(subtitle) = subtitle {
        row.child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(SurgicalTypography::BODY_EMPHASIS_SIZE))
                        .text_color(SurgicalTheme::text_primary())
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(SurgicalTypography::CAPTION_SIZE))
                        .text_color(SurgicalTheme::text_secondary())
                        .child(subtitle),
                ),
        )
    } else {
        row.child(
            div()
                .flex_1()
                .text_size(px(SurgicalTypography::BODY_SIZE))
                .text_color(SurgicalTheme::text_primary())
                .child(title),
        )
    };

    if is_selected {
        row = row.child(
            div()
                .text_size(px(SurgicalTypography::CAPTION_SIZE))
                .text_color(SurgicalTheme::primary())
                .child("\u{2713}"),
        );
    }

    row
}

impl CloudContextStatusBar {
    /// Port of `CloudContextQuickPicker`'s body: header, search field, list
    /// content, footer. Rendered from `render()` when `active_quick_picker`
    /// is `Some`; sits below the chip row. See `quick_picker.rs` for the
    /// mock data/filtering logic this pulls from.
    fn render_quick_picker(
        &self,
        kind: CloudContextQuickPickerKind,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_profile = kind == CloudContextQuickPickerKind::Profile;
        let query = self.quick_picker_search_text.clone();

        let mut list = div()
            .debug_selector(|| "statusbar.quickpicker.list".into())
            .flex()
            .flex_col()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(radius::LARGE))
            .bg(SurgicalTheme::surface_low());

        if is_profile {
            // Port of `profileRow`: look up the full `AWSProfile` so the
            // Current/Recent sections can display `appState.profileHint(for:)`
            // the same way the "All Profiles"/"Matches" section does, rather
            // than a placeholder label.
            let all_profiles = mock_profiles();
            let hint_for = |name: &str| -> Option<String> {
                all_profiles
                    .iter()
                    .find(|p| p.name == name)
                    .map(|p| p.hint.clone())
            };

            if let Some(active) = &self.profile {
                list = list.child(section_label("Current")).child(quick_picker_row(
                    format!("statusbar.quickpicker.profile.row.{active}"),
                    true,
                    true,
                    true,
                    active.clone(),
                    Some(hint_for(active).unwrap_or_else(|| "Current profile".into())),
                ));
            }

            let recents = recent_profiles(self.profile.as_deref());
            if !recents.is_empty() {
                let mut recent_col = div().flex().flex_col().gap(px(4.0));
                for name in &recents {
                    let name = name.clone();
                    let subtitle = hint_for(&name).unwrap_or_else(|| "Recent".into());
                    recent_col = recent_col.child(
                        quick_picker_row(
                            format!("statusbar.quickpicker.profile.row.{name}"),
                            false,
                            false,
                            true,
                            name.clone(),
                            Some(subtitle),
                        )
                        .on_click(cx.listener({
                            let name = name.clone();
                            move |this, _event, _window, cx| {
                                this.select_profile(name.clone());
                                cx.notify();
                            }
                        })),
                    );
                }
                list = list.child(section_label("Recent")).child(recent_col);
            }

            let matches = filtered_profiles(self.profile.as_deref(), &query);
            list = list.child(section_label(if query.is_empty() {
                "All Profiles"
            } else {
                "Matches"
            }));
            if matches.is_empty() {
                list = list.child(empty_state("No matching profiles"));
            } else {
                let mut col = div().flex().flex_col().gap(px(4.0));
                for profile in matches {
                    let name = profile.name.clone();
                    col = col.child(
                        quick_picker_row(
                            format!("statusbar.quickpicker.profile.row.{name}"),
                            false,
                            false,
                            true,
                            profile.name.clone(),
                            Some(profile.hint.clone()),
                        )
                        .on_click(cx.listener(
                            move |this, _event, _window, cx| {
                                this.select_profile(name.clone());
                                cx.notify();
                            },
                        )),
                    );
                }
                list = list.child(col);
            }
        } else {
            let matches = filtered_regions(&query);
            list = list.child(section_label(if query.is_empty() {
                "Regions"
            } else {
                "Matches"
            }));
            if matches.is_empty() {
                list = list.child(empty_state("No matching regions"));
            } else {
                let mut col = div().flex().flex_col().gap(px(4.0));
                for region in matches {
                    let is_selected = region == self.region;
                    let title = region_row_title(&region);
                    let region_id = region.clone();
                    col = col.child(
                        quick_picker_row(
                            format!("statusbar.quickpicker.region.row.{region_id}"),
                            is_selected,
                            false,
                            false,
                            title,
                            None,
                        )
                        .on_click(cx.listener(
                            move |this, _event, _window, cx| {
                                this.select_region(region_id.clone());
                                cx.notify();
                            },
                        )),
                    );
                }
                list = list.child(col);
            }
        }

        div()
            .id("statusbar.quickpicker")
            .debug_selector(|| "statusbar.quickpicker".into())
            .absolute()
            .bottom(px(48.0))
            .left(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .w(px(336.0))
            .p(px(12.0))
            .rounded(px(radius::LARGE))
            .bg(SurgicalTheme::surface())
            .border_1()
            .border_color({
                let mut c = SurgicalTheme::outline_variant();
                c.a *= 0.35;
                c
            })
            .shadow(popover_shadow())
            .child(
                // header
                div()
                    .debug_selector(|| "statusbar.quickpicker.header".into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(SurgicalTypography::BODY_EMPHASIS_SIZE))
                                    .text_color(SurgicalTheme::text_primary())
                                    .child(picker_title(is_profile)),
                            )
                            .child(
                                div()
                                    .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                    .text_color(SurgicalTheme::text_secondary())
                                    .child(if is_profile {
                                        self.profile
                                            .clone()
                                            .unwrap_or_else(|| "No profile selected".into())
                                    } else {
                                        format!("AWS {}", self.provider)
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .id("statusbar.quickpicker.dismiss")
                            .debug_selector(|| "statusbar.quickpicker.dismiss".into())
                            .w(px(28.0))
                            .h(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(radius::STANDARD))
                            .text_color(SurgicalTheme::text_secondary())
                            .child("\u{2715}")
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.dismiss_quick_picker();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                // searchField (static display, not real keyboard input in
                // this bead's scope; see `set_quick_picker_search_text`).
                div()
                    .debug_selector(|| "statusbar.quickpicker.search".into())
                    .px(px(10.0))
                    .py(px(7.0))
                    .rounded(px(radius::STANDARD))
                    .bg(SurgicalTheme::surface_container())
                    .text_size(px(SurgicalTypography::BODY_SIZE))
                    .text_color(if query.is_empty() {
                        SurgicalTheme::text_secondary()
                    } else {
                        SurgicalTheme::text_primary()
                    })
                    .child(if query.is_empty() {
                        search_prompt(is_profile).to_string()
                    } else {
                        query.clone()
                    }),
            )
            .child(list)
            .child(
                // footer
                div()
                    .debug_selector(|| "statusbar.quickpicker.footer".into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child(footer_message(is_profile)),
                    )
                    .child(
                        // Port of the "Manage Accounts..." button. The
                        // oracle's `manageAccounts` callback (opening real
                        // account-management UI) is out of scope for this
                        // mock-data bead, so this just dismisses the picker.
                        div()
                            .id("statusbar.quickpicker.manage-accounts")
                            .debug_selector(|| "statusbar.quickpicker.manage-accounts".into())
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(radius::STANDARD))
                            .bg(SurgicalTheme::surface_container())
                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                            .text_color(SurgicalTheme::text_primary())
                            .child("Manage Accounts...")
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.dismiss_quick_picker();
                                cx.notify();
                            })),
                    ),
            )
    }
}

/// Port of `sectionLabel(_:)`.
fn section_label(title: &'static str) -> impl IntoElement {
    div()
        .text_size(px(SurgicalTypography::LABEL_SIZE))
        .text_color(SurgicalTheme::text_secondary())
        .child(title.to_uppercase())
}

/// Port of `emptyState(_:)`.
fn empty_state(title: &'static str) -> impl IntoElement {
    div()
        .px(px(10.0))
        .py(px(10.0))
        .rounded(px(radius::STANDARD))
        .bg(SurgicalTheme::surface_container())
        .text_size(px(SurgicalTypography::CAPTION_SIZE))
        .text_color(SurgicalTheme::text_secondary())
        .child(title)
}

impl Render for CloudContextStatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let provider_label = format!("\u{2601} {}", self.provider); // cloud glyph stand-in for systemImage: "cloud"
        let profile_title = self.profile_chip_title();
        let region_title = self.region_chip_title();
        let status_color = self.status_indicator_color();
        let status_text = self.status_text();

        let chip_border_color = {
            let mut c = SurgicalTheme::outline_variant();
            c.a *= 0.45;
            c
        };

        let active_quick_picker = self.active_quick_picker;

        div()
            .id("statusbar.root")
            .debug_selector(|| "statusbar.root".into())
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .px(px(16.0))
            .py(px(10.0))
            .bg(SurgicalTheme::chrome())
            .border_t_1()
            .border_color({
                let mut c = SurgicalTheme::outline_variant();
                c.a *= 0.35;
                c
            })
            .child(
                // providerLabel
                div()
                    .id("statusbar.provider")
                    .debug_selector(|| "statusbar.provider".into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(SurgicalTypography::METADATA_SIZE))
                    .text_color(SurgicalTheme::text_primary())
                    .child(provider_label)
                    .child(
                        // `Circle().fill(textSecondary.opacity(0.5))
                        // .frame(width: 3, height: 3)` separator dot.
                        div()
                            .debug_selector(|| "statusbar.provider.separator".into())
                            .w(px(3.0))
                            .h(px(3.0))
                            .rounded(px(1.5))
                            .bg({
                                let mut c = SurgicalTheme::text_secondary();
                                c.a *= 0.5;
                                c
                            }),
                    ),
            )
            .child(
                // Divider() .frame(height: 16)
                div()
                    .debug_selector(|| "statusbar.divider".into())
                    .w(px(1.0))
                    .h(px(16.0))
                    .bg(SurgicalTheme::outline_variant()),
            )
            .child(
                // profileChip
                div()
                    .id("statusbar.profile")
                    .debug_selector(|| "statusbar.profile".into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .py(px(7.0))
                    .rounded(px(radius::STANDARD))
                    .bg(SurgicalTheme::surface_highest())
                    .border_1()
                    .border_color(chip_border_color)
                    .shadow(chip_shadow())
                    .text_size(px(SurgicalTypography::METADATA_SIZE))
                    .text_color(SurgicalTheme::text_primary())
                    .child("\u{1F464}") // stand-in for systemImage: "person.crop.circle"
                    .child(profile_title)
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.toggle_quick_picker(CloudContextQuickPickerKind::Profile);
                        cx.notify();
                    })),
            )
            .child(
                // regionChip
                div()
                    .id("statusbar.region")
                    .debug_selector(|| "statusbar.region".into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .px(px(12.0))
                    .py(px(7.0))
                    .rounded(px(radius::STANDARD))
                    .bg(SurgicalTheme::surface_highest())
                    .border_1()
                    .border_color(chip_border_color)
                    .shadow(chip_shadow())
                    .text_size(px(SurgicalTypography::METADATA_SIZE))
                    .text_color(SurgicalTheme::text_primary())
                    .child("\u{1F4CD}") // stand-in for systemImage: "location"
                    .child(region_title)
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.toggle_quick_picker(CloudContextQuickPickerKind::Region);
                        cx.notify();
                    })),
            )
            .child(
                // Spacer(minLength: 16)
                div()
                    .debug_selector(|| "statusbar.spacer".into())
                    .flex_1()
                    .min_w(px(16.0)),
            )
            .child(
                // statusIndicator
                div()
                    .id("statusbar.message")
                    .debug_selector(|| "statusbar.message".into())
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .debug_selector(|| "statusbar.message.dot".into())
                            .w(px(7.0))
                            .h(px(7.0))
                            .rounded(px(3.5))
                            .bg(status_color),
                    )
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                            .text_color(status_color)
                            .child(status_text),
                    ),
            )
            .when_some(active_quick_picker, |el, kind| {
                el.child(self.render_quick_picker(kind, cx))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};

    /// Test-only harness that docks `CloudContextStatusBar` at the bottom
    /// of a tall window, mirroring `AppShell`'s real layout (sidebar/main
    /// content above, status bar docked at the bottom). Mounting the
    /// status bar as a window's sole root view (as the other tests in this
    /// module do for plain layout assertions) leaves no room above it for
    /// the quick-picker popover, which anchors via `.bottom(...)` off the
    /// status bar's own (short) container — clicks on rows land outside
    /// the window canvas in that scaffolding. Tests exercising the
    /// popover's click behavior use this harness instead.
    struct StatusBarHarness {
        bar: Entity<CloudContextStatusBar>,
    }

    impl Render for StatusBarHarness {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .flex()
                .flex_col()
                .size_full()
                .child(div().flex_1())
                .child(self.bar.clone())
        }
    }

    fn mount_docked(
        cx: &mut TestAppContext,
        bar: CloudContextStatusBar,
    ) -> (Entity<CloudContextStatusBar>, &mut VisualTestContext) {
        let (harness, cx) = cx.add_window_view(|_, cx| {
            let bar = cx.new(|_| bar);
            StatusBarHarness { bar }
        });
        let bar_entity = harness.read_with(cx, |h, _| h.bar.clone());
        (bar_entity, cx)
    }

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| CloudContextStatusBar::default());

        assert!(
            cx.debug_bounds("statusbar.root").is_some(),
            "statusbar.root should render"
        );
    }

    #[gpui::test]
    fn renders_all_sections(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| CloudContextStatusBar {
            provider: "AWS".into(),
            profile: Some("prod-admin".into()),
            region: "us-west-2".into(),
            status: ContextStatusState::Ready,
            active_quick_picker: None,
            quick_picker_search_text: String::new(),
        });

        assert!(cx.debug_bounds("statusbar.provider").is_some());
        assert!(cx.debug_bounds("statusbar.profile").is_some());
        assert!(cx.debug_bounds("statusbar.region").is_some());
        assert!(cx.debug_bounds("statusbar.message").is_some());
    }

    #[gpui::test]
    fn sections_are_laid_out_left_to_right(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| CloudContextStatusBar {
            provider: "AWS".into(),
            profile: Some("prod-admin".into()),
            region: "us-west-2".into(),
            status: ContextStatusState::Ready,
            active_quick_picker: None,
            quick_picker_search_text: String::new(),
        });

        let provider = cx.debug_bounds("statusbar.provider").unwrap();
        let profile = cx.debug_bounds("statusbar.profile").unwrap();
        let region = cx.debug_bounds("statusbar.region").unwrap();
        let message = cx.debug_bounds("statusbar.message").unwrap();

        assert!(provider.origin.x < profile.origin.x);
        assert!(profile.origin.x < region.origin.x);
        assert!(
            region.origin.x < message.origin.x,
            "status indicator should be pushed to the far right by the spacer"
        );
    }

    #[test]
    fn region_chip_title_matches_oracle_fallbacks() {
        let mut bar = CloudContextStatusBar::default();
        assert_eq!(bar.region_chip_title(), "Region");

        bar.region = FLEET_REGION_IDENTIFIER.into();
        assert_eq!(bar.region_chip_title(), "Fleet");

        bar.region = "eu-central-1".into();
        assert_eq!(bar.region_chip_title(), "eu-central-1");
    }

    #[test]
    fn profile_chip_title_falls_back_when_unset() {
        let bar = CloudContextStatusBar::default();
        assert_eq!(bar.profile_chip_title(), "Profile");
    }

    #[test]
    fn status_text_and_color_match_oracle_states() {
        let mut bar = CloudContextStatusBar::default();
        assert_eq!(bar.status_text(), "Ready");

        bar.status = ContextStatusState::Reloading;
        assert_eq!(bar.status_text(), "Refreshing...");
        assert_eq!(bar.status_indicator_color(), SurgicalTheme::warning());

        bar.status = ContextStatusState::SetupRequired;
        assert_eq!(bar.status_text(), "Setup Required");
        assert_eq!(bar.status_indicator_color(), SurgicalTheme::warning());

        bar.status = ContextStatusState::Error("boom".into());
        assert_eq!(bar.status_text(), "boom");
        assert_eq!(bar.status_indicator_color(), SurgicalTheme::error());
    }

    #[test]
    fn toggle_quick_picker_flips_and_untoggles() {
        let mut bar = CloudContextStatusBar::default();
        assert_eq!(bar.active_quick_picker, None);

        bar.toggle_quick_picker(CloudContextQuickPickerKind::Profile);
        assert_eq!(
            bar.active_quick_picker,
            Some(CloudContextQuickPickerKind::Profile)
        );

        bar.toggle_quick_picker(CloudContextQuickPickerKind::Profile);
        assert_eq!(bar.active_quick_picker, None);

        bar.toggle_quick_picker(CloudContextQuickPickerKind::Region);
        assert_eq!(
            bar.active_quick_picker,
            Some(CloudContextQuickPickerKind::Region)
        );
    }

    #[test]
    fn select_profile_sets_profile_and_closes_the_picker() {
        let mut bar = CloudContextStatusBar {
            active_quick_picker: Some(CloudContextQuickPickerKind::Profile),
            quick_picker_search_text: "sand".into(),
            ..CloudContextStatusBar::default()
        };

        bar.select_profile("sandbox".into());

        assert_eq!(bar.profile, Some("sandbox".into()));
        assert_eq!(bar.active_quick_picker, None);
        assert_eq!(
            bar.quick_picker_search_text, "",
            "search text should be cleared on selection, matching the oracle's dismiss()"
        );
    }

    #[test]
    fn select_region_sets_region_and_closes_the_picker() {
        let mut bar = CloudContextStatusBar {
            active_quick_picker: Some(CloudContextQuickPickerKind::Region),
            quick_picker_search_text: "us-".into(),
            ..CloudContextStatusBar::default()
        };

        bar.select_region("us-west-2".into());

        assert_eq!(bar.region, "us-west-2");
        assert_eq!(bar.active_quick_picker, None);
        assert_eq!(bar.quick_picker_search_text, "");
    }

    #[test]
    fn dismiss_quick_picker_closes_without_changing_selection() {
        let mut bar = CloudContextStatusBar {
            profile: Some("prod-admin".into()),
            region: "us-east-1".into(),
            active_quick_picker: Some(CloudContextQuickPickerKind::Region),
            quick_picker_search_text: "eu".into(),
            ..CloudContextStatusBar::default()
        };

        bar.dismiss_quick_picker();

        assert_eq!(bar.active_quick_picker, None);
        assert_eq!(bar.quick_picker_search_text, "");
        assert_eq!(bar.profile, Some("prod-admin".into()));
        assert_eq!(bar.region, "us-east-1");
    }

    #[gpui::test]
    fn quick_picker_is_hidden_until_a_chip_toggles_it_open(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| CloudContextStatusBar::default());

        assert!(
            cx.debug_bounds("statusbar.quickpicker").is_none(),
            "quick picker should not render until a chip toggles active_quick_picker"
        );
    }

    #[gpui::test]
    fn clicking_the_profile_chip_opens_the_picker_with_selectable_rows(cx: &mut TestAppContext) {
        let (_bar, cx) = mount_docked(
            cx,
            CloudContextStatusBar {
                profile: Some("prod-admin".into()),
                ..CloudContextStatusBar::default()
            },
        );

        let chip = cx
            .debug_bounds("statusbar.profile")
            .expect("profile chip should render");
        cx.simulate_click(chip.center(), gpui::Modifiers::default());

        assert!(
            cx.debug_bounds("statusbar.quickpicker").is_some(),
            "quick picker should open from the status bar's profile chip hook"
        );
        assert!(
            cx.debug_bounds("statusbar.quickpicker.profile.row.sandbox")
                .is_some(),
            "an unselected mock profile should be listed as a selectable row"
        );
    }

    #[gpui::test]
    fn clicking_a_profile_row_selects_it_and_closes_the_picker(cx: &mut TestAppContext) {
        let (view, cx) = mount_docked(
            cx,
            CloudContextStatusBar {
                active_quick_picker: Some(CloudContextQuickPickerKind::Profile),
                ..CloudContextStatusBar::default()
            },
        );

        let row = cx
            .debug_bounds("statusbar.quickpicker.profile.row.sandbox")
            .expect("sandbox profile row should render while the picker is open");
        cx.simulate_click(row.center(), gpui::Modifiers::default());

        // NB: `debug_bounds` entries accumulate for the lifetime of the test
        // window and are never cleared per-frame (gpui's test-support debug
        // map is write-only bookkeeping, not a live "is this on screen right
        // now" query), so "the picker closed" is asserted via the entity's
        // own state rather than a `debug_bounds(...).is_none()` check.
        view.read_with(cx, |bar, _| {
            assert_eq!(bar.profile, Some("sandbox".into()));
            assert_eq!(
                bar.active_quick_picker, None,
                "picker should close once a profile row is selected"
            );
        });
    }

    #[gpui::test]
    fn clicking_the_region_chip_opens_the_picker_with_selectable_rows(cx: &mut TestAppContext) {
        let (_bar, cx) = mount_docked(
            cx,
            CloudContextStatusBar {
                region: "us-east-1".into(),
                ..CloudContextStatusBar::default()
            },
        );

        let chip = cx
            .debug_bounds("statusbar.region")
            .expect("region chip should render");
        cx.simulate_click(chip.center(), gpui::Modifiers::default());

        assert!(cx.debug_bounds("statusbar.quickpicker").is_some());
        assert!(
            cx.debug_bounds("statusbar.quickpicker.region.row.us-west-2")
                .is_some(),
            "an unselected mock region should be listed as a selectable row"
        );
    }

    #[gpui::test]
    fn clicking_a_region_row_selects_it_and_closes_the_picker(cx: &mut TestAppContext) {
        let (view, cx) = mount_docked(
            cx,
            CloudContextStatusBar {
                active_quick_picker: Some(CloudContextQuickPickerKind::Region),
                ..CloudContextStatusBar::default()
            },
        );

        let row = cx
            .debug_bounds("statusbar.quickpicker.region.row.us-west-2")
            .expect("us-west-2 region row should render while the picker is open");
        cx.simulate_click(row.center(), gpui::Modifiers::default());

        // See the comment in `clicking_a_profile_row_selects_it_and_closes_the_picker`
        // on why closing is asserted via entity state, not `debug_bounds`.
        view.read_with(cx, |bar, _| {
            assert_eq!(bar.region, "us-west-2");
            assert_eq!(
                bar.active_quick_picker, None,
                "picker should close once a region row is selected"
            );
        });
    }

    #[gpui::test]
    fn clicking_the_dismiss_button_closes_the_picker_without_selecting(cx: &mut TestAppContext) {
        let (view, cx) = mount_docked(
            cx,
            CloudContextStatusBar {
                profile: Some("prod-admin".into()),
                active_quick_picker: Some(CloudContextQuickPickerKind::Profile),
                ..CloudContextStatusBar::default()
            },
        );

        let dismiss = cx
            .debug_bounds("statusbar.quickpicker.dismiss")
            .expect("dismiss button should render");
        cx.simulate_click(dismiss.center(), gpui::Modifiers::default());

        view.read_with(cx, |bar, _| {
            assert_eq!(
                bar.profile,
                Some("prod-admin".into()),
                "dismissing without selecting a row should not change the profile"
            );
            assert_eq!(
                bar.active_quick_picker, None,
                "dismiss button should close the picker"
            );
        });
    }
}
