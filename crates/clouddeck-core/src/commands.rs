//! Command palette: every app action, its title, macOS shortcut and enabled rule.

/// An app action listed in the command palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandId {
    SwitchProfile,
    SwitchRegion,
    Refresh,
    ToggleSidebar,
    ToggleInspector,
    FilterRunning,
    FilterStopped,
    FilterPending,
    ClearFilters,
    CopyId,
    CopyPrivateIp,
    CopyPublicIp,
    OpenInConsole,
    RetryFailedRegion,
    ConnectToAws,
    OpenSettings,
}

impl CommandId {
    /// Every command in palette order.
    pub const ALL: [CommandId; 16] = [
        CommandId::SwitchProfile,
        CommandId::SwitchRegion,
        CommandId::Refresh,
        CommandId::ToggleSidebar,
        CommandId::ToggleInspector,
        CommandId::FilterRunning,
        CommandId::FilterStopped,
        CommandId::FilterPending,
        CommandId::ClearFilters,
        CommandId::CopyId,
        CommandId::CopyPrivateIp,
        CommandId::CopyPublicIp,
        CommandId::OpenInConsole,
        CommandId::RetryFailedRegion,
        CommandId::ConnectToAws,
        CommandId::OpenSettings,
    ];

    /// Palette label for the command.
    pub fn title(self) -> &'static str {
        match self {
            CommandId::SwitchProfile => "View: Switch Profile…",
            CommandId::SwitchRegion => "View: Switch Region…",
            CommandId::Refresh => "Fleet: Refresh",
            CommandId::ToggleSidebar => "View: Toggle Sidebar",
            CommandId::ToggleInspector => "View: Toggle Inspector",
            CommandId::FilterRunning => "Filter: Running Only",
            CommandId::FilterStopped => "Filter: Stopped Only",
            CommandId::FilterPending => "Filter: Pending Only",
            CommandId::ClearFilters => "Filter: Clear All",
            CommandId::CopyId => "Instance: Copy ID",
            CommandId::CopyPrivateIp => "Instance: Copy Private IP",
            CommandId::CopyPublicIp => "Instance: Copy Public IP",
            CommandId::OpenInConsole => "Instance: Open in Console",
            CommandId::RetryFailedRegion => "Fleet: Retry Failed Region",
            CommandId::ConnectToAws => "Account: Connect to AWS…",
            CommandId::OpenSettings => "Preferences: Open Settings…",
        }
    }

    /// macOS shortcut glyphs, or `None` when the command has no shortcut.
    pub fn shortcut(self) -> Option<&'static str> {
        match self {
            CommandId::SwitchProfile => Some("⌘P"),
            CommandId::SwitchRegion => Some("⌘⇧R"),
            CommandId::Refresh => Some("⌘R"),
            CommandId::ToggleSidebar => Some("⌘B"),
            CommandId::ToggleInspector => Some("⌘⌥I"),
            CommandId::CopyId => Some("⌘C"),
            CommandId::OpenInConsole => Some("⌘O"),
            CommandId::OpenSettings => Some("⌘,"),
            CommandId::FilterRunning
            | CommandId::FilterStopped
            | CommandId::FilterPending
            | CommandId::ClearFilters
            | CommandId::CopyPrivateIp
            | CommandId::CopyPublicIp
            | CommandId::RetryFailedRegion
            | CommandId::ConnectToAws => None,
        }
    }

    /// Whether the command applies in `ctx`; disabled commands stay listed in the palette.
    pub fn is_enabled(self, ctx: &CommandContext) -> bool {
        match self {
            CommandId::ClearFilters => !ctx.query_is_empty,
            CommandId::CopyId | CommandId::OpenInConsole => ctx.has_selection,
            CommandId::CopyPrivateIp => ctx.has_selection && ctx.selection_has_private_ip,
            CommandId::CopyPublicIp => ctx.has_selection && ctx.selection_has_public_ip,
            CommandId::RetryFailedRegion => ctx.has_failed_region,
            CommandId::SwitchProfile
            | CommandId::SwitchRegion
            | CommandId::Refresh
            | CommandId::ToggleSidebar
            | CommandId::ToggleInspector
            | CommandId::FilterRunning
            | CommandId::FilterStopped
            | CommandId::FilterPending
            | CommandId::ConnectToAws
            | CommandId::OpenSettings => true,
        }
    }
}

/// Selection and query state that decides which palette commands are enabled.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CommandContext {
    pub has_selection: bool,
    pub selection_has_private_ip: bool,
    pub selection_has_public_ip: bool,
    pub query_is_empty: bool,
    pub has_failed_region: bool,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::CommandId::*;
    use super::*;

    const IDLE: CommandContext = CommandContext {
        has_selection: false,
        selection_has_private_ip: false,
        selection_has_public_ip: false,
        query_is_empty: true,
        has_failed_region: false,
    };

    fn enabled(ctx: &CommandContext) -> HashSet<CommandId> {
        CommandId::ALL
            .into_iter()
            .filter(|command| command.is_enabled(ctx))
            .collect()
    }

    #[test]
    fn all_lists_sixteen_titles_in_palette_order() {
        assert_eq!(CommandId::ALL.len(), 16);
        let titles: Vec<&str> = CommandId::ALL
            .iter()
            .map(|command| command.title())
            .collect();
        assert_eq!(
            titles,
            [
                "View: Switch Profile…",
                "View: Switch Region…",
                "Fleet: Refresh",
                "View: Toggle Sidebar",
                "View: Toggle Inspector",
                "Filter: Running Only",
                "Filter: Stopped Only",
                "Filter: Pending Only",
                "Filter: Clear All",
                "Instance: Copy ID",
                "Instance: Copy Private IP",
                "Instance: Copy Public IP",
                "Instance: Open in Console",
                "Fleet: Retry Failed Region",
                "Account: Connect to AWS…",
                "Preferences: Open Settings…",
            ]
        );
    }

    #[test]
    fn switch_profile_title_uses_ellipsis_character() {
        let title = SwitchProfile.title();
        assert!(title.ends_with('…'));
        assert!(!title.contains("..."));
    }

    #[test]
    fn shortcuts_match_macos_glyphs() {
        assert_eq!(SwitchProfile.shortcut(), Some("⌘P"));
        assert_eq!(SwitchRegion.shortcut(), Some("⌘⇧R"));
        assert_eq!(ToggleInspector.shortcut(), Some("⌘⌥I"));
        assert_eq!(OpenSettings.shortcut(), Some("⌘,"));
        assert_eq!(CopyId.shortcut(), Some("⌘C"));
        assert_eq!(FilterRunning.shortcut(), None);
        assert_eq!(RetryFailedRegion.shortcut(), None);
    }

    #[test]
    fn exactly_eight_commands_have_a_shortcut() {
        let with_shortcut = CommandId::ALL
            .iter()
            .filter(|command| command.shortcut().is_some())
            .count();
        assert_eq!(with_shortcut, 8);
    }

    #[test]
    fn idle_context_enables_only_unconditional_commands() {
        let expected: HashSet<CommandId> = [
            SwitchProfile,
            SwitchRegion,
            Refresh,
            ToggleSidebar,
            ToggleInspector,
            FilterRunning,
            FilterStopped,
            FilterPending,
            ConnectToAws,
            OpenSettings,
        ]
        .into_iter()
        .collect();
        assert_eq!(enabled(&IDLE), expected);
    }

    #[test]
    fn selection_with_private_ip_enables_private_copy_but_not_public() {
        let ctx = CommandContext {
            has_selection: true,
            selection_has_private_ip: true,
            selection_has_public_ip: false,
            query_is_empty: true,
            has_failed_region: false,
        };
        let set = enabled(&ctx);
        assert!(set.contains(&CopyId));
        assert!(set.contains(&CopyPrivateIp));
        assert!(set.contains(&OpenInConsole));
        assert!(!set.contains(&CopyPublicIp));
    }

    #[test]
    fn public_ip_copy_requires_selection_even_when_flagged() {
        let ctx = CommandContext {
            has_selection: false,
            selection_has_private_ip: false,
            selection_has_public_ip: true,
            query_is_empty: true,
            has_failed_region: false,
        };
        assert!(!CopyPublicIp.is_enabled(&ctx));
    }

    #[test]
    fn clear_filters_enabled_only_when_query_is_not_empty() {
        let ctx = CommandContext {
            query_is_empty: false,
            ..IDLE
        };
        assert!(ClearFilters.is_enabled(&ctx));
        assert!(!ClearFilters.is_enabled(&IDLE));
    }

    #[test]
    fn retry_failed_region_enabled_only_when_a_region_failed() {
        let ctx = CommandContext {
            has_failed_region: true,
            ..IDLE
        };
        assert!(RetryFailedRegion.is_enabled(&ctx));
        assert!(!RetryFailedRegion.is_enabled(&IDLE));
    }
}
