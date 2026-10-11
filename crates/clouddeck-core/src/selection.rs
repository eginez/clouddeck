//! Pure keyboard selection rules for the main window table and the Escape key chain.

/// Next selected row key for an arrow or j/k press, clamped at the ends of the list.
pub fn move_selection(
    visible_keys: &[String],
    current: Option<&str>,
    down: bool,
) -> Option<String> {
    if visible_keys.is_empty() {
        return None;
    }
    let index = current.and_then(|key| visible_keys.iter().position(|candidate| candidate == key));
    let next = match index {
        None => 0,
        Some(i) if down => (i + 1).min(visible_keys.len() - 1),
        Some(i) => i.saturating_sub(1),
    };
    Some(visible_keys[next].clone())
}

/// What the Escape key does, in priority order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeAction {
    /// Close the open popover, menu or palette.
    CloseOverlay,
    /// Clear the search query.
    ClearSearch,
    /// Close the sidebar overlay in a narrow window.
    CloseSidebarOverlay,
    /// Clear the table selection, which also closes the inspector.
    ClearSelection,
    /// Escape has no effect.
    Nothing,
}

/// Window and focus state that decides the Escape action.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EscapeContext {
    /// A popover, menu or palette is open.
    pub overlay_open: bool,
    /// The search field has focus.
    pub search_focused: bool,
    /// The search query is empty.
    pub query_empty: bool,
    /// The window is narrow.
    pub narrow: bool,
    /// The sidebar is open.
    pub sidebar_open: bool,
    /// A table row is selected.
    pub has_selection: bool,
}

/// The action Escape performs for the given context.
pub fn escape_action(ctx: &EscapeContext) -> EscapeAction {
    if ctx.overlay_open {
        EscapeAction::CloseOverlay
    } else if ctx.search_focused {
        if ctx.query_empty {
            EscapeAction::Nothing
        } else {
            EscapeAction::ClearSearch
        }
    } else if ctx.narrow && ctx.sidebar_open {
        EscapeAction::CloseSidebarOverlay
    } else if ctx.has_selection {
        EscapeAction::ClearSelection
    } else {
        EscapeAction::Nothing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys() -> Vec<String> {
        vec!["a".to_string(), "b".to_string(), "c".to_string()]
    }

    #[test]
    fn move_selection_without_current_selects_first_in_both_directions() {
        assert_eq!(move_selection(&keys(), None, true), Some("a".to_string()));
        assert_eq!(move_selection(&keys(), None, false), Some("a".to_string()));
    }

    #[test]
    fn move_selection_steps_down_and_clamps_at_end() {
        assert_eq!(
            move_selection(&keys(), Some("a"), true),
            Some("b".to_string())
        );
        assert_eq!(
            move_selection(&keys(), Some("c"), true),
            Some("c".to_string())
        );
    }

    #[test]
    fn move_selection_steps_up_and_clamps_at_start() {
        assert_eq!(
            move_selection(&keys(), Some("b"), false),
            Some("a".to_string())
        );
        assert_eq!(
            move_selection(&keys(), Some("a"), false),
            Some("a".to_string())
        );
    }

    #[test]
    fn move_selection_unknown_current_selects_first() {
        assert_eq!(
            move_selection(&keys(), Some("zz"), true),
            Some("a".to_string())
        );
    }

    #[test]
    fn move_selection_empty_list_returns_none() {
        assert_eq!(move_selection(&[], None, true), None);
    }

    #[test]
    fn escape_overlay_closes_first() {
        let ctx = EscapeContext {
            overlay_open: true,
            search_focused: true,
            ..Default::default()
        };
        assert_eq!(escape_action(&ctx), EscapeAction::CloseOverlay);
    }

    #[test]
    fn escape_clears_non_empty_focused_search() {
        let ctx = EscapeContext {
            search_focused: true,
            query_empty: false,
            ..Default::default()
        };
        assert_eq!(escape_action(&ctx), EscapeAction::ClearSearch);
    }

    #[test]
    fn escape_on_empty_focused_search_does_nothing() {
        let ctx = EscapeContext {
            search_focused: true,
            query_empty: true,
            ..Default::default()
        };
        assert_eq!(escape_action(&ctx), EscapeAction::Nothing);
    }

    #[test]
    fn escape_closes_sidebar_overlay_in_narrow_window() {
        let ctx = EscapeContext {
            narrow: true,
            sidebar_open: true,
            has_selection: true,
            ..Default::default()
        };
        assert_eq!(escape_action(&ctx), EscapeAction::CloseSidebarOverlay);
    }

    #[test]
    fn escape_clears_selection_when_present() {
        let ctx = EscapeContext {
            has_selection: true,
            ..Default::default()
        };
        assert_eq!(escape_action(&ctx), EscapeAction::ClearSelection);
    }

    #[test]
    fn escape_clears_selection_in_narrow_window_without_open_sidebar() {
        let ctx = EscapeContext {
            narrow: true,
            has_selection: true,
            ..Default::default()
        };
        assert_eq!(escape_action(&ctx), EscapeAction::ClearSelection);
    }

    #[test]
    fn escape_ignores_query_when_search_not_focused() {
        let ctx = EscapeContext {
            query_empty: false,
            ..Default::default()
        };
        assert_eq!(escape_action(&ctx), EscapeAction::Nothing);
    }

    #[test]
    fn escape_default_context_does_nothing() {
        assert_eq!(
            escape_action(&EscapeContext::default()),
            EscapeAction::Nothing
        );
    }
}
