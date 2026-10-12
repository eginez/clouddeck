//! Text width measurement.
//! Headless layout/behaviour test harness (epic clouddeck-rs-mac-ui.69). Test-only.

use gpui::{TextRun, Window, black, font, px};

/// Shaped width in points of `text` drawn as one line in `font_family` at `font_size`.
pub(crate) fn text_width(
    window: &mut Window,
    text: &str,
    font_size: f32,
    font_family: &str,
) -> f32 {
    let run = TextRun {
        len: text.len(),
        font: font(font_family.to_string()),
        color: black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped =
        window
            .text_system()
            .shape_line(text.to_string().into(), px(font_size), &[run], None);
    f32::from(shaped.width)
}

/// Ok when `text` shaped at `font_size` is no wider than `available` points.
pub(crate) fn fits(
    window: &mut Window,
    text: &str,
    font_size: f32,
    font_family: &str,
    available: f32,
) -> Result<(), String> {
    let width = text_width(window, text, font_size, font_family);
    if width <= available {
        Ok(())
    } else {
        Err(format!(
            "\"{text}\" needs {width:.1}pt, has {available:.1}pt"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::UI_FONT;
    use gpui::TestAppContext;

    #[gpui::test]
    #[ignore = "gpui test platform uses NoopTextSystem: every glyph advances a fixed 0.6em, so widths are per-char, not per-glyph"]
    fn measured_widths_are_realistic(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _cx| {
            let w_wide = text_width(window, "WWWWWWWW", 12.0, UI_FONT);
            let w_narrow = text_width(window, "iiiiiiii", 12.0, UI_FONT);
            let abc_12 = text_width(window, "abc", 12.0, UI_FONT);
            let abc_24 = text_width(window, "abc", 24.0, UI_FONT);
            assert!(
                w_wide > w_narrow,
                "W width {w_wide} should exceed i width {w_narrow}"
            );
            assert!(w_narrow > 0.0, "i width {w_narrow} should be positive");
            assert!(
                abc_24 > abc_12,
                "abc@24 {abc_24} should exceed abc@12 {abc_12}"
            );
            assert!(abc_12 > 0.0, "abc@12 {abc_12} should be positive");
        });
    }

    #[gpui::test]
    #[ignore = "gpui test platform uses NoopTextSystem: every glyph advances a fixed 0.6em, so widths are per-char, not per-glyph"]
    fn cmd_f_fits_in_40pt_at_11pt(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _cx| {
            assert!(fits(window, "⌘F", 11.0, UI_FONT, 40.0).is_ok());
        });
    }

    #[gpui::test]
    #[ignore = "gpui test platform uses NoopTextSystem: every glyph advances a fixed 0.6em, so widths are per-char, not per-glyph"]
    fn long_search_hint_does_not_fit_in_60pt_at_12pt(cx: &mut TestAppContext) {
        let cx = cx.add_empty_window();
        cx.update(|window, _cx| {
            let err = fits(
                window,
                "Search or filter, e.g. state:stopped",
                12.0,
                UI_FONT,
                60.0,
            )
            .unwrap_err();
            assert!(
                err.contains("Search or filter, e.g. state:stopped"),
                "{err}"
            );
            assert!(err.contains("has 60.0pt"), "{err}");
        });
    }
}
