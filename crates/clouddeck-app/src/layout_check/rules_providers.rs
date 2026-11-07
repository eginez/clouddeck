//! Layout rules for the All-providers list and the states that use several providers.

use super::rules::{Check, Rule};
use super::states::State;

const MAIN: (f32, f32) = (1280.0, 760.0);
const NARROW: (f32, f32) = (700.0, 760.0);
const COMPACT: (f32, f32) = (520.0, 760.0);

const PROVIDER_HEADER: &str = "table.header.0";
const NAME_HEADER: &str = "table.header.1";
const AWS_ROW: &str = "table.row.us-east-1:i-scenario-001";
const AWS_PROVIDER_CELL: &str = "table.row.us-east-1:i-scenario-001.provider";
const GCP_ROW: &str = "table.row.us-east-1:gcp-instance-001";
const GCP_GROUP: &str = "table.group.gcp";
const PROVIDER_BUTTON: &str = "bar.aws";
/// Row 0 is the "All regions" row, which has no value and so no only link.
const DIMENSION_ROOT_LABEL: &str = "dimension-picker.label.1";
const DIMENSION_ZONE_LABEL: &str = "dimension-picker.label.2";
const DIMENSION_DEEP_ZONE_LABEL: &str = "dimension-picker.label.3";
const DIMENSION_ROW_ROOT: &str = "dimension-picker.row.1";
const DIMENSION_ONLY_LINK: &str = "dimension-picker.only.1";
const INDENT: f32 = 16.0;

/// The Provider column is 64pt wide; the rule allows the header's padding.
const PROVIDER_WIDTH: (f32, f32) = (62.0, 66.0);

pub(crate) fn rules() -> Vec<Rule> {
    let mut rules = Vec::new();
    rules.extend(all_providers(State::MainAllProviders, MAIN));
    rules.extend(all_providers_dark(State::MainAllProvidersDark, MAIN));
    rules.extend(all_providers_no_column(
        State::MainAllProvidersNoColumn,
        MAIN,
        [
            "all-providers-column-hidden",
            "all-providers-hidden-keeps-rows",
        ],
    ));
    rules.extend(all_providers_no_column(
        State::MainAllProvidersNoColumnDark,
        MAIN,
        [
            "all-providers-dark-column-hidden",
            "all-providers-dark-hidden-keeps-rows",
        ],
    ));
    rules.extend(failed_group());
    rules.extend(all_providers_narrow());
    rules.extend(all_providers_compact());
    rules.extend(gcp_active());
    rules.extend(gcp_sign_in());
    rules.extend(dimension_tree());
    rules.extend(dimension_single());
    rules
}

fn all_providers(state: State, window: (f32, f32)) -> Vec<Rule> {
    vec![
        Rule {
            id: "all-providers-column-first",
            state,
            window,
            check: Check::LeftOf(PROVIDER_HEADER, NAME_HEADER),
            why: "the Provider column is the leftmost column in the All view",
            known_bug: None,
        },
        Rule {
            id: "all-providers-column-width",
            state,
            window,
            check: Check::WidthBetween(PROVIDER_HEADER, PROVIDER_WIDTH.0, PROVIDER_WIDTH.1),
            why: "the Provider column is 64pt wide",
            known_bug: None,
        },
        Rule {
            id: "all-providers-header-text",
            state,
            window,
            check: Check::TextIs {
                target: PROVIDER_HEADER,
                text: "Provider",
            },
            why: "the All view labels the column Provider rather than a single provider's name",
            known_bug: None,
        },
        Rule {
            id: "all-providers-cell-text",
            state,
            window,
            check: Check::TextIs {
                target: AWS_PROVIDER_CELL,
                text: "AWS",
            },
            why: "each row shows the short name of the provider it was listed from",
            known_bug: None,
        },
        Rule {
            id: "all-providers-gcp-rows-listed",
            state,
            window,
            check: Check::Present(GCP_ROW),
            why: "rows from a second provider appear alongside the first provider's rows",
            known_bug: None,
        },
    ]
}

fn all_providers_dark(state: State, window: (f32, f32)) -> Vec<Rule> {
    vec![Rule {
        id: "all-providers-dark-column-first",
        state,
        window,
        check: Check::LeftOf(PROVIDER_HEADER, NAME_HEADER),
        why: "the Provider column is leftmost in the dark palette too",
        known_bug: None,
    }]
}

fn all_providers_no_column(state: State, window: (f32, f32), ids: [&'static str; 2]) -> Vec<Rule> {
    vec![
        Rule {
            id: ids[0],
            state,
            window,
            check: Check::TextIs {
                target: PROVIDER_HEADER,
                text: "Name",
            },
            why: "with the Provider column toggled off the first column is Name",
            known_bug: None,
        },
        Rule {
            id: ids[1],
            state,
            window,
            check: Check::Present(GCP_ROW),
            why: "hiding the Provider column does not hide rows from any provider",
            known_bug: None,
        },
    ]
}

fn failed_group() -> Vec<Rule> {
    let state = State::MainAllProvidersFailedGroup;
    vec![
        Rule {
            id: "all-failed-group-header",
            state,
            window: MAIN,
            check: Check::Present(GCP_GROUP),
            why: "a provider whose load failed keeps a group header in the provider grouping",
            known_bug: None,
        },
        Rule {
            id: "all-failed-group-keeps-other-rows",
            state,
            window: MAIN,
            check: Check::Present(AWS_ROW),
            why: "a failed provider does not hide the rows of the providers that loaded",
            known_bug: None,
        },
        Rule {
            id: "all-failed-group-has-no-rows",
            state,
            window: MAIN,
            check: Check::Absent(GCP_ROW),
            why: "a failed provider contributes no rows",
            known_bug: None,
        },
    ]
}

fn all_providers_narrow() -> Vec<Rule> {
    let state = State::MainAllProvidersNarrow;
    vec![
        Rule {
            id: "all-narrow-column-in-window",
            state,
            window: NARROW,
            check: Check::InWindow(PROVIDER_HEADER),
            why: "the Provider column stays inside the window at a narrow width",
            known_bug: None,
        },
        Rule {
            id: "all-narrow-column-width",
            state,
            window: NARROW,
            check: Check::WidthBetween(PROVIDER_HEADER, PROVIDER_WIDTH.0, PROVIDER_WIDTH.1),
            why: "the Provider column keeps its 64pt width at a narrow width",
            known_bug: None,
        },
        Rule {
            id: "all-narrow-provider-button-in-window",
            state,
            window: NARROW,
            check: Check::InWindow(PROVIDER_BUTTON),
            why: "the provider button stays inside the window at a narrow width",
            known_bug: None,
        },
    ]
}

fn all_providers_compact() -> Vec<Rule> {
    let state = State::MainAllProvidersCompact;
    vec![
        Rule {
            id: "all-compact-column-in-window",
            state,
            window: COMPACT,
            check: Check::InWindow(PROVIDER_HEADER),
            why: "the Provider column stays inside the window at the most compact width",
            known_bug: None,
        },
        Rule {
            id: "all-compact-column-no-overlap",
            state,
            window: COMPACT,
            check: Check::NoOverlap(PROVIDER_HEADER, NAME_HEADER),
            why: "the Provider and Name headers do not overlap at the most compact width",
            known_bug: None,
        },
    ]
}

fn gcp_active() -> Vec<Rule> {
    let state = State::MainGcpActive;
    vec![
        Rule {
            id: "gcp-active-no-provider-column",
            state,
            window: MAIN,
            check: Check::TextIs {
                target: PROVIDER_HEADER,
                text: "Name",
            },
            why: "a single-provider view has no Provider column",
            known_bug: None,
        },
        Rule {
            id: "gcp-active-lists-only-gcp",
            state,
            window: MAIN,
            check: Check::Absent(AWS_ROW),
            why: "a single-provider view lists only the active provider's rows",
            known_bug: None,
        },
        Rule {
            id: "gcp-active-lists-gcp-rows",
            state,
            window: MAIN,
            check: Check::Present(GCP_ROW),
            why: "the active provider's rows are listed",
            known_bug: None,
        },
    ]
}

fn gcp_sign_in() -> Vec<Rule> {
    let state = State::MainGcpSignIn;
    vec![
        Rule {
            id: "gcp-sign-in-button",
            state,
            window: MAIN,
            check: Check::Present("bar.sign-in"),
            why: "a signed-out provider shows the sign-in button in the bar",
            known_bug: None,
        },
        Rule {
            id: "gcp-sign-in-no-rows",
            state,
            window: MAIN,
            check: Check::Absent(GCP_ROW),
            why: "a signed-out provider lists no rows",
            known_bug: None,
        },
    ]
}

fn dimension_tree() -> Vec<Rule> {
    let state = State::OverlayDimensionTree;
    vec![
        Rule {
            id: "dimension-tree-zone-indented",
            state,
            window: MAIN,
            check: Check::IndentedBy {
                parent: DIMENSION_ROOT_LABEL,
                child: DIMENSION_ZONE_LABEL,
                min: INDENT,
            },
            why: "a zone is indented under its region",
            known_bug: None,
        },
        Rule {
            id: "dimension-tree-deep-zone-indented",
            state,
            window: MAIN,
            check: Check::IndentedBy {
                parent: DIMENSION_ZONE_LABEL,
                child: DIMENSION_DEEP_ZONE_LABEL,
                min: INDENT,
            },
            why: "a zone under a zone is indented further than its parent",
            known_bug: None,
        },
        Rule {
            id: "dimension-tree-multi-has-only-link",
            state,
            window: MAIN,
            check: Check::Present(DIMENSION_ONLY_LINK),
            why: "a multi-select dimension offers the only link on each row",
            known_bug: None,
        },
    ]
}

fn dimension_single() -> Vec<Rule> {
    let state = State::OverlayDimensionSingle;
    vec![
        Rule {
            id: "dimension-single-rows-listed",
            state,
            window: MAIN,
            check: Check::Present(DIMENSION_ROW_ROOT),
            why: "the single-select region picker lists its values",
            known_bug: None,
        },
        Rule {
            id: "dimension-single-no-only-link",
            state,
            window: MAIN,
            check: Check::Absent(DIMENSION_ONLY_LINK),
            why: "a single-select dimension has no only link, since selecting a row already selects only it",
            known_bug: None,
        },
    ]
}
