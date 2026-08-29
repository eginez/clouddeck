//! Port of `AccountWindowView.swift` (the Swift CloudDeck oracle's account
//! onboarding wizard chrome): the window frame, left rail step list, and
//! footer back/cancel/continue navigation that individual wizard step
//! screens mount into.
//!
//! This bead ports the window chrome and step-navigation state machine
//! only. `AccountProviderStepView` and `AccountWorkspaceSetupStepView` (the
//! oracle's real step content) are separate beads; the `content` region
//! here renders a placeholder for whichever step is active so this chrome
//! is testable on its own. Likewise the oracle's `canAdvance`/`canFinish`
//! gating is driven by a live `AccountWizardDraft` (profile/region/provider
//! selection state) that doesn't exist yet in this port — until that
//! lands, advancing/finishing is always enabled so the placeholder step
//! sequence can be driven end to end. See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/Account/AccountWindowView.swift`
//! and `~/src/cloudDeck/main/Sources/CloudDeck/App/AppContextModels.swift`
//! (`AccountWizardStep`, `AccountWindowMode`) for the oracle this was
//! ported from.

use gpui::{
    Context, Div, InteractiveElement, IntoElement, ParentElement, Render, Stateful,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::theme::{SurgicalTheme, SurgicalTypography, radius};

/// Port of `AccountWizardStep` (`AppContextModels.swift`): the ordered
/// steps of the account onboarding wizard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountWizardStep {
    Provider,
    Workspace,
}

impl AccountWizardStep {
    /// Mirrors `AccountWizardStep.allCases` (`CaseIterable`), in order.
    pub const ALL: [AccountWizardStep; 2] = [AccountWizardStep::Provider, AccountWizardStep::Workspace];

    /// Port of `AccountWizardStep.rawValue` (its `Int` position).
    pub fn raw_value(self) -> usize {
        match self {
            AccountWizardStep::Provider => 0,
            AccountWizardStep::Workspace => 1,
        }
    }

    /// Port of `AccountWizardStep.title`.
    pub fn title(self) -> &'static str {
        match self {
            AccountWizardStep::Provider => "Provider Select",
            AccountWizardStep::Workspace => "Workspace Setup",
        }
    }

    /// Port of `stepSubtitle(for:)`.
    pub fn subtitle(self) -> &'static str {
        match self {
            AccountWizardStep::Provider => "Choose the cloud provider",
            AccountWizardStep::Workspace => "Pick source, profile, and region",
        }
    }

    /// Port of `AccountWizardStep.accessibilityIdentifier`.
    pub fn accessibility_identifier(self) -> &'static str {
        match self {
            AccountWizardStep::Provider => "provider",
            AccountWizardStep::Workspace => "workspace",
        }
    }

    /// Port of `AccountWizardStep.previous` (`AccountWizardStep(rawValue:
    /// rawValue - 1)`).
    pub fn previous(self) -> Option<AccountWizardStep> {
        match self {
            AccountWizardStep::Provider => None,
            AccountWizardStep::Workspace => Some(AccountWizardStep::Provider),
        }
    }

    /// Port of `AccountWizardStep.next` (`AccountWizardStep(rawValue:
    /// rawValue + 1)`).
    pub fn next(self) -> Option<AccountWizardStep> {
        match self {
            AccountWizardStep::Provider => Some(AccountWizardStep::Workspace),
            AccountWizardStep::Workspace => None,
        }
    }
}

/// Port of `AccountWindowMode`: whether the wizard is doing first-run
/// setup or editing an already-configured context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountWindowMode {
    SetupRequired,
    ManageContext,
}

/// Port of `AccountWindowView`: window chrome and step-navigation state
/// machine. Step content is a placeholder (see module docs); real content
/// wiring belongs to the step-view beads.
pub struct AccountWindowView {
    pub mode: AccountWindowMode,
    pub current_step: AccountWizardStep,
    /// Set when the footer's Cancel button fires (port of the oracle's
    /// `dismiss()` closure). No window-closing wiring exists yet, so this
    /// is a plain flag the composing bead can observe/react to.
    pub dismissed: bool,
    /// Set when the footer's Continue to Workbench button fires (port of
    /// `finish()`). Same caveat as `dismissed`.
    pub finished: bool,
}

impl Default for AccountWindowView {
    fn default() -> Self {
        Self {
            mode: AccountWindowMode::SetupRequired,
            // Matches the oracle's `AppState.accountWizardStep` default
            // (`AppState.swift`: `var accountWizardStep: AccountWizardStep =
            // .workspace`), not the first step in the sequence.
            current_step: AccountWizardStep::Workspace,
            dismissed: false,
            finished: false,
        }
    }
}

impl AccountWindowView {
    /// Port of `introText`.
    fn intro_text(&self) -> &'static str {
        match self.mode {
            AccountWindowMode::SetupRequired => {
                "Bring your existing AWS CLI profiles into a focused desktop workspace."
            }
            AccountWindowMode::ManageContext => {
                "Adjust provider and workspace configuration without leaving your current EC2 context."
            }
        }
    }

    /// Port of `progressText`.
    fn progress_text(&self) -> String {
        format!(
            "Step {} of {}",
            self.current_step.raw_value() + 1,
            AccountWizardStep::ALL.len()
        )
    }

    /// Port of `canOpen(step:)`. Real gating (provider must be AWS to open
    /// `workspace`) is driven by draft state that doesn't exist in this
    /// port yet; every step is open until that bead lands.
    fn can_open(&self, _step: AccountWizardStep) -> bool {
        true
    }

    /// Port of `canAdvance(from:)` / `canFinish`. See module docs: always
    /// enabled until the draft-state bead lands.
    fn can_advance(&self) -> bool {
        true
    }

    fn can_finish(&self) -> bool {
        true
    }

    /// Port of the step-button `Button` action: `appState.accountWizardStep
    /// = step`, gated by `canOpen(step:)`.
    fn go_to_step(&mut self, step: AccountWizardStep) {
        if self.can_open(step) {
            self.current_step = step;
        }
    }

    /// Port of the footer Back button action.
    fn go_back(&mut self) {
        if let Some(previous) = self.current_step.previous() {
            self.current_step = previous;
        }
    }

    /// Port of the footer Continue button action, gated by `canAdvance`.
    fn advance(&mut self) {
        if !self.can_advance() {
            return;
        }
        if let Some(next) = self.current_step.next() {
            self.current_step = next;
        }
    }

    /// Port of `finish()`, gated by `canFinish`.
    fn finish(&mut self) {
        if self.can_finish() {
            self.finished = true;
        }
    }

    /// Port of the footer Cancel button action (`cancelAccountWizardDraft`
    /// + `dismiss()`).
    fn cancel(&mut self) {
        self.dismissed = true;
    }
}

/// A small square glyph placeholder standing in for the oracle's SF Symbol
/// icons (`checkmark.circle.fill`, `rectangle.grid.2x2[.fill]`,
/// `shield.lefthalf.filled`), which have no gpui/cross-platform equivalent
/// here. Mirrors `sidebar.rs`'s `icon_placeholder`.
fn icon_placeholder(color: gpui::Hsla) -> impl IntoElement {
    div()
        .w(px(8.0))
        .h(px(8.0))
        .mt(px(4.0))
        .rounded(px(2.0))
        .bg(color)
}

fn secondary_button_id(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .px(px(14.0))
        .py(px(8.0))
        .rounded(px(radius::STANDARD))
        .bg(SurgicalTheme::surface_container())
        .text_size(px(SurgicalTypography::BODY_SIZE))
        .text_color(SurgicalTheme::text_primary())
}

fn primary_button_id(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .px(px(14.0))
        .py(px(8.0))
        .rounded(px(radius::STANDARD))
        .bg(SurgicalTheme::primary())
        .text_size(px(SurgicalTypography::BODY_SIZE))
        .text_color(SurgicalTheme::surface())
}

impl Render for AccountWindowView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let progress_text = self.progress_text();
        let intro_text = self.intro_text();
        let current_step = self.current_step;

        div()
            .debug_selector(|| "account-window-root".into())
            .flex()
            .flex_row()
            .size_full()
            .bg(SurgicalTheme::surface())
            .child(
                // leftRail
                div()
                    .debug_selector(|| "account-window-rail".into())
                    .flex()
                    .flex_col()
                    .gap(px(18.0))
                    .w(px(248.0))
                    .h_full()
                    .p(px(24.0))
                    .bg({
                        let mut c = SurgicalTheme::surface_low();
                        c.a = 0.92;
                        c
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .text_size(px(SurgicalTypography::LABEL_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child("SETUP PROGRESS")
                            .child(progress_text),
                    )
                    .child(
                        div()
                            .text_size(px(24.0))
                            .text_color(SurgicalTheme::text_primary())
                            .child("CloudDeck"),
                    )
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child(intro_text),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .children(AccountWizardStep::ALL.into_iter().map(|step| {
                                let is_active = step == current_step;
                                let is_enabled = self.can_open(step);
                                // Port of `.accessibilityIdentifier("account.step.\(step
                                // .accessibilityIdentifier).\(isActive ? "active" :
                                // "inactive")")`.
                                let selector_id = format!(
                                    "account-window-step-{}-{}",
                                    step.accessibility_identifier(),
                                    if is_active { "active" } else { "inactive" }
                                );
                                let debug_id = selector_id.clone();
                                let icon_color = if is_active {
                                    SurgicalTheme::primary()
                                } else if is_enabled {
                                    SurgicalTheme::text_secondary()
                                } else {
                                    SurgicalTheme::inactive()
                                };
                                div()
                                    .id(gpui::SharedString::from(selector_id))
                                    .debug_selector(move || debug_id.clone().into())
                                    .flex()
                                    .flex_row()
                                    .gap(px(10.0))
                                    .px(px(12.0))
                                    .py(px(10.0))
                                    .rounded(px(radius::LARGE))
                                    .bg(if is_active {
                                        SurgicalTheme::surface_container()
                                    } else {
                                        gpui::transparent_black()
                                    })
                                    .child(icon_placeholder(icon_color))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(2.0))
                                            .child(
                                                div()
                                                    .text_size(px(
                                                        SurgicalTypography::BODY_EMPHASIS_SIZE,
                                                    ))
                                                    .text_color(if is_active {
                                                        SurgicalTheme::text_primary()
                                                    } else if is_enabled {
                                                        SurgicalTheme::text_secondary()
                                                    } else {
                                                        SurgicalTheme::inactive()
                                                    })
                                                    .child(step.title()),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                                    .text_color(SurgicalTheme::text_secondary())
                                                    .child(step.subtitle()),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.go_to_step(step);
                                        cx.notify();
                                    }))
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .p(px(14.0))
                            .rounded(px(radius::LARGE))
                            .bg(SurgicalTheme::surface_container())
                            .child(icon_placeholder(SurgicalTheme::primary()))
                            .child(
                                div()
                                    .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                    .text_color(SurgicalTheme::text_secondary())
                                    .child(
                                        "CloudDeck reads local config files only; no secrets are uploaded.",
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .h_full()
                    .bg(SurgicalTheme::surface())
                    .child(
                        // content: placeholder for whichever step is
                        // active; real step screens are separate beads.
                        div()
                            .debug_selector(|| "account-window-content".into())
                            .flex()
                            .flex_col()
                            .flex_1()
                            .p(px(32.0))
                            .text_size(px(SurgicalTypography::VIEW_TITLE_SIZE))
                            .text_color(SurgicalTheme::text_primary())
                            .child(format!("{} (placeholder)", current_step.title())),
                    )
                    .child(
                        // footer
                        div()
                            .debug_selector(|| "account-window-footer".into())
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(10.0))
                            .px(px(24.0))
                            .py(px(14.0))
                            .bg(SurgicalTheme::surface_container())
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(SurgicalTypography::BODY_SIZE))
                                    .text_color(SurgicalTheme::text_secondary())
                                    .child("Choose one profile and one region to continue."),
                            )
                            .children(current_step.previous().map(|_| {
                                secondary_button_id("account-window-back")
                                    .child("Back")
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.go_back();
                                        cx.notify();
                                    }))
                            }))
                            .child(
                                secondary_button_id("account-window-cancel")
                                    .child("Cancel")
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.cancel();
                                        cx.notify();
                                    })),
                            )
                            .child(if current_step.next().is_some() {
                                secondary_button_id("account-window-next")
                                    .child("Continue")
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.advance();
                                        cx.notify();
                                    }))
                                    .into_any_element()
                            } else {
                                primary_button_id("account-window-finish")
                                    .child("Continue to Workbench")
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.finish();
                                        cx.notify();
                                    }))
                                    .into_any_element()
                            }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[test]
    fn step_previous_next_match_oracle_ordering() {
        assert_eq!(AccountWizardStep::Provider.previous(), None);
        assert_eq!(
            AccountWizardStep::Provider.next(),
            Some(AccountWizardStep::Workspace)
        );
        assert_eq!(
            AccountWizardStep::Workspace.previous(),
            Some(AccountWizardStep::Provider)
        );
        assert_eq!(AccountWizardStep::Workspace.next(), None);
    }

    #[test]
    fn go_to_step_updates_current_step() {
        let mut view = AccountWindowView {
            current_step: AccountWizardStep::Workspace,
            ..Default::default()
        };
        view.go_to_step(AccountWizardStep::Provider);
        assert_eq!(view.current_step, AccountWizardStep::Provider);
    }

    #[test]
    fn advance_and_go_back_step_through_the_sequence() {
        let mut view = AccountWindowView {
            current_step: AccountWizardStep::Provider,
            ..Default::default()
        };
        view.advance();
        assert_eq!(view.current_step, AccountWizardStep::Workspace);
        // No further step: advance is a no-op at the end.
        view.advance();
        assert_eq!(view.current_step, AccountWizardStep::Workspace);

        view.go_back();
        assert_eq!(view.current_step, AccountWizardStep::Provider);
        // No previous step: go_back is a no-op at the start.
        view.go_back();
        assert_eq!(view.current_step, AccountWizardStep::Provider);
    }

    #[test]
    fn cancel_and_finish_set_their_flags() {
        let mut view = AccountWindowView::default();
        assert!(!view.dismissed);
        view.cancel();
        assert!(view.dismissed);

        let mut view = AccountWindowView {
            current_step: AccountWizardStep::Workspace,
            ..Default::default()
        };
        assert!(!view.finished);
        view.finish();
        assert!(view.finished);
    }

    #[gpui::test]
    fn mounts_without_a_display(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| AccountWindowView::default());

        assert!(
            cx.debug_bounds("account-window-root").is_some(),
            "account window root should render"
        );
    }

    #[gpui::test]
    fn renders_chrome_regions(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| AccountWindowView::default());

        assert!(cx.debug_bounds("account-window-rail").is_some());
        assert!(cx.debug_bounds("account-window-content").is_some());
        assert!(cx.debug_bounds("account-window-footer").is_some());
    }

    #[gpui::test]
    fn first_step_has_no_back_button_last_step_has_finish(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| AccountWindowView {
            current_step: AccountWizardStep::Provider,
            ..Default::default()
        });

        assert!(
            cx.debug_bounds("account-window-back").is_none(),
            "first step should have no Back button"
        );
        assert!(cx.debug_bounds("account-window-next").is_some());
        assert!(cx.debug_bounds("account-window-finish").is_none());
    }

    /// Simulates a click at the center of the element registered under
    /// `selector` (via `debug_selector`), mirroring how a real click would
    /// land on it.
    fn click_selector(cx: &mut gpui::VisualTestContext, selector: &'static str) {
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should render before it can be clicked"));
        let center = bounds.center();
        cx.simulate_click(center, Default::default());
    }

    #[gpui::test]
    fn clicking_next_then_finish_advances_and_completes_the_sequence(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(|_, _| AccountWindowView {
            current_step: AccountWizardStep::Provider,
            ..Default::default()
        });

        click_selector(cx, "account-window-next");
        view.update(cx, |view, _cx| {
            assert_eq!(view.current_step, AccountWizardStep::Workspace);
        });

        assert!(
            cx.debug_bounds("account-window-back").is_some(),
            "second step should have a Back button"
        );
        assert!(cx.debug_bounds("account-window-finish").is_some());

        click_selector(cx, "account-window-finish");
        view.update(cx, |view, _cx| {
            assert!(view.finished, "finish button should mark the wizard finished");
        });
    }

    #[gpui::test]
    fn clicking_back_retreats_a_step(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| AccountWindowView {
            current_step: AccountWizardStep::Workspace,
            ..Default::default()
        });

        click_selector(cx, "account-window-back");
        view.update(cx, |view, _cx| {
            assert_eq!(view.current_step, AccountWizardStep::Provider);
        });
    }

    #[gpui::test]
    fn clicking_cancel_sets_dismissed(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| AccountWindowView::default());

        click_selector(cx, "account-window-cancel");
        view.update(cx, |view, _cx| {
            assert!(view.dismissed);
        });
    }

    #[gpui::test]
    fn clicking_a_step_in_the_rail_jumps_directly_to_it(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| AccountWindowView {
            current_step: AccountWizardStep::Workspace,
            ..Default::default()
        });

        // Step is currently Workspace/inactive, so its selector carries
        // the "inactive" suffix (port of the oracle's
        // `account.step.<id>.<active|inactive>` accessibility identifier).
        click_selector(cx, "account-window-step-provider-inactive");
        view.update(cx, |view, _cx| {
            assert_eq!(view.current_step, AccountWizardStep::Provider);
        });
    }
}
