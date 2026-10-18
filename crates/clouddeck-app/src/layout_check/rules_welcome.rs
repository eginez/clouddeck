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
        // Add-profile button left of the search field
        Rule {
            id: "wel-add-present",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::Present("welcome.add"),
            why: "add-profile button shown beside the search",
            known_bug: None,
        },
        Rule {
            id: "wel-add-nonzero",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::NonZero("welcome.add"),
            why: "add-profile button has a size",
            known_bug: None,
        },
        Rule {
            id: "wel-add-in",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::InWindow("welcome.add"),
            why: "add-profile button visible",
            known_bug: None,
        },
        Rule {
            id: "wel-add-w",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::WidthBetween("welcome.add", 20.0, 32.0),
            why: "add-profile button is an icon button",
            known_bug: None,
        },
        Rule {
            id: "wel-add-left-of-search",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::LeftOf("welcome.add", "welcome.searchbox"),
            why: "add-profile button sits left of the search field, not over it",
            known_bug: None,
        },
        Rule {
            id: "wel-add-same-row",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::SameRow("welcome.add", "welcome.searchbox"),
            why: "add-profile button is vertically aligned with the search field",
            known_bug: None,
        },
        Rule {
            id: "wel-search-fills-row",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::RightAligned("welcome.searchbox", "welcome.searchrow"),
            why: "search field runs to the right edge of the row",
            known_bug: None,
        },
        Rule {
            id: "wel-add-starts-row",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::LeftAligned("welcome.add", "welcome.searchrow"),
            why: "add-profile button starts the search row",
            known_bug: None,
        },
        Rule {
            id: "wel-add-above-rows",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::Above {
                upper: "welcome.add",
                lower: "welcome.profile.0",
            },
            why: "add-profile button above the rows",
            known_bug: None,
        },
        Rule {
            id: "wel-add-1280",
            state: State::Welcome,
            window: (1280.0, 760.0),
            check: Check::LeftOf("welcome.add", "welcome.searchbox"),
            why: "add-profile button sits left of the search field at the large size",
            known_bug: None,
        },
        Rule {
            id: "wel-add-same-row-1280",
            state: State::Welcome,
            window: (1280.0, 760.0),
            check: Check::SameRow("welcome.add", "welcome.searchbox"),
            why: "add-profile button is vertically aligned with the search field at the large size",
            known_bug: None,
        },
        Rule {
            id: "wel-add-vs-connect",
            state: State::Welcome,
            window: (800.0, 460.0),
            check: Check::LeftOf("welcome.connect", "welcome.add"),
            why: "add-profile button lies in the right pane, right of the left column",
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
