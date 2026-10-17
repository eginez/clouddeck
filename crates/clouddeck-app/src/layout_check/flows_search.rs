//! Scripted flows for search.

use super::rules::Check;
use super::scripts::{Expect, Script, Step};
use super::states::State;

const ROW_SELECTOR: &str = "table.row.us-east-1:i-scenario-001";

/// The search flows: typing, accepting and clearing a query, facets, and narrow-window panes.
pub(crate) fn scripts() -> Vec<Script> {
    vec![
        Script {
            id: "flow-search-accept",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Keys("cmd-f"),
                    vec![Expect::Layout(Check::Present("search.field"))],
                ),
                (
                    Step::Type("st"),
                    vec![
                        Expect::Layout(Check::Present("search.suggestions")),
                        Expect::Layout(Check::InWindow("search.suggestions")),
                    ],
                ),
                (Step::Keys("enter"), vec![Expect::Query("state:")]),
            ],
            why: "typing a partial field shows suggestions and enter accepts the field",
            known_bug: None,
        },
        Script {
            id: "flow-search-value",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Keys("cmd-f"),
                    vec![Expect::Layout(Check::Present("search.field"))],
                ),
                (
                    Step::Type("state:s"),
                    vec![Expect::Layout(Check::Present("search.suggestion.0"))],
                ),
                (Step::Keys("tab"), vec![Expect::Query("state:stopped ")]),
            ],
            why: "tab accepts the first value suggestion and leaves a trailing space",
            known_bug: None,
        },
        Script {
            id: "flow-search-esc",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Keys("cmd-f"),
                    vec![Expect::Layout(Check::Present("search.field"))],
                ),
                (Step::Type("web"), vec![Expect::Query("web")]),
                (Step::Keys("escape"), vec![Expect::Query("")]),
            ],
            why: "Esc clears the typed query",
            known_bug: None,
        },
        Script {
            id: "flow-facet",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Click("sidebar.state.stopped"),
                    vec![Expect::Query("state:stopped")],
                ),
                (Step::Click("sidebar.state.stopped"), vec![Expect::Query("")]),
            ],
            why: "clicking a sidebar facet sets the query and clicking it again clears the query",
            known_bug: None,
        },
        Script {
            id: "flow-narrow-closes",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Resize(700.0, 760.0),
                    vec![
                        Expect::Panes {
                            sidebar: false,
                            inspector: false,
                        },
                        Expect::Layout(Check::Present("main.table")),
                    ],
                ),
                (
                    Step::Resize(1280.0, 760.0),
                    vec![Expect::Panes {
                        sidebar: true,
                        inspector: false,
                    }],
                ),
            ],
            why: "narrowing the window closes the sidebar and widening it restores the sidebar",
            known_bug: None,
        },
        Script {
            id: "flow-narrow-inspector",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Click(ROW_SELECTOR),
                    vec![Expect::Panes {
                        sidebar: true,
                        inspector: true,
                    }],
                ),
                (
                    Step::Resize(700.0, 760.0),
                    vec![Expect::Panes {
                        sidebar: false,
                        inspector: false,
                    }],
                ),
            ],
            why: "selecting a row opens the inspector and crossing the breakpoint closes both side panes",
            known_bug: None,
        },
        Script {
            id: "flow-recent-narrow",
            start: State::SearchFocusedEmpty,
            window: (1280.0, 760.0),
            steps: vec![(
                Step::Resize(700.0, 760.0),
                vec![Expect::Layout(Check::InWindow("search.recent"))],
            )],
            why: "the recent-searches dropdown stays inside the window after narrowing",
            known_bug: None,
        },
    ]
}
