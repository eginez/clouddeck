//! Rule and Check types and their evaluation.
//! The test that checks all layout rules.
//! Geometric assertion helpers over debug bounds.

use gpui::{Bounds, Pixels, Size};

use super::geometry::{self, TOL};
use super::rules_bar;
use super::rules_main;
use super::rules_overlays;
use super::rules_search;
use super::rules_style;
use super::rules_table;
use super::rules_welcome;
use super::states::State;
use crate::ui_record::Recorded;

/// One geometric assertion over the debug-selector boxes of a named state.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Check {
    /// The selector is rendered.
    Present(&'static str),
    /// The selector is not rendered.
    Absent(&'static str),
    /// The box has positive width and height.
    NonZero(&'static str),
    /// The inner box lies within the outer box.
    Inside {
        inner: &'static str,
        outer: &'static str,
    },
    /// The box lies within the window.
    InWindow(&'static str),
    /// The two boxes do not overlap.
    NoOverlap(&'static str, &'static str),
    /// The upper box sits above the lower box.
    Above {
        upper: &'static str,
        lower: &'static str,
    },
    /// The two boxes share a left edge.
    LeftAligned(&'static str, &'static str),
    /// The two boxes share a right edge.
    RightAligned(&'static str, &'static str),
    /// The first box lies left of the second without overlapping it.
    LeftOf(&'static str, &'static str),
    /// The two boxes share a vertical centre.
    SameRow(&'static str, &'static str),
    /// The box's right edge is on the window's right edge.
    RightEdgeAtWindow(&'static str),
    /// The box's bottom edge is on the window's bottom edge.
    BottomEdgeAtWindow(&'static str),
    /// The box's left edge is on the window's left edge.
    LeftEdgeAtWindow(&'static str),
    /// The box width is within the given range.
    WidthBetween(&'static str, f32, f32),
    /// The box height is within the given range.
    HeightBetween(&'static str, f32, f32),
    /// The box is horizontally centred in the window.
    CenteredX(&'static str),
    /// The recorded text of the selector equals the given text.
    TextIs {
        target: &'static str,
        text: &'static str,
    },
    /// The recorded text colour token of the selector equals the given token.
    FgIs {
        target: &'static str,
        token: &'static str,
    },
    /// The recorded font of the selector equals the given font.
    FontIs {
        target: &'static str,
        font: &'static str,
    },
}

/// A layout assertion for one named state at one window size.
pub(crate) struct Rule {
    /// Unique identifier reported on failure.
    pub id: &'static str,
    /// The named state the rule is checked in.
    pub state: State,
    /// Window width and height in points.
    pub window: (f32, f32),
    /// The assertion itself.
    pub check: Check,
    /// Why the assertion matters, reported on failure.
    pub why: &'static str,
    /// Describes the real UI bug this rule currently trips on, if any.
    pub known_bug: Option<&'static str>,
}

fn get(
    lookup: &dyn Fn(&'static str) -> Option<Bounds<Pixels>>,
    sel: &'static str,
) -> Result<Bounds<Pixels>, String> {
    lookup(sel).ok_or_else(|| format!("{sel} was not rendered"))
}

fn recorded(
    records: &dyn Fn(&'static str) -> Option<Recorded>,
    sel: &'static str,
) -> Result<Recorded, String> {
    records(sel).ok_or_else(|| format!("{sel} recorded nothing"))
}

/// Evaluates `check` against the boxes reported by `lookup` and the render records reported by `records`.
pub(crate) fn evaluate(
    check: &Check,
    lookup: &dyn Fn(&'static str) -> Option<Bounds<Pixels>>,
    records: &dyn Fn(&'static str) -> Option<Recorded>,
    window: Size<Pixels>,
) -> Result<(), String> {
    match *check {
        Check::Present(sel) => get(lookup, sel).map(|_| ()),
        Check::Absent(sel) => match lookup(sel) {
            None => Ok(()),
            Some(_) => Err(format!("{sel} was rendered but must be absent")),
        },
        Check::NonZero(sel) => geometry::nonzero(sel, get(lookup, sel)?),
        Check::Inside { inner, outer } => {
            geometry::inside(inner, get(lookup, inner)?, outer, get(lookup, outer)?, TOL)
        }
        Check::InWindow(sel) => geometry::in_window(sel, get(lookup, sel)?, window, TOL),
        Check::NoOverlap(a, b) => geometry::no_overlap(a, get(lookup, a)?, b, get(lookup, b)?),
        Check::Above { upper, lower } => {
            geometry::above(upper, get(lookup, upper)?, lower, get(lookup, lower)?, TOL)
        }
        Check::LeftAligned(a, b) => {
            geometry::left_aligned(a, get(lookup, a)?, b, get(lookup, b)?, TOL)
        }
        Check::RightAligned(a, b) => {
            geometry::right_aligned(a, get(lookup, a)?, b, get(lookup, b)?, TOL)
        }
        Check::LeftOf(a, b) => geometry::left_of(a, get(lookup, a)?, b, get(lookup, b)?, 0.0),
        Check::SameRow(a, b) => geometry::same_row(a, get(lookup, a)?, b, get(lookup, b)?, TOL),
        Check::RightEdgeAtWindow(sel) => {
            geometry::right_edge_at_window(sel, get(lookup, sel)?, window, TOL)
        }
        Check::BottomEdgeAtWindow(sel) => {
            geometry::bottom_edge_at_window(sel, get(lookup, sel)?, window, TOL)
        }
        Check::LeftEdgeAtWindow(sel) => {
            geometry::left_edge_at_window(sel, get(lookup, sel)?, window, TOL)
        }
        Check::WidthBetween(sel, min, max) => {
            geometry::width_between(sel, get(lookup, sel)?, min, max)
        }
        Check::HeightBetween(sel, min, max) => {
            geometry::height_between(sel, get(lookup, sel)?, min, max)
        }
        Check::CenteredX(sel) => geometry::centered_x(sel, get(lookup, sel)?, window, TOL),
        Check::TextIs { target, text } => match recorded(records, target)?.text {
            Some(actual) if actual == text => Ok(()),
            actual => Err(format!("{target} text is {actual:?}, want {text:?}")),
        },
        Check::FgIs { target, token } => match recorded(records, target)?.fg {
            Some(actual) if actual == token => Ok(()),
            actual => Err(format!("{target} fg is {actual:?}, want {token:?}")),
        },
        Check::FontIs { target, font } => match recorded(records, target)?.font {
            Some(actual) if actual == font => Ok(()),
            actual => Err(format!("{target} font is {actual:?}, want {font:?}")),
        },
    }
}

/// Every layout rule, concatenated from the per-area tables.
pub(crate) fn all_rules() -> Vec<Rule> {
    [
        rules_main::rules(),
        rules_table::rules(),
        rules_bar::rules(),
        rules_search::rules(),
        rules_overlays::rules(),
        rules_welcome::rules(),
        rules_style::rules(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};
    use std::collections::HashMap;

    fn b(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn win() -> Size<Pixels> {
        size(px(1000.0), px(500.0))
    }

    fn check(boxes: &[(&'static str, Bounds<Pixels>)], check: Check) -> Result<(), String> {
        let map: HashMap<&'static str, Bounds<Pixels>> = boxes.iter().cloned().collect();
        evaluate(&check, &|sel| map.get(sel).cloned(), &|_| None, win())
    }

    fn check_records(
        records: &[(&'static str, Recorded)],
        check: Check,
    ) -> Result<(), String> {
        let map: HashMap<&'static str, Recorded> = records.iter().cloned().collect();
        evaluate(&check, &|_| None, &|sel| map.get(sel).cloned(), win())
    }

    fn text(text: &str) -> Recorded {
        Recorded {
            text: Some(text.to_string()),
            ..Recorded::default()
        }
    }

    #[test]
    fn text_is_ok_and_err() {
        let c = Check::TextIs {
            target: "bar.status",
            text: "Ready",
        };
        assert!(check_records(&[("bar.status", text("Ready"))], c).is_ok());
        let err = check_records(&[("bar.status", text("Busy"))], c).unwrap_err();
        assert!(err.contains("bar.status") && err.contains("Busy"), "{err}");
        let err = check_records(&[], c).unwrap_err();
        assert_eq!(err, "bar.status recorded nothing");
    }

    #[test]
    fn fg_is_ok_and_err() {
        let c = Check::FgIs {
            target: "bar.failure",
            token: "warntext",
        };
        let amber = Recorded {
            fg: Some("warntext"),
            ..Recorded::default()
        };
        let neutral = Recorded {
            fg: Some("dim"),
            ..Recorded::default()
        };
        assert!(check_records(&[("bar.failure", amber)], c).is_ok());
        let err = check_records(&[("bar.failure", neutral)], c).unwrap_err();
        assert!(err.contains("bar.failure") && err.contains("warntext"), "{err}");
        let err = check_records(&[], c).unwrap_err();
        assert_eq!(err, "bar.failure recorded nothing");
    }

    #[test]
    fn font_is_ok_and_err() {
        let c = Check::FontIs {
            target: "table.row.us-east-1:i-1.id",
            font: "mono",
        };
        let mono = Recorded {
            font: Some("mono"),
            ..Recorded::default()
        };
        let ui = Recorded {
            font: Some("ui"),
            ..Recorded::default()
        };
        assert!(check_records(&[("table.row.us-east-1:i-1.id", mono)], c).is_ok());
        let err = check_records(&[("table.row.us-east-1:i-1.id", ui)], c).unwrap_err();
        assert!(err.contains("mono"), "{err}");
        let err = check_records(&[], c).unwrap_err();
        assert_eq!(err, "table.row.us-east-1:i-1.id recorded nothing");
    }

    #[test]
    fn present_missing_is_an_error() {
        let err = check(&[], Check::Present("x")).unwrap_err();
        assert_eq!(err, "x was not rendered");
    }

    #[test]
    fn absent_present_is_an_error() {
        assert!(check(&[], Check::Absent("x")).is_ok());
        let err = check(&[("x", b(0.0, 0.0, 1.0, 1.0))], Check::Absent("x")).unwrap_err();
        assert!(err.contains("x"), "{err}");
    }

    #[test]
    fn inside_ok_and_err() {
        let outer = ("outer", b(0.0, 0.0, 100.0, 100.0));
        let ok = ("inner", b(10.0, 10.0, 20.0, 20.0));
        let bad = ("inner", b(90.0, 10.0, 20.0, 20.0));
        let c = Check::Inside {
            inner: "inner",
            outer: "outer",
        };
        assert!(check(&[outer, ok], c).is_ok());
        let err = check(&[outer, bad], c).unwrap_err();
        assert!(err.contains("inner") && err.contains("outer"), "{err}");
    }

    #[test]
    fn above_ok() {
        let boxes = [
            ("upper", b(0.0, 0.0, 10.0, 10.0)),
            ("lower", b(0.0, 10.0, 10.0, 10.0)),
        ];
        let c = Check::Above {
            upper: "upper",
            lower: "lower",
        };
        assert!(check(&boxes, c).is_ok());
    }

    #[test]
    fn width_between_err_names_the_selector() {
        let err = check(
            &[("table.col", b(0.0, 0.0, 500.0, 10.0))],
            Check::WidthBetween("table.col", 300.0, 420.0),
        )
        .unwrap_err();
        assert!(err.contains("table.col"), "{err}");
    }
}
