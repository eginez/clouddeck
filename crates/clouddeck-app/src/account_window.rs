//! Compact AWS onboarding and context management.
//!
//! The original Swift view used a provider step, a progress rail, and a
//! second workspace step. CloudDeck only supports AWS, so that structure made
//! a simple profile/region choice feel like a wizard. This view keeps the
//! public draft fields that the shell observes, but presents one focused
//! screen and leaves the actual credential operation to the core service.
//!
//! The connection state is deliberately UI-facing. Core/auth code can report
//! an identity, browser-login wait, or recoverable failure through the public
//! methods below without this module knowing how credentials are resolved.

use gpui::{
    Context, Div, InteractiveElement, IntoElement, ParentElement, Render, Stateful,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::theme::{SurgicalTheme, SurgicalTypography, radius};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountWindowMode {
    SetupRequired,
    ManageContext,
}

/// A recoverable connection problem that can be displayed without exposing
/// credential details. The auth service may map its richer errors into this
/// small UI vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountConnectionError {
    MissingProfile,
    MissingRegion,
    UnsupportedProfile,
    ExpiredLogin,
    AccessDenied,
    Network,
    Unknown(String),
}

impl AccountConnectionError {
    pub fn title(&self) -> &'static str {
        match self {
            Self::MissingProfile => "Choose a profile to continue",
            Self::MissingRegion => "Choose a region to continue",
            Self::UnsupportedProfile => "This profile cannot be used here yet",
            Self::ExpiredLogin => "Sign in again to continue",
            Self::AccessDenied => "AWS access is restricted",
            Self::Network => "Could not reach AWS",
            Self::Unknown(_) => "Connection could not be completed",
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::MissingProfile => "Select one of the detected AWS profiles, then connect."
                .into(),
            Self::MissingRegion => "Select an AWS region for the EC2 fleet, then connect.".into(),
            Self::UnsupportedProfile => {
                "This profile uses an external credential provider that CloudDeck cannot open yet. Choose another profile.".into()
            }
            Self::ExpiredLogin => {
                "The selected profile's browser session has expired. Sign in again and retry."
                    .into()
            }
            Self::AccessDenied => {
                "AWS confirmed your identity, but this profile cannot describe EC2 in the selected region."
                    .into()
            }
            Self::Network => "AWS could not be reached. Check your connection and try again.".into(),
            Self::Unknown(message) => message.clone(),
        }
    }
}

/// The lifecycle of the connection check. `Connected` includes an AWS account
/// identity; a profile name alone is never presented as an account identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountConnectionState {
    Idle,
    Connecting,
    AwaitingBrowserLogin,
    CheckingFleetAccess { account_id: String },
    Connected { account_id: String },
    Failed(AccountConnectionError),
}

impl Default for AccountConnectionState {
    fn default() -> Self {
        Self::Idle
    }
}

impl AccountConnectionState {
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            Self::Connecting | Self::AwaitingBrowserLogin | Self::CheckingFleetAccess { .. }
        )
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }
}

/// Device authorization details supplied by the SDK while browser sign in is
/// pending. Keeping this UI type independent of the core crate lets the auth
/// implementation evolve while preserving a stable rendering contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountBrowserLoginPrompt {
    pub verification_uri: String,
    pub verification_uri_complete: Option<String>,
    pub user_code: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// State and UI for the single Connect to AWS screen.
pub struct AccountWindowView {
    /// Profile metadata is intentionally secret-free and comes from the core
    /// profile scanner. `None` means scanning has not completed yet; `Some`
    /// with an empty vector means the scan completed and found nothing.
    pub profiles: Option<Vec<clouddeck_core::AwsProfile>>,
    /// Region choices supplied by the shell or a test fixture.
    pub available_regions: Option<Vec<String>>,
    pub selected_profile: Option<String>,
    pub selected_region: String,
    pub mode: AccountWindowMode,
    pub dismissed: bool,
    /// Set only after the user chooses Open Fleet following a successful
    /// identity/connection check.
    pub finished: bool,
    pub connection_state: AccountConnectionState,
    pub browser_login_prompt: Option<AccountBrowserLoginPrompt>,
    pub profile_menu_open: bool,
    pub region_menu_open: bool,
    /// One-shot requests consumed by the composing shell/auth service.
    pub connect_requested: bool,
    pub rescan_requested: bool,
    pub sign_in_requested: bool,
    pub open_browser_requested: bool,
    pub demo_requested: bool,
    /// Incremented for each attempt and on cancellation so an async auth
    /// result can be ignored when it belongs to an obsolete attempt.
    pub connection_attempt: u64,
}

impl Default for AccountWindowView {
    fn default() -> Self {
        Self {
            profiles: None,
            available_regions: None,
            selected_profile: None,
            selected_region: "us-east-1".into(),
            mode: AccountWindowMode::SetupRequired,
            dismissed: false,
            finished: false,
            connection_state: AccountConnectionState::Idle,
            browser_login_prompt: None,
            profile_menu_open: false,
            region_menu_open: false,
            connect_requested: false,
            rescan_requested: false,
            sign_in_requested: false,
            open_browser_requested: false,
            demo_requested: false,
            connection_attempt: 0,
        }
    }
}

impl AccountWindowView {
    /// Start an SDK-backed connection check. The service observes
    /// `connect_requested`, captures `connection_attempt`, and reports the
    /// next state through the methods below.
    pub fn begin_connect(&mut self) {
        if self.selected_profile.as_deref().is_none_or(str::is_empty) {
            self.connection_state =
                AccountConnectionState::Failed(AccountConnectionError::MissingProfile);
            return;
        }
        if !is_usable_region(&self.selected_region) {
            self.connection_state =
                AccountConnectionState::Failed(AccountConnectionError::MissingRegion);
            return;
        }
        self.connection_attempt = self.connection_attempt.wrapping_add(1);
        self.connect_requested = true;
        self.connection_state = AccountConnectionState::Connecting;
    }

    /// Tell the UI that the SDK is waiting for the user's browser SSO flow.
    pub fn await_browser_login(&mut self) {
        self.connection_state = AccountConnectionState::AwaitingBrowserLogin;
    }

    pub fn await_browser_login_with_prompt(&mut self, prompt: AccountBrowserLoginPrompt) {
        self.browser_login_prompt = Some(prompt);
        self.await_browser_login();
    }

    /// Report a resolved identity while the service is still checking EC2
    /// access. This state is intentionally not successful yet: STS identity
    /// confirms credentials, while the fleet/DescribeInstances check confirms
    /// that the selected profile can actually open the workbench.
    pub fn identity_resolved(&mut self, account_id: impl Into<String>) {
        self.browser_login_prompt = None;
        self.connection_state = AccountConnectionState::CheckingFleetAccess {
            account_id: account_id.into(),
        };
    }

    pub fn identity_resolved_for_attempt(
        &mut self,
        attempt: u64,
        account_id: impl Into<String>,
    ) -> bool {
        if self.connection_attempt != attempt || self.dismissed {
            return false;
        }
        self.identity_resolved(account_id);
        true
    }

    /// Report a resolved AWS account identity. The window remains open until
    /// the user explicitly chooses Open Fleet, which gives identity feedback
    /// time to be read and keeps failures recoverable.
    pub fn complete_connection(&mut self, account_id: impl Into<String>) {
        self.browser_login_prompt = None;
        self.connection_state = AccountConnectionState::Connected {
            account_id: account_id.into(),
        };
    }

    /// Apply a result only when it belongs to the currently visible attempt.
    /// This is the cancellation guard for async browser authentication.
    pub fn complete_connection_for_attempt(
        &mut self,
        attempt: u64,
        account_id: impl Into<String>,
    ) -> bool {
        if self.connection_attempt != attempt || self.dismissed {
            return false;
        }
        self.complete_connection(account_id);
        true
    }

    pub fn fail_connection(&mut self, error: AccountConnectionError) {
        self.connection_state = AccountConnectionState::Failed(error);
    }

    pub fn fail_connection_for_attempt(
        &mut self,
        attempt: u64,
        error: AccountConnectionError,
    ) -> bool {
        if self.connection_attempt != attempt || self.dismissed {
            return false;
        }
        self.fail_connection(error);
        true
    }

    pub fn retry_connection(&mut self) {
        if matches!(
            self.connection_state,
            AccountConnectionState::Failed(AccountConnectionError::ExpiredLogin)
        ) {
            self.request_sign_in();
        } else {
            self.begin_connect();
        }
    }

    pub fn cancel_browser_login(&mut self) {
        self.connection_attempt = self.connection_attempt.wrapping_add(1);
        self.connect_requested = false;
        self.browser_login_prompt = None;
        self.connection_state = AccountConnectionState::Idle;
    }

    pub fn request_rescan(&mut self) {
        self.connection_attempt = self.connection_attempt.wrapping_add(1);
        self.connect_requested = false;
        self.rescan_requested = true;
        self.connection_state = AccountConnectionState::Idle;
    }

    pub fn request_sign_in(&mut self) {
        self.sign_in_requested = true;
        self.open_browser_requested = true;
        self.connection_attempt = self.connection_attempt.wrapping_add(1);
        self.browser_login_prompt = None;
        self.connection_state = AccountConnectionState::AwaitingBrowserLogin;
    }

    pub fn request_open_browser(&mut self) {
        self.open_browser_requested = true;
    }

    pub fn request_demo(&mut self) {
        self.demo_requested = true;
    }

    pub fn open_fleet(&mut self) {
        if self.connection_state.is_connected() {
            self.finished = true;
        }
    }

    pub fn cancel(&mut self) {
        self.dismissed = true;
        self.connection_attempt = self.connection_attempt.wrapping_add(1);
    }

    fn select_profile(&mut self, name: String, configured_region: Option<String>) {
        self.connection_attempt = self.connection_attempt.wrapping_add(1);
        self.connect_requested = false;
        self.browser_login_prompt = None;
        self.selected_profile = Some(name);
        if let Some(region) = configured_region.filter(|region| is_usable_region(region)) {
            self.selected_region = region;
        }
        self.profile_menu_open = false;
        self.browser_login_prompt = None;
        self.connection_state = AccountConnectionState::Idle;
    }

    fn select_region(&mut self, region: String) {
        self.connection_attempt = self.connection_attempt.wrapping_add(1);
        self.connect_requested = false;
        self.selected_region = region;
        self.region_menu_open = false;
        self.browser_login_prompt = None;
        self.connection_state = AccountConnectionState::Idle;
    }

    fn profile_name(&self) -> &str {
        self.selected_profile
            .as_deref()
            .unwrap_or("Choose a profile")
    }

    fn region_options(&self) -> Vec<String> {
        let source = self.available_regions.clone().unwrap_or_else(|| {
            clouddeck_core::FLEET_REGIONS
                .iter()
                .map(|region| (*region).to_string())
                .collect()
        });
        let mut regions: Vec<String> = source
            .into_iter()
            // `fleet` is an app scope used by the shell, not an AWS region.
            .filter(|region| region != "fleet")
            .filter(|region| !region.trim().is_empty())
            .collect();
        regions.sort();
        regions.dedup();
        regions
    }

    fn can_connect(&self) -> bool {
        self.selected_profile
            .as_deref()
            .is_some_and(|profile| !profile.trim().is_empty())
            && is_usable_region(&self.selected_region)
            && !self.connection_state.is_busy()
    }

    fn connection_status(&self) -> (&'static str, gpui::Hsla) {
        match &self.connection_state {
            AccountConnectionState::Idle => ("Ready to connect", SurgicalTheme::text_secondary()),
            AccountConnectionState::Connecting => {
                ("Checking AWS identity…", SurgicalTheme::warning())
            }
            AccountConnectionState::AwaitingBrowserLogin => {
                ("Waiting for browser sign in…", SurgicalTheme::warning())
            }
            AccountConnectionState::CheckingFleetAccess { .. } => {
                ("Checking EC2 access…", SurgicalTheme::warning())
            }
            AccountConnectionState::Connected { .. } => ("Connected", SurgicalTheme::primary()),
            AccountConnectionState::Failed(_) => ("Action needed", SurgicalTheme::error()),
        }
    }
}

fn selector(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .debug_selector(move || id.into())
        .flex_shrink_0()
        .rounded(px(radius::STANDARD))
}

fn dynamic_selector(id: String) -> Stateful<Div> {
    let debug_id = id.clone();
    div()
        .id(gpui::SharedString::from(id))
        .debug_selector(move || debug_id.clone().into())
        .flex_shrink_0()
        .rounded(px(radius::STANDARD))
}

fn is_usable_region(region: &str) -> bool {
    !region.trim().is_empty() && region != "fleet"
}

fn label(text: impl Into<String>) -> impl IntoElement {
    div()
        .text_size(px(SurgicalTypography::LABEL_SIZE))
        .text_color(SurgicalTheme::text_secondary())
        .child(text.into())
}

fn panel() -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .p(px(16.0))
        .rounded(px(radius::LARGE))
        .bg(SurgicalTheme::surface_low())
}

impl AccountWindowView {
    fn profile_selector(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let profile_name = self.profile_name().to_string();
        let mut root = div()
            .id("account-profile-field")
            .debug_selector(|| "account-profile-field".into())
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(label("AWS profile"))
            .child(
                selector("account-profile-selector")
                    .w_full()
                    .px(px(12.0))
                    .py(px(11.0))
                    .bg(SurgicalTheme::surface_container())
                    .text_size(px(SurgicalTypography::BODY_SIZE))
                    .text_color(if self.selected_profile.is_some() {
                        SurgicalTheme::text_primary()
                    } else {
                        SurgicalTheme::text_secondary()
                    })
                    .child(profile_name)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.profile_menu_open = !this.profile_menu_open;
                        this.region_menu_open = false;
                        cx.notify();
                    })),
            );

        if self.profile_menu_open {
            let mut menu = div()
                .id("account-profile-menu")
                .debug_selector(|| "account-profile-menu".into())
                .flex()
                .flex_col()
                .gap(px(4.0))
                .p(px(6.0))
                .rounded(px(radius::STANDARD))
                .bg(SurgicalTheme::surface_container());
            for profile in self.profiles.clone().unwrap_or_default() {
                let name = profile.name.clone();
                let configured_region = profile.region.clone();
                let is_selected = self.selected_profile.as_deref() == Some(name.as_str());
                menu = menu.child(
                    dynamic_selector(format!("account-profile-option-{name}"))
                        .w_full()
                        .px(px(10.0))
                        .py(px(9.0))
                        .bg(if is_selected {
                            SurgicalTheme::surface_high()
                        } else {
                            SurgicalTheme::surface()
                        })
                        .text_size(px(SurgicalTypography::BODY_SIZE))
                        .text_color(SurgicalTheme::text_primary())
                        .child(name.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_profile(name.clone(), configured_region.clone());
                            cx.notify();
                        })),
                );
            }
            if self.profiles.as_ref().is_some_and(Vec::is_empty) {
                menu = menu.child(
                    div()
                        .px(px(10.0))
                        .py(px(8.0))
                        .text_size(px(SurgicalTypography::BODY_SIZE))
                        .text_color(SurgicalTheme::text_secondary())
                        .child("No profiles detected yet"),
                );
            }
            root = root.child(menu);
        }
        root.into_any_element()
    }

    fn region_selector(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let selected = if self.selected_region == "fleet" || self.selected_region.is_empty() {
            "Choose an AWS region".to_string()
        } else {
            self.selected_region.clone()
        };
        let mut root = div()
            .id("account-region-field")
            .debug_selector(|| "account-region-field".into())
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(label("Region"))
            .child(
                selector("account-region-selector")
                    .w_full()
                    .px(px(12.0))
                    .py(px(11.0))
                    .bg(SurgicalTheme::surface_container())
                    .text_size(px(SurgicalTypography::BODY_SIZE))
                    .text_color(
                        if self.selected_region.is_empty() || self.selected_region == "fleet" {
                            SurgicalTheme::text_secondary()
                        } else {
                            SurgicalTheme::text_primary()
                        },
                    )
                    .child(selected)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.region_menu_open = !this.region_menu_open;
                        this.profile_menu_open = false;
                        cx.notify();
                    })),
            );

        if self.region_menu_open {
            let mut menu = div()
                .id("account-region-menu")
                .debug_selector(|| "account-region-menu".into())
                .flex()
                .flex_col()
                .gap(px(4.0))
                .p(px(6.0))
                .rounded(px(radius::STANDARD))
                .bg(SurgicalTheme::surface_container());
            for region in self.region_options() {
                let is_selected = self.selected_region == region;
                let option_id = format!("account-region-option-{region}");
                let region_name = region.clone();
                menu = menu.child(
                    dynamic_selector(option_id)
                        .w_full()
                        .px(px(10.0))
                        .py(px(9.0))
                        .bg(if is_selected {
                            SurgicalTheme::surface_high()
                        } else {
                            SurgicalTheme::surface()
                        })
                        .text_size(px(SurgicalTypography::BODY_SIZE))
                        .text_color(SurgicalTheme::text_primary())
                        .child(region.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_region(region_name.clone());
                            cx.notify();
                        })),
                );
            }
            root = root.child(menu);
        }
        root.into_any_element()
    }

    fn profiles_hint(&self) -> gpui::AnyElement {
        match self.profiles.as_deref() {
            None => div()
                .id("account-profile-scan-state")
                .debug_selector(|| "account-profile-scan-state".into())
                .text_size(px(SurgicalTypography::CAPTION_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child("Looking for saved AWS profiles…")
                .into_any_element(),
            Some([]) => div()
                .id("account-empty-profiles")
                .debug_selector(|| "account-empty-profiles".into())
                .flex()
                .flex_col()
                .gap(px(6.0))
                .p(px(12.0))
                .rounded(px(radius::STANDARD))
                .bg(SurgicalTheme::surface_container())
                .child(
                    div()
                        .text_size(px(SurgicalTypography::BODY_SIZE))
                        .text_color(SurgicalTheme::text_primary())
                        .child("No AWS profiles detected"),
                )
                .child(
                    div()
                        .text_size(px(SurgicalTypography::CAPTION_SIZE))
                        .text_color(SurgicalTheme::text_secondary())
                        .child("Sign in through your browser, then rescan to pick up the profile."),
                )
                .into_any_element(),
            Some(profiles) => div()
                .id("account-profile-count")
                .debug_selector(|| "account-profile-count".into())
                .text_size(px(SurgicalTypography::CAPTION_SIZE))
                .text_color(SurgicalTheme::text_secondary())
                .child(format!(
                    "{} profile{} detected",
                    profiles.len(),
                    if profiles.len() == 1 { "" } else { "s" }
                ))
                .into_any_element(),
        }
    }

    fn connection_panel(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (status, status_color) = self.connection_status();
        let mut root = panel()
            .id("account-connection-panel")
            .debug_selector(|| "account-connection-panel".into())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w(px(8.0))
                            .h(px(8.0))
                            .rounded(px(4.0))
                            .bg(status_color),
                    )
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::BODY_EMPHASIS_SIZE))
                            .text_color(status_color)
                            .child(status),
                    ),
            );

        match &self.connection_state {
            AccountConnectionState::Idle => {
                root = root.child(
                    div()
                        .text_size(px(SurgicalTypography::CAPTION_SIZE))
                        .text_color(SurgicalTheme::text_secondary())
                        .child("CloudDeck will verify your AWS identity before opening the fleet."),
                );
            }
            AccountConnectionState::Connecting => {
                root = root
                    .child(
                        div()
                            .id("account-connecting-message")
                            .debug_selector(|| "account-connecting-message".into())
                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child("Checking credentials and AWS identity…"),
                    )
                    .child(
                        selector("account-cancel-connection")
                            .px(px(10.0))
                            .py(px(7.0))
                            .bg(SurgicalTheme::surface_container())
                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                            .text_color(SurgicalTheme::text_primary())
                            .child("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_browser_login();
                                cx.notify();
                            })),
                    );
            }
            AccountConnectionState::AwaitingBrowserLogin => {
                let mut login = div()
                    .id("account-browser-login-message")
                    .debug_selector(|| "account-browser-login-message".into())
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .text_size(px(SurgicalTypography::CAPTION_SIZE))
                    .text_color(SurgicalTheme::text_secondary())
                    .child("Complete sign in in your browser. CloudDeck is waiting for AWS to confirm it.");
                if let Some(prompt) = self.browser_login_prompt.clone() {
                    login = login
                        .child(format!("Code: {}", prompt.user_code))
                        .child(format!(
                            "Open: {}",
                            prompt
                                .verification_uri_complete
                                .unwrap_or(prompt.verification_uri)
                        ))
                        .child(format!(
                            "This code expires in {} seconds.",
                            prompt.expires_in
                        ));
                    root = root.child(login).child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(8.0))
                            .child(
                                selector("account-open-browser")
                                    .px(px(10.0))
                                    .py(px(7.0))
                                    .bg(SurgicalTheme::surface_container())
                                    .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                    .text_color(SurgicalTheme::text_primary())
                                    .child("Open sign-in page")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.request_open_browser();
                                        cx.notify();
                                    })),
                            )
                            .child(
                                selector("account-cancel-login")
                                    .px(px(10.0))
                                    .py(px(7.0))
                                    .bg(SurgicalTheme::surface_container())
                                    .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                    .text_color(SurgicalTheme::text_primary())
                                    .child("Cancel sign in")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.cancel_browser_login();
                                        cx.notify();
                                    })),
                            ),
                    );
                } else {
                    root = root.child(login).child(
                        selector("account-cancel-login")
                            .px(px(10.0))
                            .py(px(7.0))
                            .bg(SurgicalTheme::surface_container())
                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                            .text_color(SurgicalTheme::text_primary())
                            .child("Cancel sign in")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_browser_login();
                                cx.notify();
                            })),
                    );
                }
            }
            AccountConnectionState::CheckingFleetAccess { .. } => {
                root = root
                    .child(
                        div()
                            .id("account-checking-fleet-message")
                            .debug_selector(|| "account-checking-fleet-message".into())
                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child(
                                "Identity confirmed. Checking EC2 access in the selected region…",
                            ),
                    )
                    .child(
                        selector("account-cancel-connection")
                            .px(px(10.0))
                            .py(px(7.0))
                            .bg(SurgicalTheme::surface_container())
                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                            .text_color(SurgicalTheme::text_primary())
                            .child("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel_browser_login();
                                cx.notify();
                            })),
                    );
            }
            AccountConnectionState::Connected { account_id } => {
                root = root
                    .child(
                        div()
                            .id("account-identity")
                            .debug_selector(|| "account-identity".into())
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(SurgicalTheme::text_primary())
                            .child(format!("AWS account {account_id}")),
                    )
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child(format!(
                                "Profile: {} · Region: {}",
                                self.profile_name(),
                                self.selected_region
                            )),
                    );
            }
            AccountConnectionState::Failed(error) => {
                root = root
                    .child(
                        div()
                            .id("account-connection-error")
                            .debug_selector(|| "account-connection-error".into())
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(SurgicalTheme::error())
                            .child(error.title()),
                    )
                    .child(
                        div()
                            .text_size(px(SurgicalTypography::CAPTION_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child(error.message()),
                    );

                let retry_label = if matches!(error, &AccountConnectionError::ExpiredLogin) {
                    "Sign in again"
                } else {
                    "Retry"
                };
                root = root.child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(8.0))
                        .child(
                            selector("account-retry")
                                .px(px(10.0))
                                .py(px(7.0))
                                .bg(SurgicalTheme::surface_container())
                                .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                .text_color(SurgicalTheme::text_primary())
                                .child(retry_label)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.retry_connection();
                                    cx.notify();
                                })),
                        )
                        .child(
                            selector("account-rescan-error")
                                .px(px(10.0))
                                .py(px(7.0))
                                .bg(SurgicalTheme::surface_container())
                                .text_size(px(SurgicalTypography::CAPTION_SIZE))
                                .text_color(SurgicalTheme::text_primary())
                                .child("Rescan profiles")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.request_rescan();
                                    cx.notify();
                                })),
                        ),
                );
            }
        }
        root.into_any_element()
    }
}

impl Render for AccountWindowView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode_copy = self.mode;
        let connect_enabled = self.can_connect();
        let state_connected = self.connection_state.is_connected();
        let state_busy = self.connection_state.is_busy();
        let primary_label = if state_busy {
            "Connecting…"
        } else if state_connected {
            "Open Fleet"
        } else {
            "Connect to AWS"
        };

        let mut root = div()
            .debug_selector(|| "account-window-root".into())
            .size_full()
            .flex()
            .flex_col()
            .bg(SurgicalTheme::surface())
            .child(
                div()
                    .id("account-connect-screen")
                    .debug_selector(|| "account-connect-screen".into())
                    .flex()
                    .flex_col()
                    .flex_1()
                    .items_center()
                    .overflow_y_scroll()
                    .p(px(32.0))
                    .child(
                        div()
                            .w(px(620.0))
                            .flex()
                            .flex_col()
                            .gap(px(18.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .text_size(px(30.0))
                                            .text_color(SurgicalTheme::text_primary())
                                            .child(
                                                if matches!(mode_copy, AccountWindowMode::SetupRequired) {
                                                    "Connect to AWS"
                                                } else {
                                                    "AWS connection"
                                                },
                                            ),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(SurgicalTypography::BODY_SIZE))
                                            .text_color(SurgicalTheme::text_secondary())
                                            .child("Choose a saved profile and region. CloudDeck checks your identity, then opens the EC2 fleet."),
                                    ),
                            )
                            .child(
                                panel()
                                    .id("account-context-panel")
                                    .debug_selector(|| "account-context-panel".into())
                                    .child(self.profile_selector(cx))
                                    .child(self.region_selector(cx))
                                    .child(self.profiles_hint()),
                            )
                            .child(self.connection_panel(cx)),
                    ),
            );

        if !state_busy && !state_connected {
            root = root.child(
                div()
                    .id("account-connect-actions")
                    .debug_selector(|| "account-connect-actions".into())
                    .flex()
                    .flex_row()
                    .justify_center()
                    .gap(px(10.0))
                    .px(px(24.0))
                    .py(px(16.0))
                    .bg(SurgicalTheme::surface_container())
                    .child(
                        selector("account-rescan")
                            .px(px(14.0))
                            .py(px(9.0))
                            .bg(SurgicalTheme::surface_container())
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(SurgicalTheme::text_primary())
                            .child("Rescan")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.request_rescan();
                                cx.notify();
                            })),
                    )
                    .child(
                        selector("account-sign-in")
                            .px(px(14.0))
                            .py(px(9.0))
                            .bg(SurgicalTheme::surface_container())
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(SurgicalTheme::text_primary())
                            .child("Sign in with browser")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.request_sign_in();
                                cx.notify();
                            })),
                    )
                    .child(
                        selector("account-connect")
                            .px(px(16.0))
                            .py(px(9.0))
                            .bg(if connect_enabled {
                                SurgicalTheme::primary()
                            } else {
                                SurgicalTheme::surface_high()
                            })
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(if connect_enabled {
                                SurgicalTheme::surface()
                            } else {
                                SurgicalTheme::text_secondary()
                            })
                            .child(primary_label)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.begin_connect();
                                cx.notify();
                            })),
                    )
                    .child(
                        selector("account-cancel")
                            .px(px(12.0))
                            .py(px(9.0))
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child("Cancel")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel();
                                cx.notify();
                                window.remove_window();
                            })),
                    ),
            );
        } else if state_connected {
            root = root.child(
                div()
                    .id("account-connected-actions")
                    .debug_selector(|| "account-connected-actions".into())
                    .flex()
                    .flex_row()
                    .justify_center()
                    .gap(px(10.0))
                    .px(px(24.0))
                    .py(px(16.0))
                    .bg(SurgicalTheme::surface_container())
                    .child(
                        selector("account-open-fleet")
                            .px(px(16.0))
                            .py(px(9.0))
                            .bg(SurgicalTheme::primary())
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(SurgicalTheme::surface())
                            .child(primary_label)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_fleet();
                                cx.notify();
                                if this.finished {
                                    window.remove_window();
                                }
                            })),
                    )
                    .child(
                        selector("account-cancel-connected")
                            .px(px(12.0))
                            .py(px(9.0))
                            .text_size(px(SurgicalTypography::BODY_SIZE))
                            .text_color(SurgicalTheme::text_secondary())
                            .child("Cancel")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.cancel();
                                cx.notify();
                                window.remove_window();
                            })),
                    ),
            );
        }

        // Keep the demo route visually separate from account setup. It does
        // not pretend to establish an AWS identity or choose a real region.
        root.child(
            div()
                .id("account-demo-action")
                .debug_selector(|| "account-demo-action".into())
                .flex()
                .justify_center()
                .px(px(24.0))
                .pb(px(12.0))
                .bg(SurgicalTheme::surface_container())
                .child(
                    selector("account-demo")
                        .text_size(px(SurgicalTypography::CAPTION_SIZE))
                        .text_color(SurgicalTheme::text_secondary())
                        .child("Explore demo fleet")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.request_demo();
                            cx.notify();
                            window.remove_window();
                        })),
                ),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    fn profile(name: &str, region: Option<&str>) -> clouddeck_core::AwsProfile {
        clouddeck_core::AwsProfile {
            name: name.into(),
            region: region.map(str::to_string),
        }
    }

    fn configured_view() -> AccountWindowView {
        AccountWindowView {
            profiles: Some(vec![
                profile("work", Some("eu-west-1")),
                profile("sandbox", None),
            ]),
            selected_profile: Some("work".into()),
            selected_region: "eu-west-1".into(),
            ..Default::default()
        }
    }

    #[test]
    fn regions_exclude_fleet_scope_and_deduplicate() {
        let view = AccountWindowView {
            available_regions: Some(vec![
                "fleet".into(),
                "us-east-1".into(),
                "us-east-1".into(),
                "eu-west-1".into(),
            ]),
            ..Default::default()
        };
        assert_eq!(view.region_options(), vec!["eu-west-1", "us-east-1"]);
    }

    #[test]
    fn profile_selection_prefers_configured_region_without_calling_it_an_account() {
        let mut view = AccountWindowView::default();
        view.select_profile("work".into(), Some("eu-west-1".into()));
        assert_eq!(view.selected_profile.as_deref(), Some("work"));
        assert_eq!(view.selected_region, "eu-west-1");
        assert!(matches!(
            view.connection_state,
            AccountConnectionState::Idle
        ));
    }

    #[test]
    fn missing_profile_stays_recoverable() {
        let mut view = AccountWindowView::default();
        view.begin_connect();
        assert_eq!(
            view.connection_state,
            AccountConnectionState::Failed(AccountConnectionError::MissingProfile)
        );
        view.profiles = Some(vec![profile("work", None)]);
        view.select_profile("work".into(), None);
        view.begin_connect();
        assert_eq!(view.connection_state, AccountConnectionState::Connecting);
        assert!(view.connect_requested);
    }

    #[test]
    fn browser_login_and_identity_states_are_distinct() {
        let mut view = configured_view();
        view.begin_connect();
        let attempt = view.connection_attempt;
        view.await_browser_login();
        assert_eq!(
            view.connection_state,
            AccountConnectionState::AwaitingBrowserLogin
        );
        assert!(view.identity_resolved_for_attempt(attempt, "123456789012"));
        assert_eq!(
            view.connection_state,
            AccountConnectionState::CheckingFleetAccess {
                account_id: "123456789012".into()
            }
        );
        assert!(view.complete_connection_for_attempt(attempt, "123456789012"));
        assert_eq!(
            view.connection_state,
            AccountConnectionState::Connected {
                account_id: "123456789012".into()
            }
        );
        assert!(!view.finished, "identity feedback should remain visible");
        view.open_fleet();
        assert!(view.finished);
    }

    #[test]
    fn cancelled_attempt_cannot_apply_a_late_identity() {
        let mut view = configured_view();
        view.begin_connect();
        let attempt = view.connection_attempt;
        view.cancel_browser_login();
        assert!(!view.complete_connection_for_attempt(attempt, "123456789012"));
        assert_eq!(view.connection_state, AccountConnectionState::Idle);
    }

    #[test]
    fn changing_scope_invalidates_late_success_and_error() {
        let mut view = configured_view();
        view.begin_connect();
        let attempt = view.connection_attempt;
        view.select_region("us-west-2".into());
        assert!(!view.complete_connection_for_attempt(attempt, "123456789012"));
        assert_eq!(view.connection_state, AccountConnectionState::Idle);
        assert!(!view.fail_connection_for_attempt(attempt, AccountConnectionError::Network));

        view.begin_connect();
        let attempt = view.connection_attempt;
        view.request_rescan();
        assert!(!view.fail_connection_for_attempt(attempt, AccountConnectionError::ExpiredLogin));
        assert_eq!(view.connection_state, AccountConnectionState::Idle);
    }

    #[test]
    fn app_scope_is_not_a_connectable_region() {
        let mut view = configured_view();
        view.selected_region = "fleet".into();
        view.begin_connect();
        assert_eq!(
            view.connection_state,
            AccountConnectionState::Failed(AccountConnectionError::MissingRegion)
        );
    }

    #[test]
    fn connection_failures_can_retry_or_rescan() {
        let mut view = configured_view();
        // An expired-login failure retries through the browser SSO flow
        // (clouddeck-rs-wz2.2), not a bare reconnect: `retry_connection`
        // routes `Failed(ExpiredLogin)` to `request_sign_in`, which puts the
        // view in `AwaitingBrowserLogin` and flags a sign-in request for the
        // composing shell/auth service to act on.
        view.fail_connection(AccountConnectionError::ExpiredLogin);
        view.retry_connection();
        assert_eq!(
            view.connection_state,
            AccountConnectionState::AwaitingBrowserLogin
        );
        assert!(view.sign_in_requested);

        // A non-expired failure (e.g. a transient network error) retries via
        // a plain reconnect instead.
        view.fail_connection(AccountConnectionError::Network);
        view.retry_connection();
        assert_eq!(view.connection_state, AccountConnectionState::Connecting);

        view.fail_connection(AccountConnectionError::Network);
        view.request_rescan();
        assert!(view.rescan_requested);
        assert_eq!(view.connection_state, AccountConnectionState::Idle);
    }

    #[gpui::test]
    fn mounts_compact_connect_screen_without_wizard_rail(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| configured_view());
        assert!(cx.debug_bounds("account-connect-screen").is_some());
        assert!(cx.debug_bounds("account-profile-selector").is_some());
        assert!(cx.debug_bounds("account-region-selector").is_some());
        assert!(cx.debug_bounds("account-connect").is_some());
        assert!(cx.debug_bounds("account-window-rail").is_none());
        assert!(
            cx.debug_bounds("account-window-step-provider-active")
                .is_none()
        );
    }

    #[gpui::test]
    fn profile_and_region_dropdowns_expand_as_needed(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| configured_view());
        let profile_bounds = cx.debug_bounds("account-profile-selector").unwrap();
        cx.simulate_click(profile_bounds.center(), Default::default());
        assert!(cx.debug_bounds("account-profile-menu").is_some());
        assert!(cx.debug_bounds("account-profile-option-sandbox").is_some());
        let region_bounds = cx.debug_bounds("account-region-selector").unwrap();
        cx.simulate_click(region_bounds.center(), Default::default());
        assert!(cx.debug_bounds("account-region-menu").is_some());
        assert!(cx.debug_bounds("account-region-option-us-east-1").is_some());
        view.update(cx, |view, _| {
            assert!(!view.profile_menu_open);
            assert!(view.region_menu_open);
        });
    }

    #[gpui::test]
    fn connect_requires_profile_and_does_not_close_window(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| AccountWindowView {
            profiles: Some(vec![]),
            ..Default::default()
        });
        let connect = cx.debug_bounds("account-connect").unwrap();
        cx.simulate_click(connect.center(), Default::default());
        view.update(cx, |view, _| {
            assert_eq!(
                view.connection_state,
                AccountConnectionState::Failed(AccountConnectionError::MissingProfile)
            );
            assert!(!view.finished);
        });
        assert!(cx.debug_bounds("account-connection-error").is_some());
    }
}
