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
    ]
    .into_iter()
    .flatten()
    .collect()
}
