//! Layout rules for the instance table.

use super::rules::{Check, Rule};
use super::states::State;

const ROW: &str = "table.row.us-east-1:i-scenario-001";
const ROW_NAME: &str = "table.row.us-east-1:i-scenario-001.name";
const ROW_LARGE: &str = "table.row.us-east-1:i-large-000";
const ROW_RUNNING_SECOND: &str = "table.row.us-east-1:i-scenario-003";
const ROW_STOPPED: &str = "table.row.us-east-1:i-scenario-002";
const GROUP_RUNNING: &str = "table.group.Running";
const GROUP_RUNNING_LABEL: &str = "table.group.Running.label";
const GROUP_STOPPED: &str = "table.group.Stopped";

/// Rules for a table grouped by state with every group expanded.
fn grouped_expanded(state: State, ids: [&'static str; 6]) -> Vec<Rule> {
    let window = (1280.0, 760.0);
    vec![
        Rule {
            id: ids[0],
            state,
            window,
            check: Check::HeightBetween(GROUP_RUNNING, 26.0, 30.0),
            why: "group header rows are 28pt",
            known_bug: None,
        },
        Rule {
            id: ids[1],
            state,
            window,
            check: Check::LeftAligned("table.header", GROUP_RUNNING),
            why: "group header starts at the table's left edge",
            known_bug: None,
        },
        Rule {
            id: ids[2],
            state,
            window,
            check: Check::RightAligned("table.header", GROUP_RUNNING),
            why: "group header spans the table width",
            known_bug: None,
        },
        Rule {
            id: ids[3],
            state,
            window,
            check: Check::Inside {
                inner: GROUP_RUNNING_LABEL,
                outer: GROUP_RUNNING,
            },
            why: "group label is not clipped",
            known_bug: None,
        },
        Rule {
            id: ids[4],
            state,
            window,
            check: Check::Above {
                upper: GROUP_RUNNING,
                lower: ROW,
            },
            why: "a group header sits above its rows",
            known_bug: None,
        },
        Rule {
            id: ids[5],
            state,
            window,
            check: Check::Above {
                upper: ROW_RUNNING_SECOND,
                lower: GROUP_STOPPED,
            },
            why: "the Stopped header follows the Running rows",
            known_bug: None,
        },
    ]
}

/// Rules for a table grouped by state with the Running group collapsed.
fn grouped_collapsed(state: State, ids: [&'static str; 5]) -> Vec<Rule> {
    let window = (1280.0, 760.0);
    vec![
        Rule {
            id: ids[0],
            state,
            window,
            check: Check::Present(GROUP_RUNNING),
            why: "a collapsed group keeps its header",
            known_bug: None,
        },
        Rule {
            id: ids[1],
            state,
            window,
            check: Check::Absent(ROW),
            why: "a collapsed group hides its first row",
            known_bug: None,
        },
        Rule {
            id: ids[2],
            state,
            window,
            check: Check::Absent(ROW_RUNNING_SECOND),
            why: "a collapsed group hides its second row",
            known_bug: None,
        },
        Rule {
            id: ids[3],
            state,
            window,
            check: Check::Above {
                upper: GROUP_RUNNING,
                lower: ROW_STOPPED,
            },
            why: "the next group's rows follow the collapsed header",
            known_bug: None,
        },
        Rule {
            id: ids[4],
            state,
            window,
            check: Check::HeightBetween(GROUP_RUNNING, 26.0, 30.0),
            why: "a collapsed group header is 28pt",
            known_bug: None,
        },
    ]
}

/// Layout rules contributed by the instance table area.
pub(crate) fn rules() -> Vec<Rule> {
    let mut rules = vec![
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
        // The pane is 750pt at 960 with the sidebar open; Public IP needs 835.
        Rule {
            id: "tbl-960-6",
            state: State::MainIdle,
            window: (960.0, 760.0),
            check: Check::Absent("table.header.6"),
            why: "Public IP dropped at 960: pane is 750pt, 7 columns need 835",
            known_bug: None,
        },
        Rule {
            id: "tbl-960-5",
            state: State::MainIdle,
            window: (960.0, 760.0),
            check: Check::Present("table.header.5"),
            why: "Private IP still shown at 960",
            known_bug: None,
        },
        Rule {
            id: "tbl-960-fits",
            state: State::MainIdle,
            window: (960.0, 760.0),
            check: Check::Inside {
                inner: "table.header.5",
                outer: "main.table",
            },
            why: "columns at 960 stay inside the table pane",
            known_bug: None,
        },
        // With the inspector open the pane is 1280 - 210 - 340 = 730pt.
        Rule {
            id: "tbl-1280-sel-6",
            state: State::MainRowSelected,
            window: (1280.0, 760.0),
            check: Check::Absent("table.header.6"),
            why: "Public IP dropped at 1280 with the inspector open",
            known_bug: None,
        },
        Rule {
            id: "tbl-1280-sel-5",
            state: State::MainRowSelected,
            window: (1280.0, 760.0),
            check: Check::Present("table.header.5"),
            why: "Private IP still shown at 1280 with the inspector open",
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
    ];
    rules.extend(grouped_expanded(
        State::MainStateGrouped,
        [
            "tbl-grouped-state-header-h",
            "tbl-grouped-state-header-left",
            "tbl-grouped-state-header-right",
            "tbl-grouped-state-label",
            "tbl-grouped-state-header-above-rows",
            "tbl-grouped-state-stopped-below",
        ],
    ));
    rules.extend(grouped_expanded(
        State::MainStateGroupedDark,
        [
            "tbl-grouped-state-dark-header-h",
            "tbl-grouped-state-dark-header-left",
            "tbl-grouped-state-dark-header-right",
            "tbl-grouped-state-dark-label",
            "tbl-grouped-state-dark-header-above-rows",
            "tbl-grouped-state-dark-stopped-below",
        ],
    ));
    rules.extend(grouped_collapsed(
        State::MainStateGroupedCollapsed,
        [
            "tbl-grouped-collapsed-header",
            "tbl-grouped-collapsed-hides-first",
            "tbl-grouped-collapsed-hides-second",
            "tbl-grouped-collapsed-next-group",
            "tbl-grouped-collapsed-header-h",
        ],
    ));
    rules.extend(grouped_collapsed(
        State::MainStateGroupedCollapsedDark,
        [
            "tbl-grouped-collapsed-dark-header",
            "tbl-grouped-collapsed-dark-hides-first",
            "tbl-grouped-collapsed-dark-hides-second",
            "tbl-grouped-collapsed-dark-next-group",
            "tbl-grouped-collapsed-dark-header-h",
        ],
    ));
    rules
}
