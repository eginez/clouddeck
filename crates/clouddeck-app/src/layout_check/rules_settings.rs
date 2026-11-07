//! Layout rules for the Settings window.

use super::rules::{Check, Rule};
use super::states::State;

const WINDOW: (f32, f32) = crate::settings_window::WINDOW_SIZE;
const NARROW_WINDOW: (f32, f32) = (640.0, 560.0);

macro_rules! footer_rules {
    ($state:expr, $window:expr, $prefix:literal) => {
        vec![
            Rule {
                id: concat!($prefix, "-done-visible"),
                state: $state,
                window: $window,
                check: Check::InWindow("settings.done"),
                why: "Done is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-restore-visible"),
                state: $state,
                window: $window,
                check: Check::InWindow("settings.restore"),
                why: "Restore Defaults is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-done-in-footer"),
                state: $state,
                window: $window,
                check: Check::Inside {
                    inner: "settings.done",
                    outer: "settings.footer",
                },
                why: "Done is inside the footer, not clipped past it",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-hint-has-width"),
                state: $state,
                window: $window,
                check: Check::NonZero("settings.hint"),
                why: "the helper text is not collapsed to nothing to make the buttons fit",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-buttons-order"),
                state: $state,
                window: $window,
                check: Check::LeftOf("settings.restore", "settings.done"),
                why: "Restore Defaults and Done do not overlap",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-hint-before-buttons"),
                state: $state,
                window: $window,
                check: Check::LeftOf("settings.hint", "settings.restore"),
                why: "the helper text never runs into the buttons",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-hint-in-footer"),
                state: $state,
                window: $window,
                check: Check::Inside {
                    inner: "settings.hint",
                    outer: "settings.footer",
                },
                why: "the helper text is clipped inside the footer",
                known_bug: None,
            },
        ]
    };
}

macro_rules! controls_rules {
    ($state:expr, $window:expr, $prefix:literal, [$($selector:literal),* $(,)?]) => {
        vec![$(
            Rule {
                id: concat!($prefix, "-", $selector),
                state: $state,
                window: $window,
                check: Check::InWindow($selector),
                why: "every setting control of the extension category is fully inside the window",
                known_bug: None,
            },
        )*]
    };
}

pub(crate) fn rules() -> Vec<Rule> {
    let mut rules = Vec::new();
    rules.extend(footer_rules!(
        State::SettingsLight,
        WINDOW,
        "settings-light"
    ));
    rules.extend(footer_rules!(State::SettingsDark, WINDOW, "settings-dark"));
    rules.extend(footer_rules!(
        State::SettingsNarrow,
        NARROW_WINDOW,
        "settings-narrow"
    ));
    rules.extend(footer_rules!(
        State::SettingsNarrowDark,
        NARROW_WINDOW,
        "settings-narrow-dark"
    ));
    rules.extend(footer_rules!(
        State::ProviderSettingsLight,
        WINDOW,
        "provider-settings-light"
    ));
    rules.extend(footer_rules!(
        State::ProviderSettingsDark,
        WINDOW,
        "provider-settings-dark"
    ));
    rules.extend(footer_rules!(
        State::ProviderSettingsChanged,
        WINDOW,
        "provider-settings-changed"
    ));
    rules.extend(footer_rules!(
        State::ProviderSettingsInvalid,
        WINDOW,
        "provider-settings-invalid"
    ));
    rules.extend(footer_rules!(
        State::ProviderSettingsSearch,
        WINDOW,
        "provider-settings-search"
    ));
    rules.extend(controls_rules!(
        State::ProviderSettingsLight,
        WINDOW,
        "provider-settings-light",
        [
            "settings.provider.control.sample.region_label",
            "settings.provider.control.sample.config_path",
            "settings.provider.control.sample.verbose",
            "settings.provider.control.sample.retries",
            "settings.provider.control.sample.timeout_secs",
            "settings.provider.control.sample.mode",
        ]
    ));
    rules.extend(controls_rules!(
        State::ProviderSettingsDark,
        WINDOW,
        "provider-settings-dark",
        [
            "settings.provider.control.sample.region_label",
            "settings.provider.control.sample.config_path",
            "settings.provider.control.sample.verbose",
            "settings.provider.control.sample.retries",
            "settings.provider.control.sample.timeout_secs",
            "settings.provider.control.sample.mode",
        ]
    ));
    rules.push(Rule {
        id: "provider-settings-light-secret-rendered",
        state: State::ProviderSettingsLight,
        window: WINDOW,
        check: Check::Present("settings.provider.control.sample.token"),
        why: "the secret field is drawn; it sits below the fold, so the pane scrolls to it",
        known_bug: None,
    });
    rules.push(Rule {
        id: "provider-settings-dark-secret-rendered",
        state: State::ProviderSettingsDark,
        window: WINDOW,
        check: Check::Present("settings.provider.control.sample.token"),
        why: "the secret field is drawn; it sits below the fold, so the pane scrolls to it",
        known_bug: None,
    });
    rules.push(Rule {
        id: "provider-settings-changed-reset-visible",
        state: State::ProviderSettingsChanged,
        window: WINDOW,
        check: Check::InWindow("settings.provider.reset.sample.region_label"),
        why: "the Reset link for a changed setting is fully inside the window",
        known_bug: None,
    });
    rules.push(Rule {
        id: "provider-settings-invalid-error-in-row",
        state: State::ProviderSettingsInvalid,
        window: WINDOW,
        check: Check::Inside {
            inner: "settings.provider.error.sample.retries",
            outer: "settings.provider.item.sample.retries",
        },
        why: "the error for an out-of-range value is drawn inside its setting row",
        known_bug: None,
    });
    rules.push(Rule {
        id: "provider-settings-search-hit-visible",
        state: State::ProviderSettingsSearch,
        window: WINDOW,
        check: Check::InWindow("settings.provider.item.sample.timeout_secs"),
        why: "a search hit in an extension category is fully inside the window",
        known_bug: None,
    });
    rules
}
