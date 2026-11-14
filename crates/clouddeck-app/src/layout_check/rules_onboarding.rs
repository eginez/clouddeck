//! Layout rules for the onboarding window.

use super::rules::{Check, Rule};
use super::states::State;

const WINDOW: (f32, f32) = crate::onboarding_window::WINDOW_SIZE;
const NARROW: (f32, f32) = (720.0, 520.0);

macro_rules! common_rules {
    ($state:expr, $prefix:literal, $window:expr) => {
        vec![
            Rule {
                id: concat!($prefix, "-rail-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("onboarding.rail"),
                why: "the provider rail is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-pane-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("onboarding.pane"),
                why: "the pane is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-rail-left-of-pane"),
                state: $state,
                window: $window,
                check: Check::LeftOf("onboarding.rail", "onboarding.pane"),
                why: "the rail sits left of the pane without overlapping it",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-provider-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("onboarding.provider.aws"),
                why: "the selected provider row is fully inside the window",
                known_bug: None,
            },
        ]
    };
}

macro_rules! listing_rules {
    ($state:expr, $prefix:literal, $window:expr) => {
        vec![
            Rule {
                id: concat!($prefix, "-search-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("onboarding.search"),
                why: "the search field is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-search-above-rows"),
                state: $state,
                window: $window,
                check: Check::Above {
                    upper: "onboarding.search",
                    lower: "onboarding.rows",
                },
                why: "the search field sits above the connection list",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-rows-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("onboarding.rows"),
                why: "the connection list is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-row-default-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("onboarding.row.default"),
                why: "the first connection row is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-row-staging-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("onboarding.row.staging"),
                why: "the second connection row is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-rows-stacked"),
                state: $state,
                window: $window,
                check: Check::Above {
                    upper: "onboarding.row.default",
                    lower: "onboarding.row.staging",
                },
                why: "the connection rows are listed one above the other",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-rows-left-aligned"),
                state: $state,
                window: $window,
                check: Check::LeftAligned("onboarding.row.default", "onboarding.row.staging"),
                why: "the connection rows share a left edge",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-footer-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("onboarding.footer"),
                why: "the footer is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-rows-above-footer"),
                state: $state,
                window: $window,
                check: Check::Above {
                    upper: "onboarding.rows",
                    lower: "onboarding.footer",
                },
                why: "the footer sits below the connection list",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-open-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("onboarding.open"),
                why: "the Open button is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-open-has-width"),
                state: $state,
                window: $window,
                check: Check::NonZero("onboarding.open"),
                why: "the Open button is not collapsed to nothing",
                known_bug: None,
            },
        ]
    };
}

macro_rules! centred_rules {
    ($state:expr, $prefix:literal, $window:expr, $panel:literal, $action:literal) => {
        vec![
            Rule {
                id: concat!($prefix, "-panel-in"),
                state: $state,
                window: $window,
                check: Check::InWindow($panel),
                why: "the centred message is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-panel-has-width"),
                state: $state,
                window: $window,
                check: Check::NonZero($panel),
                why: "the centred message is not collapsed to nothing",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-action-in"),
                state: $state,
                window: $window,
                check: Check::Inside {
                    inner: $action,
                    outer: $panel,
                },
                why: "the recovery button sits inside the centred message",
                known_bug: None,
            },
        ]
    };
}

/// Layout rules contributed by the onboarding window.
pub(crate) fn rules() -> Vec<Rule> {
    [
        common_rules!(State::OnboardingReadyLight, "onb-ready-light", WINDOW),
        common_rules!(State::OnboardingReadyDark, "onb-ready-dark", WINDOW),
        common_rules!(State::OnboardingReadyNarrow, "onb-ready-narrow", NARROW),
        listing_rules!(State::OnboardingReadyLight, "onb-ready-light", WINDOW),
        listing_rules!(State::OnboardingReadyDark, "onb-ready-dark", WINDOW),
        listing_rules!(State::OnboardingReadyNarrow, "onb-ready-narrow", NARROW),
        centred_rules!(
            State::OnboardingEmpty,
            "onb-empty",
            WINDOW,
            "onboarding.empty",
            "onboarding.check-again"
        ),
        centred_rules!(
            State::OnboardingError,
            "onb-error",
            WINDOW,
            "onboarding.failed",
            "onboarding.check-again"
        ),
        centred_rules!(
            State::OnboardingOpening,
            "onb-opening",
            WINDOW,
            "onboarding.opening",
            "onboarding.back"
        ),
        common_rules!(State::OnboardingDetecting, "onb-detecting", WINDOW),
        common_rules!(State::OnboardingEmpty, "onb-empty", WINDOW),
        common_rules!(State::OnboardingError, "onb-error", WINDOW),
        common_rules!(State::OnboardingOpening, "onb-opening", WINDOW),
        common_rules!(State::OnboardingExtensionReady, "onb-ext-ready", WINDOW),
        listing_rules!(State::OnboardingExtensionReady, "onb-ext-ready", WINDOW),
        common_rules!(State::OnboardingMethods, "onb-methods", WINDOW),
        common_rules!(State::OnboardingPrompt, "onb-prompt", WINDOW),
        common_rules!(State::OnboardingBrowser, "onb-browser", WINDOW),
        common_rules!(State::OnboardingDisabled, "onb-disabled", WINDOW),
        common_rules!(State::OnboardingDemo, "onb-demo", WINDOW),
        common_rules!(State::OnboardingAddProvider, "onb-add", WINDOW),
        vec![
            Rule {
                id: "onb-ext-ready-header-in",
                state: State::OnboardingExtensionReady,
                window: WINDOW,
                check: Check::InWindow("onboarding.header"),
                why: "the header Connect button is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: "onb-ext-ready-header-has-width",
                state: State::OnboardingExtensionReady,
                window: WINDOW,
                check: Check::NonZero("onboarding.header"),
                why: "the header Connect button is not collapsed to nothing",
                known_bug: None,
            },
            Rule {
                id: "onb-methods-cards-in",
                state: State::OnboardingMethods,
                window: WINDOW,
                check: Check::InWindow("onboarding.methods"),
                why: "the sign-in method cards are fully inside the window",
                known_bug: None,
            },
            Rule {
                id: "onb-methods-first-above-second",
                state: State::OnboardingMethods,
                window: WINDOW,
                check: Check::Above {
                    upper: "onboarding.method.api-token",
                    lower: "onboarding.method.browser",
                },
                why: "the method cards are listed one above the other",
                known_bug: None,
            },
            Rule {
                id: "onb-methods-cards-left-aligned",
                state: State::OnboardingMethods,
                window: WINDOW,
                check: Check::LeftAligned(
                    "onboarding.method.api-token",
                    "onboarding.method.browser",
                ),
                why: "the method cards share a left edge",
                known_bug: None,
            },
            Rule {
                id: "onb-methods-back-in-footer",
                state: State::OnboardingMethods,
                window: WINDOW,
                check: Check::Inside {
                    inner: "onboarding.choose-back",
                    outer: "onboarding.footer",
                },
                why: "the Back button sits in the footer",
                known_bug: None,
            },
            Rule {
                id: "onb-prompt-fields-in",
                state: State::OnboardingPrompt,
                window: WINDOW,
                check: Check::InWindow("onboarding.prompt"),
                why: "the sign-in fields are fully inside the window",
                known_bug: None,
            },
            Rule {
                id: "onb-prompt-connect-in-footer",
                state: State::OnboardingPrompt,
                window: WINDOW,
                check: Check::Inside {
                    inner: "onboarding.connect",
                    outer: "onboarding.footer",
                },
                why: "the Connect button sits in the footer",
                known_bug: None,
            },
            Rule {
                id: "onb-prompt-connect-has-width",
                state: State::OnboardingPrompt,
                window: WINDOW,
                check: Check::NonZero("onboarding.connect"),
                why: "the Connect button is not collapsed to nothing",
                known_bug: None,
            },
            Rule {
                id: "onb-browser-panel-in",
                state: State::OnboardingBrowser,
                window: WINDOW,
                check: Check::InWindow("onboarding.browser-panel"),
                why: "the browser sign-in step is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: "onb-browser-open-again-in-panel",
                state: State::OnboardingBrowser,
                window: WINDOW,
                check: Check::Inside {
                    inner: "onboarding.open-again",
                    outer: "onboarding.browser-panel",
                },
                why: "the Open again link sits in the browser step",
                known_bug: None,
            },
            Rule {
                id: "onb-disabled-row-in",
                state: State::OnboardingDisabled,
                window: WINDOW,
                check: Check::InWindow("onboarding.disabled.toy"),
                why: "the disabled extension row is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: "onb-disabled-manage-in-panel",
                state: State::OnboardingDisabled,
                window: WINDOW,
                check: Check::Inside {
                    inner: "onboarding.manage-pane",
                    outer: "onboarding.disabled",
                },
                why: "the Manage extensions button sits in the disabled message",
                known_bug: None,
            },
            Rule {
                id: "onb-demo-card-in",
                state: State::OnboardingDemo,
                window: WINDOW,
                check: Check::InWindow("onboarding.demo-card"),
                why: "the demo fleet card is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: "onb-demo-open-in-card",
                state: State::OnboardingDemo,
                window: WINDOW,
                check: Check::Inside {
                    inner: "onboarding.open-demo",
                    outer: "onboarding.demo-card",
                },
                why: "the Open demo fleet button sits in the card",
                known_bug: None,
            },
            Rule {
                id: "onb-add-command-in",
                state: State::OnboardingAddProvider,
                window: WINDOW,
                check: Check::InWindow("onboarding.add-command"),
                why: "the install command is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: "onb-add-command-has-width",
                state: State::OnboardingAddProvider,
                window: WINDOW,
                check: Check::NonZero("onboarding.add-command"),
                why: "the install command is not collapsed to nothing",
                known_bug: None,
            },
            Rule {
                id: "onb-add-button-in-rail",
                state: State::OnboardingAddProvider,
                window: WINDOW,
                check: Check::Inside {
                    inner: "onboarding.add-provider",
                    outer: "onboarding.rail",
                },
                why: "the + Add provider button sits in the rail",
                known_bug: None,
            },
        ],
        vec![
            Rule {
                id: "onb-ready-light-identity-in",
                state: State::OnboardingReadyLight,
                window: WINDOW,
                check: Check::InWindow("onboarding.identity"),
                why: "the signed-in identity banner is fully inside the window",
                known_bug: None,
            },
            Rule {
                id: "onb-ready-light-identity-above-search",
                state: State::OnboardingReadyLight,
                window: WINDOW,
                check: Check::Above {
                    upper: "onboarding.identity",
                    lower: "onboarding.search",
                },
                why: "the identity banner sits above the search field",
                known_bug: None,
            },
        ],
    ]
    .into_iter()
    .flatten()
    .collect()
}
