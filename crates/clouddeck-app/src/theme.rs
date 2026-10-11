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
    // Tonal surface tiers (lightest/neutral -> most saturated/contrast).
    pub fn surface() -> Hsla {
        rgb(0xfafafc).into()
    }
    pub fn surface_low() -> Hsla {
        rgb(0xf1f2f8).into()
    }
    pub fn surface_container() -> Hsla {
        rgb(0xe7e9f2).into()
    }
    pub fn surface_high() -> Hsla {
        rgb(0xdadeeb).into()
    }
    pub fn workspace_canvas() -> Hsla {
        rgb(0xf9f9fc).into()
    }
    pub fn chrome_dark() -> Hsla {
        rgb(0xd6dae7).into()
    }

    // Text / outline colors.
    pub fn text_primary() -> Hsla {
        rgb(0x0a0d14).into()
    }
    pub fn text_secondary() -> Hsla {
        rgb(0x474f66).into()
    }
    // Not yet exercised by any ported view — part of the oracle's full
    // text-color palette (`SurgicalTheme.textTertiary`), kept for parity
    // and future use.
    #[allow(dead_code)]
    pub fn text_tertiary() -> Hsla {
        rgb(0x8087a1).into()
    }

    // Accent / state colors.
    pub fn primary() -> Hsla {
        rgb(0x005bc1).into()
    }
    pub fn error() -> Hsla {
        rgb(0xb74743).into()
    }
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
    pub const STANDARD: f32 = 4.0;
    pub const LARGE: f32 = 12.0;
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
    pub const BODY_SIZE: f32 = 14.0;
    pub const BODY_EMPHASIS_SIZE: f32 = 14.0;
    pub const CAPTION_SIZE: f32 = 12.0;
    pub const LABEL_SIZE: f32 = 11.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_tiers_are_distinct() {
        let tiers = [
            SurgicalTheme::surface(),
            SurgicalTheme::surface_low(),
            SurgicalTheme::surface_container(),
            SurgicalTheme::surface_high(),
        ];
        for i in 0..tiers.len() {
            for j in (i + 1)..tiers.len() {
                assert_ne!(tiers[i], tiers[j], "tiers {i} and {j} should differ");
            }
        }
    }
}
