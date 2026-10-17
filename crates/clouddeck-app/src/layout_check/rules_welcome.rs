//! Layout rules for the welcome window.

use super::rules::{Check, Rule};
use super::states::State;

/// Layout rules contributed by the welcome window area.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        // Big buttons
        Rule {
            id: "wel-connect-in",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::InWindow("welcome.connect"),
            why: "buttons visible",
            known_bug: None,
        },
        Rule {
            id: "wel-sso-in",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::InWindow("welcome.sso"),
            why: "buttons visible",
            known_bug: None,
        },
        Rule {
            id: "wel-demo-in",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::InWindow("welcome.demo"),
            why: "buttons visible",
            known_bug: None,
        },
        // Search and profile rows
        Rule {
            id: "wel-search-in",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::InWindow("welcome.search"),
            why: "search visible",
            known_bug: None,
        },
        Rule {
            id: "wel-search-above-rows",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::Above {
                upper: "welcome.search",
                lower: "welcome.profile.0",
            },
            why: "rows below the search",
            known_bug: None,
        },
        Rule {
            id: "wel-row-in",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::InWindow("welcome.profile.2"),
            why: "rows visible",
            known_bug: None,
        },
        Rule {
            id: "wel-buttons-vs-rows",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::NoOverlap("welcome.connect", "welcome.profile.0"),
            why: "left column and list do not overlap",
            known_bug: None,
        },
        // Smaller window
        Rule {
            id: "wel-small-in",
            state: State::Welcome,
            window: (640.0, 420.0),
            check: Check::InWindow("welcome.demo"),
            why: "smaller window still shows the buttons",
            known_bug: None,
        },
    ]
}
