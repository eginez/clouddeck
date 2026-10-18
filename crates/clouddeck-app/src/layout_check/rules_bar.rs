//! Layout rules for the status bar.

use super::rules::{Check, Rule};
use super::states::State;

/// Layout rules contributed by the status bar area.
pub(crate) fn rules() -> Vec<Rule> {
    const MAIN: (f32, f32) = (1280.0, 760.0);
    const COMPACT: (f32, f32) = (520.0, 760.0);
    const NARROW: (f32, f32) = (700.0, 760.0);
    vec![
        Rule {
            id: "bar-aws-in",
            state: State::MainIdle,
            window: MAIN,
            check: Check::Inside {
                inner: "bar.aws",
                outer: "main.statusbar",
            },
            why: "items inside the bar",
            known_bug: None,
        },
        Rule {
            id: "bar-profile-in",
            state: State::MainIdle,
            window: MAIN,
            check: Check::Inside {
                inner: "bar.profile",
                outer: "main.statusbar",
            },
            why: "items inside the bar",
            known_bug: None,
        },
        Rule {
            id: "bar-region-in",
            state: State::MainIdle,
            window: MAIN,
            check: Check::Inside {
                inner: "bar.region",
                outer: "main.statusbar",
            },
            why: "items inside the bar",
            known_bug: None,
        },
        Rule {
            id: "bar-status-in",
            state: State::MainIdle,
            window: MAIN,
            check: Check::Inside {
                inner: "bar.status",
                outer: "main.statusbar",
            },
            why: "items inside the bar",
            known_bug: None,
        },
        Rule {
            id: "bar-count-in",
            state: State::MainIdle,
            window: MAIN,
            check: Check::Inside {
                inner: "bar.count",
                outer: "main.statusbar",
            },
            why: "items inside the bar",
            known_bug: None,
        },
        Rule {
            id: "bar-aws-vs-profile",
            state: State::MainIdle,
            window: MAIN,
            check: Check::NoOverlap("bar.aws", "bar.profile"),
            why: "no collisions",
            known_bug: None,
        },
        Rule {
            id: "bar-profile-vs-region",
            state: State::MainIdle,
            window: MAIN,
            check: Check::NoOverlap("bar.profile", "bar.region"),
            why: "no collisions",
            known_bug: None,
        },
        Rule {
            id: "bar-region-vs-count",
            state: State::MainIdle,
            window: MAIN,
            check: Check::NoOverlap("bar.region", "bar.count"),
            why: "no collisions",
            known_bug: None,
        },
        Rule {
            id: "bar-refresh-vs-status",
            state: State::MainIdle,
            window: MAIN,
            check: Check::NoOverlap("bar.refresh", "bar.status"),
            why: "no collisions",
            known_bug: None,
        },
        Rule {
            id: "bar-700-no-updated",
            state: State::MainIdle,
            window: NARROW,
            check: Check::Absent("bar.updated"),
            why: "Updated hides below 720",
            known_bug: None,
        },
        Rule {
            id: "bar-520-profile",
            state: State::MainIdle,
            window: COMPACT,
            check: Check::Inside {
                inner: "bar.profile",
                outer: "main.statusbar",
            },
            why: "compact keeps profile inside",
            known_bug: None,
        },
        Rule {
            id: "bar-520-region",
            state: State::MainIdle,
            window: COMPACT,
            check: Check::Inside {
                inner: "bar.region",
                outer: "main.statusbar",
            },
            why: "compact keeps region inside",
            known_bug: None,
        },
        Rule {
            id: "bar-520-status",
            state: State::MainIdle,
            window: COMPACT,
            check: Check::Inside {
                inner: "bar.status",
                outer: "main.statusbar",
            },
            why: "compact keeps status inside",
            known_bug: None,
        },
        Rule {
            id: "bar-520-profile-vs-region",
            state: State::MainIdle,
            window: COMPACT,
            check: Check::NoOverlap("bar.profile", "bar.region"),
            why: "compact items do not collide",
            known_bug: None,
        },
        Rule {
            id: "bar-520-region-vs-status",
            state: State::MainIdle,
            window: COMPACT,
            check: Check::NoOverlap("bar.region", "bar.status"),
            why: "compact items do not collide",
            known_bug: None,
        },
        Rule {
            id: "bar-520-no-count",
            state: State::MainIdle,
            window: COMPACT,
            check: Check::Absent("bar.count"),
            why: "counts hide when compact",
            known_bug: None,
        },
        Rule {
            id: "bar-520-no-refresh",
            state: State::MainIdle,
            window: COMPACT,
            check: Check::Absent("bar.refresh"),
            why: "buttons hide when compact",
            known_bug: None,
        },
        Rule {
            id: "bar-fail-in",
            state: State::MainPartial,
            window: MAIN,
            check: Check::Inside {
                inner: "bar.failure",
                outer: "main.statusbar",
            },
            why: "failure item inside the bar",
            known_bug: None,
        },
        Rule {
            id: "bar-fail-vs-region",
            state: State::MainPartial,
            window: MAIN,
            check: Check::NoOverlap("bar.failure", "bar.region"),
            why: "failure item inside the bar",
            known_bug: None,
        },
        Rule {
            id: "bar-loadfail-in",
            state: State::MainFailure,
            window: MAIN,
            check: Check::InWindow("bar.status"),
            why: "load-failure status survives in the window",
            known_bug: None,
        },
        Rule {
            id: "bar-520-fail-in",
            state: State::MainPartial,
            window: COMPACT,
            check: Check::Inside {
                inner: "bar.failure",
                outer: "main.statusbar",
            },
            why: "failure item survives compact",
            known_bug: None,
        },
    ]
}
