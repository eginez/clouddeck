//! Port of `SurgicalTheme.swift` (the Swift CloudDeck oracle's shared
//! design tokens) to gpui.
//!
//! gpui has no SwiftUI `Color` / `@Environment` equivalent, so the Swift
//! `enum SurgicalTheme { static let ... }` tokens become plain Rust
//! constants typed as [`gpui::Hsla`] (via [`gpui::rgb`], which matches the
//! Swift file's `Color(red:green:blue:)` initializers channel-for-channel).
//! This module intentionally contains tokens/constants only — no view
//! rendering. See
//! `~/src/cloudDeck/main/Sources/CloudDeck/Views/Shared/SurgicalTheme.swift`
//! for the oracle this was ported from.

use gpui::{Hsla, rgb};

/// Tonal surface tiers, text/outline colors, accent colors, and corner
/// radii, matching `enum SurgicalTheme` in the Swift oracle.
pub struct SurgicalTheme;

impl SurgicalTheme {
    pub fn workspace_canvas() -> Hsla {
        rgb(0xf9f9fc).into()
    }
    // Not yet exercised by any ported view — part of the oracle's full
    // text-color palette (`SurgicalTheme.textTertiary`), kept for parity
    // and future use.
    #[allow(dead_code)]
    pub fn text_tertiary() -> Hsla {
        rgb(0x8087a1).into()
    }

    // Accent / state colors.
    pub fn warning() -> Hsla {
        rgb(0xbe8519).into()
    }
}

/// Corner radii, matching `enum SurgicalTheme.Radius` in the Swift oracle
/// (values are logical pixels, mirrored here as `f32` since gpui's
/// `Pixels`/`px()` are context-dependent on the caller).
pub mod radius {
    // Not yet used by any ported view — part of the oracle's full radius
    // scale (`SurgicalTheme.Radius.small`), kept for parity and future use.
    #[allow(dead_code)]
    pub const SMALL: f32 = 2.0;
}

/// Font sizes/weights, matching `enum SurgicalTypography` in the Swift
/// oracle. gpui text styling is applied per-element (via `.text_size` /
/// `.font_weight` on a `Div`/`Text` builder) rather than through a shared
/// `Font` value type, so these are exposed as plain size/weight pairs
/// callers can apply directly.
pub struct SurgicalTypography;

impl SurgicalTypography {
    // Not yet used by any ported view — part of the oracle's full type
    // scale (`SurgicalTypography.sectionTitle`), kept for parity and future
    // use.
    #[allow(dead_code)]
    pub const SECTION_TITLE_SIZE: f32 = 18.0;
}
