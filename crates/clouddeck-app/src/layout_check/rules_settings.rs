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
    rules
}
