//! Layout rules for the instance table.

use super::rules::{Check, Rule};
use super::states::State;

const ROW: &str = "table.row.us-east-1:i-scenario-001";
const ROW_NAME: &str = "table.row.us-east-1:i-scenario-001.name";
const ROW_LARGE: &str = "table.row.us-east-1:i-large-000";

/// Layout rules contributed by the instance table area.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        // 8 columns at 1280 wide.
        Rule {
            id: "tbl-8-cols",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::Present("table.header.7"),
            why: "8 columns at 1280",
            known_bug: None,
        },
        Rule {
            id: "tbl-not-9",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::Absent("table.header.8"),
            why: "exactly 8 columns",
            known_bug: None,
        },
        // Launched is dropped below 1100 wide.
        Rule {
            id: "tbl-960-7",
            state: State::MainIdle,
            window: (960.0, 760.0),
            check: Check::Absent("table.header.7"),
            why: "Launched dropped below 1100",
            known_bug: None,
        },
        Rule {
            id: "tbl-960-6",
            state: State::MainIdle,
            window: (960.0, 760.0),
            check: Check::Present("table.header.6"),
            why: "Public IP still shown at 960",
            known_bug: None,
        },
        // 4 columns below 720 wide.
        Rule {
            id: "tbl-700-4",
            state: State::MainIdle,
            window: (700.0, 760.0),
            check: Check::Present("table.header.3"),
            why: "4 columns below 720",
            known_bug: None,
        },
        Rule {
            id: "tbl-700-not-5",
            state: State::MainIdle,
            window: (700.0, 760.0),
            check: Check::Absent("table.header.4"),
            why: "only Name, ID, State, Type below 720",
            known_bug: None,
        },
        // Rows and header are 28pt tall.
        Rule {
            id: "tbl-row-h",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::HeightBetween(ROW, 26.0, 30.0),
            why: "28pt rows",
            known_bug: None,
        },
        Rule {
            id: "tbl-header-h",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::HeightBetween("table.header", 26.0, 30.0),
            why: "header row 28pt",
            known_bug: None,
        },
        Rule {
            id: "tbl-row-in-table",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::Inside {
                inner: ROW,
                outer: "main.table",
            },
            why: "rows inside the table",
            known_bug: None,
        },
        Rule {
            id: "tbl-header-above-row",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::Above {
                upper: "table.header",
                lower: ROW,
            },
            why: "header above rows",
            known_bug: None,
        },
        Rule {
            id: "tbl-name-aligned",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::LeftAligned("table.header.0", ROW_NAME),
            why: "Name column lines up with its header",
            known_bug: None,
        },
        Rule {
            id: "tbl-large-row-h",
            state: State::MainLarge,
            window: (1280.0, 760.0),
            check: Check::HeightBetween(ROW_LARGE, 26.0, 30.0),
            why: "120 rows keep 28pt height",
            known_bug: None,
        },
    ]
}
