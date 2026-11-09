//! Layout rules for the Connect window.

use super::rules::{Check, Rule};
use super::states::State;

const WINDOW: (f32, f32) = crate::connect_window::WINDOW_SIZE;

macro_rules! footer_rules {
    ($state:expr, $prefix:literal) => {
        vec![
            Rule {
                id: concat!($prefix, "-connect-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("connect.connect"),
                why: "Connect is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-cancel-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("connect.cancel"),
                why: "Cancel is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-demo-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("connect.demo"),
                why: "the demo link is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-buttons-row"),
                state: $state,
                window: WINDOW,
                check: Check::SameRow("connect.connect", "connect.cancel"),
                why: "Connect and Cancel sit on one row",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-cancel-right"),
                state: $state,
                window: WINDOW,
                check: Check::LeftOf("connect.connect", "connect.cancel"),
                why: "Connect and Cancel do not overlap",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-demo-below"),
                state: $state,
                window: WINDOW,
                check: Check::Above {
                    upper: "connect.connect",
                    lower: "connect.demo",
                },
                why: "the demo link sits under the buttons",
                known_bug: None,
            },
        ]
    };
}

macro_rules! folder_rules {
    ($state:expr, $prefix:literal) => {
        vec![
            Rule {
                id: concat!($prefix, "-browse-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("connect.browse.config_dir"),
                why: "Browse is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-browse-has-width"),
                state: $state,
                window: WINDOW,
                check: Check::NonZero("connect.browse.config_dir"),
                why: "Browse is not collapsed to nothing",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-folder-control-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("connect.control.config_dir"),
                why: "the folder field is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-row-below-provider"),
                state: $state,
                window: WINDOW,
                check: Check::Above {
                    upper: "connect.provider.fixture",
                    lower: "connect.row.config_dir",
                },
                why: "the settings rows follow the provider choice",
                known_bug: None,
            },
        ]
    };
}

macro_rules! error_rules {
    ($state:expr, $prefix:literal) => {
        vec![
            Rule {
                id: concat!($prefix, "-error-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("connect.error"),
                why: "the error message is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-error-has-width"),
                state: $state,
                window: WINDOW,
                check: Check::NonZero("connect.error"),
                why: "the error message is not collapsed to nothing",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-error-below-rows"),
                state: $state,
                window: WINDOW,
                check: Check::Above {
                    upper: "connect.row.region",
                    lower: "connect.error",
                },
                why: "the error follows the settings rows",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-error-above-connect"),
                state: $state,
                window: WINDOW,
                check: Check::Above {
                    upper: "connect.error",
                    lower: "connect.connect",
                },
                why: "the error sits above the buttons it relates to",
                known_bug: None,
            },
        ]
    };
}

macro_rules! device_rules {
    ($state:expr, $prefix:literal) => {
        vec![
            Rule {
                id: concat!($prefix, "-code-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("connect.code"),
                why: "the device code is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-code-has-width"),
                state: $state,
                window: WINDOW,
                check: Check::NonZero("connect.code"),
                why: "the device code is not collapsed to nothing",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-code-above-connect"),
                state: $state,
                window: WINDOW,
                check: Check::Above {
                    upper: "connect.code",
                    lower: "connect.connect",
                },
                why: "the code sits above the buttons",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-open-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("connect.open"),
                why: "Open sign-in page is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-copy-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("connect.copy"),
                why: "Copy code is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-open-copy-row"),
                state: $state,
                window: WINDOW,
                check: Check::SameRow("connect.open", "connect.copy"),
                why: "the two device links sit on one row",
                known_bug: None,
            },
        ]
    };
}

macro_rules! methods_rules {
    ($state:expr, $prefix:literal) => {
        vec![
            Rule {
                id: concat!($prefix, "-method-0-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("connect.method.0"),
                why: "the first auth method is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-method-1-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("connect.method.1"),
                why: "the second auth method is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-methods-stacked"),
                state: $state,
                window: WINDOW,
                check: Check::Above {
                    upper: "connect.method.0",
                    lower: "connect.method.1",
                },
                why: "the auth methods are listed one above the other",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-methods-left-aligned"),
                state: $state,
                window: WINDOW,
                check: Check::LeftAligned("connect.method.0", "connect.method.1"),
                why: "the auth method radios share a left edge",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-method-1-above-rows"),
                state: $state,
                window: WINDOW,
                check: Check::Above {
                    upper: "connect.method.1",
                    lower: "connect.row.region",
                },
                why: "the method list sits above the selected method's settings",
                known_bug: None,
            },
        ]
    };
}

/// Layout rules contributed by the Connect window area.
pub(crate) fn rules() -> Vec<Rule> {
    [
        footer_rules!(State::ConnectFolderLight, "conn-folder-light"),
        footer_rules!(State::ConnectFolderDark, "conn-folder-dark"),
        footer_rules!(State::ConnectErrorLight, "conn-error-light"),
        footer_rules!(State::ConnectErrorDark, "conn-error-dark"),
        footer_rules!(State::ConnectDeviceCodeLight, "conn-device-light"),
        footer_rules!(State::ConnectDeviceCodeDark, "conn-device-dark"),
        footer_rules!(State::ConnectTwoMethodsLight, "conn-methods-light"),
        footer_rules!(State::ConnectTwoMethodsDark, "conn-methods-dark"),
        folder_rules!(State::ConnectFolderLight, "conn-folder-light"),
        folder_rules!(State::ConnectFolderDark, "conn-folder-dark"),
        error_rules!(State::ConnectErrorLight, "conn-error-light"),
        error_rules!(State::ConnectErrorDark, "conn-error-dark"),
        device_rules!(State::ConnectDeviceCodeLight, "conn-device-light"),
        device_rules!(State::ConnectDeviceCodeDark, "conn-device-dark"),
        methods_rules!(State::ConnectTwoMethodsLight, "conn-methods-light"),
        methods_rules!(State::ConnectTwoMethodsDark, "conn-methods-dark"),
    ]
    .into_iter()
    .flatten()
    .collect()
}
