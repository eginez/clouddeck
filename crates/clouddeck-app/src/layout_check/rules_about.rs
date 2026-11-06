//! Layout rules for the About window.

use super::rules::{Check, Rule};
use super::states::State;

const WINDOW: (f32, f32) = crate::about_window::WINDOW_SIZE;

macro_rules! about_rules {
    ($state:expr, $prefix:literal) => {
        vec![
            Rule {
                id: concat!($prefix, "-title"),
                state: $state,
                window: WINDOW,
                check: Check::Present("about.title"),
                why: "app name and version shown",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-title-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("about.title"),
                why: "title visible",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-icon"),
                state: $state,
                window: WINDOW,
                check: Check::NonZero("about.icon"),
                why: "app icon has a size",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-stack"),
                state: $state,
                window: WINDOW,
                check: Check::Above {
                    upper: "about.title",
                    lower: "about.commit",
                },
                why: "commit row sits under the title",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-commit-version"),
                state: $state,
                window: WINDOW,
                check: Check::Above {
                    upper: "about.commit",
                    lower: "about.version-full",
                },
                why: "version row sits under the commit",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-version-plugins"),
                state: $state,
                window: WINDOW,
                check: Check::Above {
                    upper: "about.version-full",
                    lower: "about.plugins",
                },
                why: "plugin list sits under the version",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-plugins-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("about.plugins"),
                why: "plugin list visible",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-plugins-height"),
                state: $state,
                window: WINDOW,
                check: Check::HeightBetween("about.plugins", 40.0, 140.0),
                why: "a long plugin list scrolls instead of pushing the buttons off screen",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-first-plugin"),
                state: $state,
                window: WINDOW,
                check: Check::NonZero("about.plugin.0"),
                why: "first plugin row has a size",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-buttons-below-plugins"),
                state: $state,
                window: WINDOW,
                check: Check::Above {
                    upper: "about.plugins",
                    lower: "about.copy",
                },
                why: "buttons sit below the plugin list",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-copy-ok-row"),
                state: $state,
                window: WINDOW,
                check: Check::SameRow("about.copy", "about.ok"),
                why: "Copy and OK share a row",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-copy-left-of-ok"),
                state: $state,
                window: WINDOW,
                check: Check::LeftOf("about.copy", "about.ok"),
                why: "Copy sits left of OK",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-ok-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("about.ok"),
                why: "OK button visible",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-copy-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("about.copy"),
                why: "Copy button visible",
                known_bug: None,
            },
        ]
    };
}

/// Layout rules contributed by the About window area.
pub(crate) fn rules() -> Vec<Rule> {
    let mut rules = Vec::new();
    rules.extend(about_rules!(State::AboutLight, "about-light"));
    rules.extend(about_rules!(State::AboutDark, "about-dark"));
    rules.extend(about_rules!(State::AboutRefusedPlugin, "about-refused"));
    rules.extend(about_rules!(State::AboutLongPluginList, "about-long"));
    rules.push(Rule {
        id: "about-refused-row",
        state: State::AboutRefusedPlugin,
        window: WINDOW,
        check: Check::Present("about.plugin.2"),
        why: "the refused plugin is listed with its reason",
        known_bug: None,
    });
    rules.push(Rule {
        id: "about-long-rows",
        state: State::AboutLongPluginList,
        window: WINDOW,
        check: Check::Present("about.plugin.20"),
        why: "every plugin is rendered in the scrolling list",
        known_bug: None,
    });
    rules
}
