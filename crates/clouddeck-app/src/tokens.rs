//! Colour palette (light/dark) and layout sizes from design/main-window.html.

use clouddeck_core::Theme;
use gpui::{App, Global, Hsla, rgb};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub page: Hsla,
    pub win: Hsla,
    pub chrome: Hsla,
    pub side: Hsla,
    pub panel: Hsla,
    pub surf: Hsla,
    pub line: Hsla,
    pub line2: Hsla,
    pub text: Hsla,
    pub dim: Hsla,
    pub faint: Hsla,
    pub hover: Hsla,
    pub sel: Hsla,
    pub selbar: Hsla,
    pub accent: Hsla,
    pub run: Hsla,
    pub runtext: Hsla,
    pub stop: Hsla,
    pub warn: Hsla,
    pub warntext: Hsla,
    pub err: Hsla,
    pub tagbg: Hsla,
    pub badge: Hsla,
}

impl Palette {
    pub fn light() -> Self {
        Self {
            page: rgb(0xc9c9cf).into(),
            win: rgb(0xfbfbfc).into(),
            chrome: rgb(0xececef).into(),
            side: rgb(0xf1f1f4).into(),
            panel: rgb(0xf7f7f9).into(),
            surf: rgb(0xffffff).into(),
            line: rgb(0xdcdce1).into(),
            line2: rgb(0xe8e8ec).into(),
            text: rgb(0x1c1c1f).into(),
            dim: rgb(0x6b6b73).into(),
            faint: rgb(0x6f6f79).into(),
            hover: rgb(0xececf0).into(),
            sel: rgb(0xdfe6f5).into(),
            selbar: rgb(0x2f6fdb).into(),
            accent: rgb(0x2f6fdb).into(),
            run: rgb(0x1f9d55).into(),
            runtext: rgb(0x177a42).into(),
            stop: rgb(0x8a8a93).into(),
            warn: rgb(0xb7791f).into(),
            warntext: rgb(0x8a5a0a).into(),
            err: rgb(0xc4372c).into(),
            tagbg: rgb(0xf3f3f6).into(),
            badge: rgb(0x8b93a1).into(),
        }
    }

    pub fn dark() -> Self {
        Self {
            page: rgb(0x101012).into(),
            win: rgb(0x1e1f23).into(),
            chrome: rgb(0x2b2c31).into(),
            side: rgb(0x25262a).into(),
            panel: rgb(0x222327).into(),
            surf: rgb(0x2a2b30).into(),
            line: rgb(0x3a3b41).into(),
            line2: rgb(0x32333a).into(),
            text: rgb(0xececf0).into(),
            dim: rgb(0xa0a0aa).into(),
            faint: rgb(0x8e8e99).into(),
            hover: rgb(0x32333a).into(),
            sel: rgb(0x2b3a5c).into(),
            selbar: rgb(0x5b8def).into(),
            accent: rgb(0x5b8def).into(),
            run: rgb(0x3ecf7a).into(),
            runtext: rgb(0x5fd391).into(),
            stop: rgb(0x8a8a93).into(),
            warn: rgb(0xe3b04b).into(),
            warntext: rgb(0xe3b04b).into(),
            err: rgb(0xff6b5e).into(),
            tagbg: rgb(0x33343a).into(),
            badge: rgb(0x5b6272).into(),
        }
    }

    pub fn for_dark(dark: bool) -> Self {
        if dark { Self::dark() } else { Self::light() }
    }

    pub fn state_dot(&self, state: &str) -> Hsla {
        match state.to_ascii_lowercase().as_str() {
            "running" => self.run,
            "pending" => self.warn,
            _ => self.stop,
        }
    }

    #[cfg(test)]
    pub fn state_text(&self, state: &str) -> Hsla {
        match state.to_ascii_lowercase().as_str() {
            "running" => self.runtext,
            "pending" => self.warntext,
            _ => self.dim,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActivePalette(pub Palette);

impl Global for ActivePalette {}

pub fn palette(cx: &App) -> Palette {
    cx.try_global::<ActivePalette>()
        .map_or_else(Palette::light, |active| active.0)
}

pub fn is_dark(theme: Theme, system_dark: bool) -> bool {
    match theme {
        Theme::FollowSystem => system_dark,
        Theme::Light => false,
        Theme::Dark => true,
    }
}

pub const TOOLBAR_HEIGHT: f32 = 44.0;
pub const STATUS_BAR_HEIGHT: f32 = 24.0;
pub const ROW_HEIGHT: f32 = 28.0;
pub const SIDEBAR_WIDTH: f32 = 210.0;
pub const INSPECTOR_WIDTH: f32 = 340.0;
pub const FONT_SIZE: f32 = 12.0;
pub const STATUS_FONT_SIZE: f32 = 11.0;

pub const UI_FONT: &str = ".SystemUIFont";
pub const MONO_FONT: &str = "Menlo";

#[cfg(test)]
mod tests {
    use super::*;

    fn hsla(hex: u32) -> Hsla {
        rgb(hex).into()
    }

    #[test]
    fn light_and_dark_palettes_hold_design_values() {
        assert_eq!(Palette::light().win, hsla(0xfbfbfc));
        assert_eq!(Palette::light().run, hsla(0x1f9d55));
        assert_eq!(Palette::dark().accent, hsla(0x5b8def));
        assert_eq!(Palette::dark().err, hsla(0xff6b5e));
    }

    #[test]
    fn for_dark_selects_palette() {
        assert_eq!(Palette::for_dark(true), Palette::dark());
        assert_eq!(Palette::for_dark(false), Palette::light());
    }

    #[test]
    fn theme_setting_decides_dark_mode() {
        assert!(is_dark(Theme::FollowSystem, true));
        assert!(!is_dark(Theme::FollowSystem, false));
        assert!(!is_dark(Theme::Light, true));
        assert!(is_dark(Theme::Dark, false));
    }

    #[test]
    fn state_dot_maps_states_case_insensitively() {
        let palette = Palette::light();
        assert_eq!(palette.state_dot("Running"), palette.run);
        assert_eq!(palette.state_dot("pending"), palette.warn);
        assert_eq!(palette.state_dot("stopped"), palette.stop);
    }

    #[test]
    fn state_text_maps_states_case_insensitively() {
        let palette = Palette::light();
        assert_eq!(palette.state_text("running"), palette.runtext);
        assert_eq!(palette.state_text("PENDING"), palette.warntext);
        assert_eq!(palette.state_text("stopped"), palette.dim);
    }

    #[test]
    fn layout_sizes_match_design() {
        assert_eq!(TOOLBAR_HEIGHT, 44.0);
        assert_eq!(STATUS_BAR_HEIGHT, 24.0);
        assert_eq!(ROW_HEIGHT, 28.0);
        assert_eq!(SIDEBAR_WIDTH, 210.0);
        assert_eq!(INSPECTOR_WIDTH, 340.0);
        assert_eq!(FONT_SIZE, 12.0);
        assert_eq!(STATUS_FONT_SIZE, 11.0);
    }
}
