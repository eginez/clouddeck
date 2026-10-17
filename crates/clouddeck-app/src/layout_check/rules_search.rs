//! Layout rules for the search field.

use super::rules::{Check, Rule};
use super::states::State;

/// Layout rules contributed by the search field area.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        // The search field lives in the toolbar.
        Rule {
            id: "srch-field-in-toolbar",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::Inside {
                inner: "search.field",
                outer: "main.toolbar",
            },
            why: "search lives in the toolbar",
            known_bug: None,
        },
        // The field is 220pt wide.
        Rule {
            id: "srch-field-w",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::WidthBetween("search.field", 200.0, 240.0),
            why: "field 220pt",
            known_bug: None,
        },
        // The field is 26pt tall.
        Rule {
            id: "srch-field-h",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::HeightBetween("search.field", 24.0, 30.0),
            why: "field 26pt",
            known_bug: None,
        },
        // The field is fully visible in the window.
        Rule {
            id: "srch-field-in-window",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::InWindow("search.field"),
            why: "field fully visible",
            known_bug: None,
        },
        // The ⌘F badge sits inside the field.
        Rule {
            id: "srch-hint-in-field",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::Inside {
                inner: "search.hint",
                outer: "search.field",
            },
            why: "the ⌘F badge sits inside the field",
            known_bug: None,
        },
        // In a compact window the search collapses to an icon.
        Rule {
            id: "srch-520-collapsed",
            state: State::MainIdle,
            window: (520.0, 760.0),
            check: Check::WidthBetween("search.field", 24.0, 40.0),
            why: "compact: search collapses to an icon",
            known_bug: Some("search.field stays 220pt wide at 520pt window (194,7.5 220x28); mockup collapses it to a 24-40pt icon"),
        },
        // The collapsed field is still visible.
        Rule {
            id: "srch-520-in-window",
            state: State::MainIdle,
            window: (520.0, 760.0),
            check: Check::InWindow("search.field"),
            why: "field visible when compact",
            known_bug: None,
        },
        // Recent opens below the field.
        Rule {
            id: "srch-recent-below",
            state: State::SearchFocusedEmpty,
            window: (1280.0, 760.0),
            check: Check::Above {
                upper: "search.field",
                lower: "search.recent",
            },
            why: "Recent opens below the field",
            known_bug: None,
        },
        // REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint (mockup: right-aligned, right:0).
        Rule {
            id: "srch-recent-right",
            state: State::SearchFocusedEmpty,
            window: (1280.0, 760.0),
            check: Check::RightAligned("search.recent", "search.field"),
            why: "REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint (mockup: right-aligned, right:0)",
            known_bug: Some("search.recent (954,39.5 280x172.5) is left-aligned to search.field (954,7.5 220x28), not right-aligned (right:0)"),
        },
        // REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint.
        Rule {
            id: "srch-recent-in-window",
            state: State::SearchFocusedEmpty,
            window: (1280.0, 760.0),
            check: Check::InWindow("search.recent"),
            why: "REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint",
            known_bug: None,
        },
        // The Recent popover is 260pt wide.
        Rule {
            id: "srch-recent-w",
            state: State::SearchFocusedEmpty,
            window: (1280.0, 760.0),
            check: Check::WidthBetween("search.recent", 240.0, 300.0),
            why: "Recent popover 260pt",
            known_bug: None,
        },
        // Recent rows sit inside the popover.
        Rule {
            id: "srch-recent-row-in",
            state: State::SearchFocusedEmpty,
            window: (1280.0, 760.0),
            check: Check::Inside {
                inner: "search.recent.0",
                outer: "search.recent",
            },
            why: "rows inside the popover",
            known_bug: None,
        },
        // REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint.
        Rule {
            id: "srch-syntax-in",
            state: State::SearchFocusedEmpty,
            window: (1280.0, 760.0),
            check: Check::Inside {
                inner: "search.syntax-hint",
                outer: "search.recent",
            },
            why: "REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint",
            known_bug: None,
        },
        // REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint.
        Rule {
            id: "srch-960-recent-in-window",
            state: State::SearchFocusedEmpty,
            window: (960.0, 760.0),
            check: Check::InWindow("search.recent"),
            why: "REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint",
            known_bug: None,
        },
        // REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint.
        Rule {
            id: "srch-700-recent-in-window",
            state: State::SearchFocusedEmpty,
            window: (700.0, 760.0),
            check: Check::InWindow("search.recent"),
            why: "REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint",
            known_bug: None,
        },
        // REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint.
        Rule {
            id: "srch-520-recent-in-window",
            state: State::SearchFocusedEmpty,
            window: (520.0, 760.0),
            check: Check::InWindow("search.recent"),
            why: "REPORTED by Esteban 2026-10-11: the Recent popover runs off the window's right edge and clips this hint",
            known_bug: None,
        },
        // Suggestions open below the field.
        Rule {
            id: "srch-sugg-below",
            state: State::SearchTypingSt,
            window: (1280.0, 760.0),
            check: Check::Above {
                upper: "search.field",
                lower: "search.suggestions",
            },
            why: "suggestions open below the field",
            known_bug: None,
        },
        // Suggestions are right-aligned to the field, the same family as the reported bug.
        Rule {
            id: "srch-sugg-right",
            state: State::SearchTypingSt,
            window: (1280.0, 760.0),
            check: Check::RightAligned("search.suggestions", "search.field"),
            why: "mockup: right-aligned, right:0 (same family as the reported bug)",
            known_bug: Some("search.suggestions (954,39.5 280x59.5) is left-aligned to search.field (954,7.5 220x28), not right-aligned (right:0)"),
        },
        // Suggestions are fully visible in the window.
        Rule {
            id: "srch-sugg-in-window",
            state: State::SearchTypingSt,
            window: (1280.0, 760.0),
            check: Check::InWindow("search.suggestions"),
            why: "suggestions fully visible",
            known_bug: None,
        },
        // Suggestions width range as specified; the mockup's 300pt minimum is not enforced here.
        Rule {
            id: "srch-sugg-w",
            state: State::SearchTypingSt,
            window: (1280.0, 760.0),
            check: Check::WidthBetween("search.suggestions", 280.0, 420.0),
            why: "suggestions width range (mockup says at least 300pt)",
            known_bug: None,
        },
        // Suggestion rows sit inside the dropdown.
        Rule {
            id: "srch-sugg-row-in",
            state: State::SearchTypingSt,
            window: (1280.0, 760.0),
            check: Check::Inside {
                inner: "search.suggestion.0",
                outer: "search.suggestions",
            },
            why: "rows inside the dropdown",
            known_bug: None,
        },
        // Suggestions stay visible in a narrow window.
        Rule {
            id: "srch-700-sugg-in-window",
            state: State::SearchTypingSt,
            window: (700.0, 760.0),
            check: Check::InWindow("search.suggestions"),
            why: "suggestions visible when narrow",
            known_bug: None,
        },
        // Suggestions stay visible in a compact window.
        Rule {
            id: "srch-520-sugg-in-window",
            state: State::SearchTypingSt,
            window: (520.0, 760.0),
            check: Check::InWindow("search.suggestions"),
            why: "suggestions visible when compact",
            known_bug: None,
        },
        // The error hint sits below the field.
        Rule {
            id: "srch-err-below",
            state: State::SearchError,
            window: (1280.0, 760.0),
            check: Check::Above {
                upper: "search.field",
                lower: "search.error",
            },
            why: "error hint below the field",
            known_bug: None,
        },
        // The error hint is visible.
        Rule {
            id: "srch-err-in-window",
            state: State::SearchError,
            window: (1280.0, 760.0),
            check: Check::InWindow("search.error"),
            why: "error hint visible",
            known_bug: None,
        },
        // The error hint stays visible in a compact window.
        Rule {
            id: "srch-520-err-in-window",
            state: State::SearchError,
            window: (520.0, 760.0),
            check: Check::InWindow("search.error"),
            why: "error hint visible when compact",
            known_bug: None,
        },
    ]
}
