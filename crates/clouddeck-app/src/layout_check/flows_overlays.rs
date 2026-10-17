//! Scripted flows for overlays.

use super::rules::Check;
use super::scripts::{Expect, Script, Step};
use super::states::State;

const ROW_SELECTOR: &str = "table.row.us-east-1:i-scenario-001";
const ROW_ID: &str = "us-east-1:i-scenario-001";

/// The overlay flows: open and close overlays by key and by click.
pub(crate) fn scripts() -> Vec<Script> {
    vec![
        Script {
            id: "flow-profile-keys",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Keys("secondary-p"),
                    vec![
                        Expect::Overlay(Some("main.overlay.profile")),
                        Expect::Layout(Check::Above {
                            upper: "main.overlay.profile",
                            lower: "main.statusbar",
                        }),
                    ],
                ),
                (Step::Keys("escape"), vec![Expect::Overlay(None)]),
            ],
            why: "the profile picker opens from its shortcut and Esc closes it",
            known_bug: None,
        },
        Script {
            id: "flow-profile-click",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Click("bar.profile"),
                    vec![Expect::Overlay(Some("main.overlay.profile"))],
                ),
                (Step::Click("bar.profile"), vec![Expect::Overlay(None)]),
            ],
            why: "clicking the profile button again closes the picker",
            known_bug: None,
        },
        Script {
            id: "flow-region-click",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Click("bar.region"),
                    vec![Expect::Overlay(Some("main.overlay.region"))],
                ),
                (Step::Click("bar.region"), vec![Expect::Overlay(None)]),
            ],
            why: "clicking the region button opens the region picker and clicking it again closes it",
            known_bug: None,
        },
        Script {
            id: "flow-region-keys",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Keys("secondary-shift-r"),
                    vec![
                        Expect::Overlay(Some("main.overlay.region")),
                        Expect::Layout(Check::InWindow("main.overlay.region")),
                    ],
                ),
                (Step::Keys("escape"), vec![Expect::Overlay(None)]),
            ],
            why: "the region picker opens inside the window and Esc closes it",
            known_bug: None,
        },
        Script {
            id: "flow-palette",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Keys("secondary-shift-p"),
                    vec![
                        Expect::Overlay(Some("main.overlay.palette")),
                        Expect::Layout(Check::CenteredX("main.overlay.palette")),
                    ],
                ),
                (
                    Step::Type("copy"),
                    vec![Expect::Layout(Check::Present("palette.row.0"))],
                ),
                (Step::Keys("escape"), vec![Expect::Overlay(None)]),
            ],
            why: "the command palette is centred and filters its rows as the user types",
            known_bug: None,
        },
        Script {
            id: "flow-palette-click-close",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Keys("secondary-shift-p"),
                    vec![Expect::Overlay(Some("main.overlay.palette"))],
                ),
                (Step::Click("bar.profile"), vec![Expect::Overlay(None)]),
            ],
            why: "a click on the bar behind the palette closes the palette without activating the bar button",
            known_bug: None,
        },
        Script {
            id: "flow-failure",
            start: State::MainPartial,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Click("bar.failure"),
                    vec![
                        Expect::Overlay(Some("main.overlay.failure")),
                        Expect::Layout(Check::Above {
                            upper: "main.overlay.failure",
                            lower: "main.statusbar",
                        }),
                    ],
                ),
                (Step::Keys("escape"), vec![Expect::Overlay(None)]),
            ],
            why: "the failure popover opens above the status bar and Esc closes it",
            known_bug: None,
        },
        Script {
            id: "flow-failure-click",
            start: State::MainPartial,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Click("bar.failure"),
                    vec![Expect::Overlay(Some("main.overlay.failure"))],
                ),
                (Step::Click("bar.failure"), vec![Expect::Overlay(None)]),
            ],
            why: "clicking the failure button again closes the failure popover",
            known_bug: None,
        },
        Script {
            id: "flow-rowmenu",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::RightClick(ROW_SELECTOR),
                    vec![
                        Expect::Overlay(Some("main.overlay.rowmenu")),
                        Expect::Selected(Some(ROW_ID)),
                    ],
                ),
                (Step::Keys("escape"), vec![Expect::Overlay(None)]),
            ],
            why: "right-clicking a row opens its menu and selects that row",
            known_bug: None,
        },
        Script {
            id: "flow-rowmenu-click",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::RightClick(ROW_SELECTOR),
                    vec![Expect::Overlay(Some("main.overlay.rowmenu"))],
                ),
                (Step::Click(ROW_SELECTOR), vec![Expect::Overlay(None)]),
            ],
            why: "clicking the row again while its menu is open closes the menu",
            known_bug: None,
        },
        Script {
            id: "flow-esc-chain",
            start: State::MainRowSelected,
            window: (1280.0, 760.0),
            steps: vec![(
                Step::Keys("escape"),
                vec![
                    Expect::Selected(None),
                    Expect::Panes {
                        sidebar: true,
                        inspector: false,
                    },
                ],
            )],
            why: "Esc clears the row selection and leaves the sidebar open and inspector closed",
            known_bug: None,
        },
        Script {
            id: "flow-narrow-palette",
            start: State::MainIdle,
            window: (520.0, 760.0),
            steps: vec![(
                Step::Keys("secondary-shift-p"),
                vec![Expect::Layout(Check::InWindow("main.overlay.palette"))],
            )],
            why: "the command palette stays inside a narrow window",
            known_bug: None,
        },
    ]
}
