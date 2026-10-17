//! Layout rules for overlays.

use super::rules::{Check, Rule};
use super::states::State;

/// Layout rules contributed by the overlays area.
pub(crate) fn rules() -> Vec<Rule> {
    vec![
        // Profile picker
        Rule {
            id: "ovl-profile-above-bar",
            state: State::OverlayProfile,
            window: (1280.0, 760.0),
            check: Check::Above {
                upper: "main.overlay.profile",
                lower: "main.statusbar",
            },
            why: "picker pops up above the status bar",
            known_bug: None,
        },
        Rule {
            id: "ovl-profile-in-window",
            state: State::OverlayProfile,
            window: (1280.0, 760.0),
            check: Check::InWindow("main.overlay.profile"),
            why: "fully visible",
            known_bug: None,
        },
        Rule {
            id: "ovl-profile-520",
            state: State::OverlayProfile,
            window: (520.0, 760.0),
            check: Check::InWindow("main.overlay.profile"),
            why: "visible when compact",
            known_bug: None,
        },
        // Region picker
        Rule {
            id: "ovl-region-above-bar",
            state: State::OverlayRegion,
            window: (1280.0, 760.0),
            check: Check::Above {
                upper: "main.overlay.region",
                lower: "main.statusbar",
            },
            why: "picker pops up above the status bar",
            known_bug: None,
        },
        Rule {
            id: "ovl-region-in-window",
            state: State::OverlayRegion,
            window: (1280.0, 760.0),
            check: Check::InWindow("main.overlay.region"),
            why: "fully visible",
            known_bug: None,
        },
        Rule {
            id: "ovl-region-520",
            state: State::OverlayRegion,
            window: (520.0, 760.0),
            check: Check::InWindow("main.overlay.region"),
            why: "visible when compact",
            known_bug: None,
        },
        // Command palette
        Rule {
            id: "ovl-palette-centred",
            state: State::OverlayPalette,
            window: (1280.0, 760.0),
            check: Check::CenteredX("main.overlay.palette"),
            why: "palette centred horizontally",
            known_bug: None,
        },
        Rule {
            id: "ovl-palette-w",
            state: State::OverlayPalette,
            window: (1280.0, 760.0),
            check: Check::WidthBetween("main.overlay.palette", 520.0, 580.0),
            why: "palette ~540-560pt",
            known_bug: Some("SELECTOR: main.overlay.palette measures 0x0 at (640,80) at 1280x760 (wrong tag, not a UI bug): the tag wraps an absolute child, so the wrapper has no size; the real 560pt panel is command_palette.rs:213"),
        },
        Rule {
            id: "ovl-palette-in-window",
            state: State::OverlayPalette,
            window: (1280.0, 760.0),
            check: Check::InWindow("main.overlay.palette"),
            why: "fully visible",
            known_bug: None,
        },
        Rule {
            id: "ovl-palette-700",
            state: State::OverlayPalette,
            window: (700.0, 760.0),
            check: Check::InWindow("main.overlay.palette"),
            why: "visible when narrow",
            known_bug: None,
        },
        Rule {
            id: "ovl-palette-520",
            state: State::OverlayPalette,
            window: (520.0, 760.0),
            check: Check::InWindow("main.overlay.palette"),
            why: "must shrink below its 540pt width when compact",
            known_bug: None,
        },
        // Failure popover
        Rule {
            id: "ovl-fail-above-bar",
            state: State::OverlayFailure,
            window: (1280.0, 760.0),
            check: Check::Above {
                upper: "main.overlay.failure",
                lower: "main.statusbar",
            },
            why: "popover above the status bar",
            known_bug: None,
        },
        Rule {
            id: "ovl-fail-w",
            state: State::OverlayFailure,
            window: (1280.0, 760.0),
            check: Check::WidthBetween("main.overlay.failure", 360.0, 400.0),
            why: "popover 380pt",
            known_bug: None,
        },
        Rule {
            id: "ovl-fail-in-window",
            state: State::OverlayFailure,
            window: (1280.0, 760.0),
            check: Check::InWindow("main.overlay.failure"),
            why: "fully visible",
            known_bug: None,
        },
        Rule {
            id: "ovl-fail-520",
            state: State::OverlayFailure,
            window: (520.0, 760.0),
            check: Check::InWindow("main.overlay.failure"),
            why: "compact: width is window minus 16",
            known_bug: None,
        },
        // Row context menu
        Rule {
            id: "ovl-rowmenu-in-window",
            state: State::OverlayRowMenu,
            window: (1280.0, 760.0),
            check: Check::InWindow("main.overlay.rowmenu"),
            why: "context menu fully visible",
            known_bug: None,
        },
        Rule {
            id: "ovl-rowmenu-w",
            state: State::OverlayRowMenu,
            window: (1280.0, 760.0),
            check: Check::WidthBetween("main.overlay.rowmenu", 180.0, 240.0),
            why: "menu ~200pt",
            known_bug: None,
        },
        Rule {
            id: "ovl-rowmenu-nonzero",
            state: State::OverlayRowMenu,
            window: (1280.0, 760.0),
            check: Check::NonZero("rowmenu.item.0"),
            why: "first item rendered",
            known_bug: None,
        },
    ]
}
