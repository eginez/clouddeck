//! Text and colour rules.

use super::rules::{Check, Rule};
use super::states::State;

/// Text, colour token and font rules over the recorded render output.
pub(crate) fn rules() -> Vec<Rule> {
    const MAIN: (f32, f32) = (1280.0, 760.0);
    vec![
        Rule {
            id: "sty-status-ready",
            state: State::MainIdle,
            window: MAIN,
            check: Check::TextIs {
                target: "bar.status",
                text: "Ready",
            },
            why: "idle status text",
            known_bug: None,
        },
        Rule {
            id: "sty-count",
            state: State::MainIdle,
            window: MAIN,
            check: Check::TextIs {
                target: "bar.count",
                text: "3 instances",
            },
            why: "SmallHappyPath has 3 instances",
            known_bug: None,
        },
        Rule {
            id: "sty-header-0",
            state: State::MainIdle,
            window: MAIN,
            check: Check::TextIs {
                target: "table.header.0",
                text: "Name",
            },
            why: "column title",
            known_bug: None,
        },
        Rule {
            id: "sty-header-1",
            state: State::MainIdle,
            window: MAIN,
            check: Check::TextIs {
                target: "table.header.1",
                text: "Instance ID",
            },
            why: "column title",
            known_bug: None,
        },
        Rule {
            id: "sty-id-mono",
            state: State::MainIdle,
            window: MAIN,
            check: Check::FontIs {
                target: "table.row.us-east-1:i-scenario-001.id",
                font: "mono",
            },
            why: "IDs in the mono font",
            known_bug: None,
        },
        Rule {
            id: "sty-running-green",
            state: State::MainIdle,
            window: MAIN,
            check: Check::FgIs {
                target: "table.row.us-east-1:i-scenario-001.state",
                token: "runtext",
            },
            why: "running is green text",
            known_bug: None,
        },
        Rule {
            id: "sty-stopped-neutral",
            state: State::MainIdle,
            window: MAIN,
            check: Check::FgIs {
                target: "table.row.us-east-1:i-scenario-002.state",
                token: "dim",
            },
            why: "stopped is neutral, not red (design brief)",
            known_bug: None,
        },
        Rule {
            id: "sty-failure-amber",
            state: State::MainPartial,
            window: MAIN,
            check: Check::FgIs {
                target: "bar.failure",
                token: "warntext",
            },
            why: "failure item amber",
            known_bug: None,
        },
        Rule {
            id: "sty-syntax-hint",
            state: State::SearchFocusedEmpty,
            window: MAIN,
            check: Check::TextIs {
                target: "search.syntax-hint",
                text: "Filter with field:value · -not · a,b either · * wildcard",
            },
            why: "copy per mockup",
            known_bug: None,
        },
        Rule {
            id: "sty-center-error",
            state: State::MainFailure,
            window: MAIN,
            check: Check::TextIs {
                target: "center.title",
                text: "Couldn’t load instances",
            },
            why: "U+2019 apostrophe",
            known_bug: None,
        },
    ]
}
