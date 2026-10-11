//! Pure responsive layout rules for the main window, derived from its size.

/// Window width below which the sidebar and inspector become overlays.
pub const NARROW_WIDTH: f32 = 760.0;
/// Window width below which the window is compact.
pub const COMPACT_WIDTH: f32 = 600.0;
/// Window height below which the window is short and popovers scroll.
pub const SHORT_HEIGHT: f32 = 600.0;

/// Which main-window elements are shown or hidden at a given window size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutRules {
    /// Width is below `NARROW_WIDTH`.
    pub narrow: bool,
    /// Width is below `COMPACT_WIDTH`.
    pub compact: bool,
    /// Height is below `SHORT_HEIGHT`.
    pub short: bool,
    /// Sidebar and inspector render as overlays over a scrim.
    pub panes_are_overlays: bool,
    /// Panes start open when a window opens or crosses the breakpoint.
    pub panes_open_by_default: bool,
    /// The window title is shown.
    pub show_title: bool,
    /// Search collapses to an icon.
    pub search_collapsed: bool,
    /// The status bar shows "Updated HH:MM" (before the caller's setting applies).
    pub show_updated_time: bool,
    /// The status bar shows "N shown".
    pub show_shown_count: bool,
    /// The status bar shows the instance count.
    pub show_instance_count: bool,
    /// The status bar shows refresh, inspector and sidebar icon buttons.
    pub show_status_bar_buttons: bool,
}

/// Layout rules for a window of the given size in points.
pub fn layout_rules(width: f32, height: f32) -> LayoutRules {
    let narrow = width < NARROW_WIDTH;
    let compact = width < COMPACT_WIDTH;
    LayoutRules {
        narrow,
        compact,
        short: height < SHORT_HEIGHT,
        panes_are_overlays: narrow,
        panes_open_by_default: !narrow,
        show_title: !compact,
        search_collapsed: compact,
        show_updated_time: width >= 720.0,
        show_shown_count: width >= 640.0,
        show_instance_count: !compact,
        show_status_bar_buttons: !compact,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_window_shows_everything() {
        for (width, height) in [(1280.0, 760.0), (960.0, 760.0)] {
            let rules = layout_rules(width, height);
            assert!(!rules.narrow);
            assert!(!rules.compact);
            assert!(!rules.short);
            assert!(!rules.panes_are_overlays);
            assert!(rules.panes_open_by_default);
            assert!(rules.show_title);
            assert!(!rules.search_collapsed);
            assert!(rules.show_updated_time);
            assert!(rules.show_shown_count);
            assert!(rules.show_instance_count);
            assert!(rules.show_status_bar_buttons);
        }
    }

    #[test]
    fn narrow_breakpoint_is_strictly_below_760() {
        let at = layout_rules(760.0, 760.0);
        assert!(!at.narrow);

        let below = layout_rules(759.0, 760.0);
        assert!(below.narrow);
        assert!(below.panes_are_overlays);
        assert!(!below.panes_open_by_default);
        assert!(!below.compact);
        assert!(below.show_updated_time);
    }

    #[test]
    fn width_700_hides_updated_time_only_below_720() {
        let rules = layout_rules(700.0, 760.0);
        assert!(rules.narrow);
        assert!(!rules.show_updated_time);
        assert!(rules.show_shown_count);
    }

    #[test]
    fn width_639_hides_shown_count() {
        let rules = layout_rules(639.0, 760.0);
        assert!(!rules.show_shown_count);
    }

    #[test]
    fn status_bar_thresholds_are_inclusive_at_720_and_640() {
        assert!(layout_rules(720.0, 760.0).show_updated_time);
        assert!(!layout_rules(719.0, 760.0).show_updated_time);
        assert!(layout_rules(640.0, 760.0).show_shown_count);
        assert!(!layout_rules(639.0, 760.0).show_shown_count);
    }

    #[test]
    fn compact_window_keeps_only_essentials() {
        let rules = layout_rules(520.0, 760.0);
        assert!(rules.compact);
        assert!(!rules.show_title);
        assert!(rules.search_collapsed);
        assert!(!rules.show_instance_count);
        assert!(!rules.show_status_bar_buttons);
    }

    #[test]
    fn short_window_is_detected_by_height() {
        let rules = layout_rules(1280.0, 480.0);
        assert!(rules.short);
        assert!(!rules.narrow);
    }

    #[test]
    fn breakpoints_at_600_are_not_compact_or_short() {
        let rules = layout_rules(600.0, 600.0);
        assert!(!rules.compact);
        assert!(!rules.short);
    }
}
