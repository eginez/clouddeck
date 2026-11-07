//! Layout rules for overlays.

use super::rules::{Check, Rule};
use super::states::State;

const COLUMN_MENU_WINDOW: (f32, f32) = (1280.0, 760.0);

fn column_menu_rule(id: &'static str, state: State, check: Check, why: &'static str) -> Rule {
    Rule {
        id,
        state,
        window: COLUMN_MENU_WINDOW,
        check,
        why,
        known_bug: None,
    }
}

/// Chooser rules for one overlay state; `$p` prefixes every rule id so ids stay unique.
macro_rules! column_menu_rules {
    ($state:expr, $p:literal) => {
        vec![
            column_menu_rule(
                concat!($p, "button-in-toolbar"),
                $state,
                Check::Inside {
                    inner: "table.columns.button",
                    outer: "table.groupby",
                },
                "Columns button sits in the group-by toolbar",
            ),
            column_menu_rule(
                concat!($p, "in-window"),
                $state,
                Check::InWindow("table.columns.menu"),
                "column chooser fully visible",
            ),
            column_menu_rule(
                concat!($p, "w"),
                $state,
                Check::WidthBetween("table.columns.menu", 170.0, 190.0),
                "column chooser ~180pt",
            ),
            column_menu_rule(
                concat!($p, "item-name-inside"),
                $state,
                Check::Inside {
                    inner: "table.columns.item.name",
                    outer: "table.columns.menu",
                },
                "name item inside the chooser",
            ),
            column_menu_rule(
                concat!($p, "item-instance-id-inside"),
                $state,
                Check::Inside {
                    inner: "table.columns.item.instance_id",
                    outer: "table.columns.menu",
                },
                "instance id item inside the chooser",
            ),
            column_menu_rule(
                concat!($p, "item-state-inside"),
                $state,
                Check::Inside {
                    inner: "table.columns.item.state",
                    outer: "table.columns.menu",
                },
                "state item inside the chooser",
            ),
            column_menu_rule(
                concat!($p, "item-type-inside"),
                $state,
                Check::Inside {
                    inner: "table.columns.item.type",
                    outer: "table.columns.menu",
                },
                "type item inside the chooser",
            ),
            column_menu_rule(
                concat!($p, "item-az-inside"),
                $state,
                Check::Inside {
                    inner: "table.columns.item.az",
                    outer: "table.columns.menu",
                },
                "az item inside the chooser",
            ),
            column_menu_rule(
                concat!($p, "item-private-ip-inside"),
                $state,
                Check::Inside {
                    inner: "table.columns.item.private_ip",
                    outer: "table.columns.menu",
                },
                "private ip item inside the chooser",
            ),
            column_menu_rule(
                concat!($p, "item-public-ip-inside"),
                $state,
                Check::Inside {
                    inner: "table.columns.item.public_ip",
                    outer: "table.columns.menu",
                },
                "public ip item inside the chooser",
            ),
            column_menu_rule(
                concat!($p, "item-launched-inside"),
                $state,
                Check::Inside {
                    inner: "table.columns.item.launched",
                    outer: "table.columns.menu",
                },
                "launched item inside the chooser",
            ),
            column_menu_rule(
                concat!($p, "text-name"),
                $state,
                Check::TextIs {
                    target: "table.columns.item.name",
                    text: "Name",
                },
                "name item text",
            ),
            column_menu_rule(
                concat!($p, "text-instance-id"),
                $state,
                Check::TextIs {
                    target: "table.columns.item.instance_id",
                    text: "Instance ID",
                },
                "instance id item text",
            ),
            column_menu_rule(
                concat!($p, "text-state"),
                $state,
                Check::TextIs {
                    target: "table.columns.item.state",
                    text: "State",
                },
                "state item text",
            ),
            column_menu_rule(
                concat!($p, "text-type"),
                $state,
                Check::TextIs {
                    target: "table.columns.item.type",
                    text: "Type",
                },
                "type item text",
            ),
            column_menu_rule(
                concat!($p, "text-az"),
                $state,
                Check::TextIs {
                    target: "table.columns.item.az",
                    text: "AZ",
                },
                "az item text",
            ),
            column_menu_rule(
                concat!($p, "text-private-ip"),
                $state,
                Check::TextIs {
                    target: "table.columns.item.private_ip",
                    text: "Private IP",
                },
                "private ip item text",
            ),
            column_menu_rule(
                concat!($p, "text-public-ip"),
                $state,
                Check::TextIs {
                    target: "table.columns.item.public_ip",
                    text: "Public IP",
                },
                "public ip item text",
            ),
            column_menu_rule(
                concat!($p, "text-launched"),
                $state,
                Check::TextIs {
                    target: "table.columns.item.launched",
                    text: "Launched",
                },
                "launched item text",
            ),
            column_menu_rule(
                concat!($p, "above-name-instance-id"),
                $state,
                Check::Above {
                    upper: "table.columns.item.name",
                    lower: "table.columns.item.instance_id",
                },
                "name item above instance id item",
            ),
            column_menu_rule(
                concat!($p, "above-instance-id-state"),
                $state,
                Check::Above {
                    upper: "table.columns.item.instance_id",
                    lower: "table.columns.item.state",
                },
                "instance id item above state item",
            ),
            column_menu_rule(
                concat!($p, "above-state-type"),
                $state,
                Check::Above {
                    upper: "table.columns.item.state",
                    lower: "table.columns.item.type",
                },
                "state item above type item",
            ),
            column_menu_rule(
                concat!($p, "above-type-az"),
                $state,
                Check::Above {
                    upper: "table.columns.item.type",
                    lower: "table.columns.item.az",
                },
                "type item above az item",
            ),
            column_menu_rule(
                concat!($p, "above-az-private-ip"),
                $state,
                Check::Above {
                    upper: "table.columns.item.az",
                    lower: "table.columns.item.private_ip",
                },
                "az item above private ip item",
            ),
            column_menu_rule(
                concat!($p, "above-private-ip-public-ip"),
                $state,
                Check::Above {
                    upper: "table.columns.item.private_ip",
                    lower: "table.columns.item.public_ip",
                },
                "private ip item above public ip item",
            ),
            column_menu_rule(
                concat!($p, "above-public-ip-launched"),
                $state,
                Check::Above {
                    upper: "table.columns.item.public_ip",
                    lower: "table.columns.item.launched",
                },
                "public ip item above launched item",
            ),
            column_menu_rule(
                concat!($p, "height-name"),
                $state,
                Check::HeightBetween("table.columns.item.name", 27.0, 29.0),
                "name item not clipped vertically",
            ),
            column_menu_rule(
                concat!($p, "height-instance-id"),
                $state,
                Check::HeightBetween("table.columns.item.instance_id", 27.0, 29.0),
                "instance id item not clipped vertically",
            ),
            column_menu_rule(
                concat!($p, "height-state"),
                $state,
                Check::HeightBetween("table.columns.item.state", 27.0, 29.0),
                "state item not clipped vertically",
            ),
            column_menu_rule(
                concat!($p, "height-type"),
                $state,
                Check::HeightBetween("table.columns.item.type", 27.0, 29.0),
                "type item not clipped vertically",
            ),
            column_menu_rule(
                concat!($p, "height-az"),
                $state,
                Check::HeightBetween("table.columns.item.az", 27.0, 29.0),
                "az item not clipped vertically",
            ),
            column_menu_rule(
                concat!($p, "height-private-ip"),
                $state,
                Check::HeightBetween("table.columns.item.private_ip", 27.0, 29.0),
                "private ip item not clipped vertically",
            ),
            column_menu_rule(
                concat!($p, "height-public-ip"),
                $state,
                Check::HeightBetween("table.columns.item.public_ip", 27.0, 29.0),
                "public ip item not clipped vertically",
            ),
            column_menu_rule(
                concat!($p, "height-launched"),
                $state,
                Check::HeightBetween("table.columns.item.launched", 27.0, 29.0),
                "launched item not clipped vertically",
            ),
            column_menu_rule(
                concat!($p, "name-faint"),
                $state,
                Check::FgIs {
                    target: "table.columns.item.name",
                    token: "faint",
                },
                "name item fg token is faint when not focused",
            ),
            column_menu_rule(
                concat!($p, "az-text"),
                $state,
                Check::FgIs {
                    target: "table.columns.item.az",
                    token: "text",
                },
                "az item fg token is text when not focused",
            ),
        ]
    };
}

/// Layout rules contributed by the overlays area.
pub(crate) fn rules() -> Vec<Rule> {
    let mut rules = vec![
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
            known_bug: None,
        },
        Rule {
            id: "ovl-palette-h",
            state: State::OverlayPalette,
            window: (1280.0, 760.0),
            check: Check::HeightBetween("main.overlay.palette", 100.0, 700.0),
            why: "a panel, not a line",
            known_bug: None,
        },
        Rule {
            id: "ovl-palette-520-w",
            state: State::OverlayPalette,
            window: (520.0, 760.0),
            check: Check::WidthBetween("main.overlay.palette", 470.0, 490.0),
            why: "shrinks to window minus 20pt margins when compact",
            known_bug: None,
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
        Rule {
            id: "ovl-rowmenu-520-in-window",
            state: State::OverlayRowMenu,
            window: (520.0, 760.0),
            check: Check::InWindow("main.overlay.rowmenu"),
            why: "context menu repositioned to fit a compact window",
            known_bug: None,
        },
        Rule {
            id: "ovl-rowmenu-520-w",
            state: State::OverlayRowMenu,
            window: (520.0, 760.0),
            check: Check::WidthBetween("main.overlay.rowmenu", 180.0, 240.0),
            why: "menu not squeezed when compact",
            known_bug: None,
        },
        Rule {
            id: "ovl-rowmenu-520-item4-in",
            state: State::OverlayRowMenu,
            window: (520.0, 760.0),
            check: Check::Inside {
                inner: "rowmenu.item.3",
                outer: "main.overlay.rowmenu",
            },
            why: "Open in Console fully inside the menu when compact",
            known_bug: None,
        },
        Rule {
            id: "ovl-rowmenu-700-in-window",
            state: State::OverlayRowMenu,
            window: (700.0, 760.0),
            check: Check::InWindow("main.overlay.rowmenu"),
            why: "context menu fully visible when narrow",
            known_bug: None,
        },
        Rule {
            id: "ovl-rowmenu-520-nonzero",
            state: State::OverlayRowMenu,
            window: (520.0, 760.0),
            check: Check::NonZero("rowmenu.item.0"),
            why: "first item rendered when compact",
            known_bug: None,
        },
        Rule {
            id: "tbl-columns-button-in-toolbar",
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::Inside {
                inner: "table.columns.button",
                outer: "table.groupby",
            },
            why: "Columns button sits in the group-by toolbar",
            known_bug: None,
        },
    ];
    rules.extend(column_menu_rules!(State::OverlayColumnMenu, "ovl-columns-"));
    rules.extend(column_menu_rules!(
        State::OverlayColumnMenuDark,
        "ovl-columns-dark-"
    ));
    rules
}
