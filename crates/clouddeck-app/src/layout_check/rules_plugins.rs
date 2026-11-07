//! Layout rules for the Plugins window.

use super::rules::{Check, Rule};
use super::states::State;

const WINDOW: (f32, f32) = crate::plugins_window::WINDOW_SIZE;
const NARROW: (f32, f32) = (480.0, 520.0);

macro_rules! chrome_rules {
    ($state:expr, $prefix:literal, $window:expr) => {
        vec![
            Rule {
                id: concat!($prefix, "-root"),
                state: $state,
                window: $window,
                check: Check::Present("plugins.root"),
                why: "window root renders",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-search-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("plugins.search"),
                why: "search field visible",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-search-tabs"),
                state: $state,
                window: $window,
                check: Check::SameRow("plugins.search", "plugins.tab.installed"),
                why: "search and tabs share the header row",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-tabs-order"),
                state: $state,
                window: $window,
                check: Check::LeftOf("plugins.tab.installed", "plugins.tab.available"),
                why: "Installed tab sits left of Available",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-install-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("plugins.install-dev"),
                why: "Install Dev Extension button visible",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-header-above-list"),
                state: $state,
                window: $window,
                check: Check::Above {
                    upper: "plugins.search",
                    lower: "plugins.list",
                },
                why: "plugin list sits under the header",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-list-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("plugins.list"),
                why: "plugin list visible",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-list-width"),
                state: $state,
                window: $window,
                check: Check::WidthBetween("plugins.list", 240.0, 280.0),
                why: "list keeps its column width",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-status-in"),
                state: $state,
                window: $window,
                check: Check::InWindow("plugins.status"),
                why: "status line visible",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-status-below-list"),
                state: $state,
                window: $window,
                check: Check::Above {
                    upper: "plugins.list",
                    lower: "plugins.status",
                },
                why: "status line sits under the list",
                known_bug: None,
            },
        ]
    };
}

macro_rules! browse_rules {
    ($state:expr, $prefix:literal) => {
        vec![
            Rule {
                id: concat!($prefix, "-rows"),
                state: $state,
                window: WINDOW,
                check: Check::NonZero("plugins.row.0"),
                why: "first plugin row has a size",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-row-no-action"),
                state: $state,
                window: WINDOW,
                check: Check::Absent("plugins.row.0.primary"),
                why: "bundled plugins offer no row action",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-last-row-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("plugins.row.2"),
                why: "third plugin row visible without scrolling",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-empty-detail-in"),
                state: $state,
                window: WINDOW,
                check: Check::InWindow("plugins.detail.empty"),
                why: "empty detail prompt visible",
                known_bug: None,
            },
            Rule {
                id: concat!($prefix, "-list-left-of-detail"),
                state: $state,
                window: WINDOW,
                check: Check::LeftOf("plugins.list", "plugins.detail.empty"),
                why: "detail panel sits right of the list",
                known_bug: None,
            },
        ]
    };
}

/// Layout rules contributed by the Plugins window area.
pub(crate) fn rules() -> Vec<Rule> {
    let mut rules = Vec::new();
    rules.extend(chrome_rules!(State::PluginsLight, "plugins-light", WINDOW));
    rules.extend(browse_rules!(State::PluginsLight, "plugins-light"));
    rules.extend(chrome_rules!(State::PluginsDark, "plugins-dark", WINDOW));
    rules.extend(browse_rules!(State::PluginsDark, "plugins-dark"));
    rules.extend(chrome_rules!(
        State::PluginsInstalled,
        "plugins-installed",
        WINDOW
    ));
    rules.push(Rule {
        id: "plugins-installed-detail-in",
        state: State::PluginsInstalled,
        window: WINDOW,
        check: Check::InWindow("plugins.detail"),
        why: "selected plugin's detail panel visible",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-installed-detail-right-of-list",
        state: State::PluginsInstalled,
        window: WINDOW,
        check: Check::NoOverlap("plugins.list", "plugins.detail"),
        why: "detail panel does not cover the list",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-installed-detail-no-remove",
        state: State::PluginsInstalled,
        window: WINDOW,
        check: Check::Absent("plugins.detail.remove"),
        why: "bundled plugins cannot be removed",
        known_bug: None,
    });
    rules.extend(chrome_rules!(
        State::PluginsRefused,
        "plugins-refused",
        WINDOW
    ));
    rules.push(Rule {
        id: "plugins-refused-row",
        state: State::PluginsRefused,
        window: WINDOW,
        check: Check::Present("plugins.row.3"),
        why: "the refused plugin is listed",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-refused-detail-in",
        state: State::PluginsRefused,
        window: WINDOW,
        check: Check::InWindow("plugins.detail"),
        why: "refused plugin's detail panel visible",
        known_bug: None,
    });
    rules.extend(chrome_rules!(
        State::PluginsEmptySearch,
        "plugins-empty",
        WINDOW
    ));
    rules.push(Rule {
        id: "plugins-empty-message-in",
        state: State::PluginsEmptySearch,
        window: WINDOW,
        check: Check::InWindow("plugins.empty"),
        why: "no-matches message visible",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-empty-no-rows",
        state: State::PluginsEmptySearch,
        window: WINDOW,
        check: Check::Absent("plugins.row.0"),
        why: "a search with no matches lists no rows",
        known_bug: None,
    });
    rules.extend(chrome_rules!(
        State::PluginsAvailableEmpty,
        "plugins-available",
        WINDOW
    ));
    rules.push(Rule {
        id: "plugins-available-message-in",
        state: State::PluginsAvailableEmpty,
        window: WINDOW,
        check: Check::InWindow("plugins.empty"),
        why: "Available empty state message visible",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-available-no-rows",
        state: State::PluginsAvailableEmpty,
        window: WINDOW,
        check: Check::Absent("plugins.row.0"),
        why: "Available lists no rows without an index",
        known_bug: None,
    });
    rules.extend(chrome_rules!(
        State::PluginsRestart,
        "plugins-restart",
        WINDOW
    ));
    rules.push(Rule {
        id: "plugins-restart-banner-in",
        state: State::PluginsRestart,
        window: WINDOW,
        check: Check::InWindow("plugins.restart"),
        why: "restart banner visible",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-restart-button-in",
        state: State::PluginsRestart,
        window: WINDOW,
        check: Check::InWindow("plugins.restart.button"),
        why: "Restart button visible",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-restart-above-header",
        state: State::PluginsRestart,
        window: WINDOW,
        check: Check::Above {
            upper: "plugins.restart",
            lower: "plugins.search",
        },
        why: "restart banner sits above the header",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-narrow-search-in",
        state: State::PluginsNarrow,
        window: NARROW,
        check: Check::InWindow("plugins.search"),
        why: "search field visible at the narrow width",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-narrow-install-in",
        state: State::PluginsNarrow,
        window: NARROW,
        check: Check::InWindow("plugins.install-dev"),
        why: "Install Dev Extension button visible at the narrow width",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-narrow-tabs-order",
        state: State::PluginsNarrow,
        window: NARROW,
        check: Check::LeftOf("plugins.tab.installed", "plugins.tab.available"),
        why: "tabs keep their order when the header wraps",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-narrow-list-in",
        state: State::PluginsNarrow,
        window: NARROW,
        check: Check::InWindow("plugins.list"),
        why: "plugin list visible at the narrow width",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-narrow-header-above-list",
        state: State::PluginsNarrow,
        window: NARROW,
        check: Check::Above {
            upper: "plugins.install-dev",
            lower: "plugins.list",
        },
        why: "the wrapped header stays above the list",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-narrow-detail-in",
        state: State::PluginsNarrow,
        window: NARROW,
        check: Check::InWindow("plugins.detail"),
        why: "detail panel visible at the narrow width",
        known_bug: None,
    });
    rules.push(Rule {
        id: "plugins-narrow-status-in",
        state: State::PluginsNarrow,
        window: NARROW,
        check: Check::InWindow("plugins.status"),
        why: "status line visible at the narrow width",
        known_bug: None,
    });
    rules
}
