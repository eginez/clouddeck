//! Geometric assertion helpers over debug bounds.

use gpui::{Bounds, Pixels, Size};

/// Default tolerance in points for edge comparisons.
pub(crate) const TOL: f32 = 2.0;

#[derive(Clone, Copy)]
struct Edges {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Edges {
    fn of(b: Bounds<Pixels>) -> Self {
        let left = f32::from(b.origin.x);
        let top = f32::from(b.origin.y);
        Self {
            left,
            top,
            right: left + f32::from(b.size.width),
            bottom: top + f32::from(b.size.height),
        }
    }

    fn width(self) -> f32 {
        self.right - self.left
    }

    fn height(self) -> f32 {
        self.bottom - self.top
    }

    fn within(self, outer: Edges, tol: f32) -> bool {
        self.left >= outer.left - tol
            && self.top >= outer.top - tol
            && self.right <= outer.right + tol
            && self.bottom <= outer.bottom + tol
    }
}

fn window_edges(window: Size<Pixels>) -> Edges {
    Edges {
        left: 0.0,
        top: 0.0,
        right: f32::from(window.width),
        bottom: f32::from(window.height),
    }
}

fn describe(name: &str, b: Bounds<Pixels>) -> String {
    let e = Edges::of(b);
    format!("{name} ({},{} {}x{})", e.left, e.top, e.width(), e.height())
}

/// Passes when the box has positive width and height.
pub(crate) fn nonzero(name: &str, b: Bounds<Pixels>) -> Result<(), String> {
    let e = Edges::of(b);
    if e.width() > 0.0 && e.height() > 0.0 {
        Ok(())
    } else {
        Err(format!("{} has zero size", describe(name, b)))
    }
}

/// Passes when `a` lies fully within `b`, allowing `tol` on each edge.
pub(crate) fn inside(
    inner: &str,
    a: Bounds<Pixels>,
    outer: &str,
    b: Bounds<Pixels>,
    tol: f32,
) -> Result<(), String> {
    if Edges::of(a).within(Edges::of(b), tol) {
        Ok(())
    } else {
        Err(format!(
            "{} is not inside {}, tolerance {tol}",
            describe(inner, a),
            describe(outer, b)
        ))
    }
}

/// Passes when `a` lies fully within the window rectangle (0,0)-(w,h), allowing `tol`.
pub(crate) fn in_window(
    name: &str,
    a: Bounds<Pixels>,
    window: Size<Pixels>,
    tol: f32,
) -> Result<(), String> {
    if Edges::of(a).within(window_edges(window), tol) {
        Ok(())
    } else {
        Err(format!(
            "{} is not inside the window {}x{}, tolerance {tol}",
            describe(name, a),
            f32::from(window.width),
            f32::from(window.height)
        ))
    }
}

/// Passes when the two boxes share no area; touching edges are allowed.
pub(crate) fn no_overlap(
    a_name: &str,
    a: Bounds<Pixels>,
    b_name: &str,
    b: Bounds<Pixels>,
) -> Result<(), String> {
    let (ea, eb) = (Edges::of(a), Edges::of(b));
    let overlap_x = ea.right.min(eb.right) - ea.left.max(eb.left);
    let overlap_y = ea.bottom.min(eb.bottom) - ea.top.max(eb.top);
    if overlap_x > 0.0 && overlap_y > 0.0 {
        Err(format!(
            "{} overlaps {}",
            describe(a_name, a),
            describe(b_name, b)
        ))
    } else {
        Ok(())
    }
}

/// Passes when the upper box's bottom edge is at or above the lower box's top edge, within `tol`.
pub(crate) fn above(
    upper_name: &str,
    a: Bounds<Pixels>,
    lower_name: &str,
    b: Bounds<Pixels>,
    tol: f32,
) -> Result<(), String> {
    if Edges::of(a).bottom <= Edges::of(b).top + tol {
        Ok(())
    } else {
        Err(format!(
            "{} is not above {}, tolerance {tol}",
            describe(upper_name, a),
            describe(lower_name, b)
        ))
    }
}

/// Passes when the left edges of the two boxes match within `tol`.
pub(crate) fn left_aligned(
    a_name: &str,
    a: Bounds<Pixels>,
    b_name: &str,
    b: Bounds<Pixels>,
    tol: f32,
) -> Result<(), String> {
    if (Edges::of(a).left - Edges::of(b).left).abs() <= tol {
        Ok(())
    } else {
        Err(format!(
            "{} and {} are not left-aligned, tolerance {tol}",
            describe(a_name, a),
            describe(b_name, b)
        ))
    }
}

/// Passes when the right edges of the two boxes match within `tol`.
pub(crate) fn right_aligned(
    a_name: &str,
    a: Bounds<Pixels>,
    b_name: &str,
    b: Bounds<Pixels>,
    tol: f32,
) -> Result<(), String> {
    if (Edges::of(a).right - Edges::of(b).right).abs() <= tol {
        Ok(())
    } else {
        Err(format!(
            "{} and {} are not right-aligned, tolerance {tol}",
            describe(a_name, a),
            describe(b_name, b)
        ))
    }
}

/// Passes when the box's right edge sits on the window's right edge within `tol`.
pub(crate) fn right_edge_at_window(
    name: &str,
    a: Bounds<Pixels>,
    window: Size<Pixels>,
    tol: f32,
) -> Result<(), String> {
    let w = f32::from(window.width);
    if (Edges::of(a).right - w).abs() <= tol {
        Ok(())
    } else {
        Err(format!(
            "{} right edge is not at window width {w}, tolerance {tol}",
            describe(name, a)
        ))
    }
}

/// Passes when the box's bottom edge sits on the window's bottom edge within `tol`.
pub(crate) fn bottom_edge_at_window(
    name: &str,
    a: Bounds<Pixels>,
    window: Size<Pixels>,
    tol: f32,
) -> Result<(), String> {
    let h = f32::from(window.height);
    if (Edges::of(a).bottom - h).abs() <= tol {
        Ok(())
    } else {
        Err(format!(
            "{} bottom edge is not at window height {h}, tolerance {tol}",
            describe(name, a)
        ))
    }
}

/// Passes when the box's left edge sits on the window's left edge within `tol`.
pub(crate) fn left_edge_at_window(
    name: &str,
    a: Bounds<Pixels>,
    window: Size<Pixels>,
    tol: f32,
) -> Result<(), String> {
    if Edges::of(a).left.abs() <= tol {
        Ok(())
    } else {
        Err(format!(
            "{} left edge is not at the left edge of window {}x{}, tolerance {tol}",
            describe(name, a),
            f32::from(window.width),
            f32::from(window.height)
        ))
    }
}

/// Passes when the box's top edge sits at `y` within `tol`.
pub(crate) fn top_edge_at(name: &str, a: Bounds<Pixels>, y: f32, tol: f32) -> Result<(), String> {
    if (Edges::of(a).top - y).abs() <= tol {
        Ok(())
    } else {
        Err(format!(
            "{} top edge is not at y {y}, tolerance {tol}",
            describe(name, a)
        ))
    }
}

/// Passes when the box width is within `min..=max`.
pub(crate) fn width_between(
    name: &str,
    a: Bounds<Pixels>,
    min: f32,
    max: f32,
) -> Result<(), String> {
    let w = Edges::of(a).width();
    if (min..=max).contains(&w) {
        Ok(())
    } else {
        Err(format!(
            "{} width {w} is outside {min}..={max}",
            describe(name, a)
        ))
    }
}

/// Passes when the box height is within `min..=max`.
pub(crate) fn height_between(
    name: &str,
    a: Bounds<Pixels>,
    min: f32,
    max: f32,
) -> Result<(), String> {
    let h = Edges::of(a).height();
    if (min..=max).contains(&h) {
        Ok(())
    } else {
        Err(format!(
            "{} height {h} is outside {min}..={max}",
            describe(name, a)
        ))
    }
}

/// Passes when the box's horizontal centre is at the window's centre within `tol`.
pub(crate) fn centered_x(
    name: &str,
    a: Bounds<Pixels>,
    window: Size<Pixels>,
    tol: f32,
) -> Result<(), String> {
    let e = Edges::of(a);
    let w = f32::from(window.width);
    if ((e.left + e.right) / 2.0 - w / 2.0).abs() <= tol {
        Ok(())
    } else {
        Err(format!(
            "{} is not horizontally centred in window width {w}, tolerance {tol}",
            describe(name, a)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, size};

    fn b(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn win(w: f32, h: f32) -> Size<Pixels> {
        size(px(w), px(h))
    }

    fn names_each(err: &str, names: &[&str]) {
        for n in names {
            assert!(err.contains(&format!("{n} (")), "{err} does not name {n}");
        }
    }

    #[test]
    fn nonzero_rejects_zero_width() {
        assert!(nonzero("a", b(0.0, 0.0, 10.0, 10.0)).is_ok());
        let err = nonzero("a", b(0.0, 0.0, 0.0, 10.0)).unwrap_err();
        names_each(&err, &["a"]);
    }

    #[test]
    fn inside_exact() {
        assert!(
            inside(
                "in",
                b(2.0, 2.0, 6.0, 6.0),
                "out",
                b(0.0, 0.0, 10.0, 10.0),
                0.0
            )
            .is_ok()
        );
        let err = inside(
            "in",
            b(8.0, 2.0, 6.0, 6.0),
            "out",
            b(0.0, 0.0, 10.0, 10.0),
            0.0,
        )
        .unwrap_err();
        names_each(&err, &["in", "out"]);
    }

    #[test]
    fn inside_with_tolerance() {
        assert!(
            inside(
                "in",
                b(1.0, 0.0, 10.0, 10.0),
                "out",
                b(0.0, 0.0, 10.0, 10.0),
                TOL
            )
            .is_ok()
        );
        let err = inside(
            "in",
            b(11.0, 0.0, 10.0, 10.0),
            "out",
            b(0.0, 0.0, 10.0, 10.0),
            TOL,
        )
        .unwrap_err();
        names_each(&err, &["in", "out"]);
    }

    #[test]
    fn in_window_bounds() {
        assert!(in_window("w", b(0.0, 0.0, 100.0, 50.0), win(100.0, 50.0), 0.0).is_ok());
        let err = in_window("w", b(90.0, 0.0, 20.0, 10.0), win(100.0, 50.0), 0.0).unwrap_err();
        names_each(&err, &["w"]);
    }

    #[test]
    fn no_overlap_allows_touching_edges() {
        assert!(no_overlap("a", b(0.0, 0.0, 10.0, 10.0), "b", b(10.0, 0.0, 10.0, 10.0)).is_ok());
        let err =
            no_overlap("a", b(0.0, 0.0, 10.0, 10.0), "b", b(9.0, 0.0, 10.0, 10.0)).unwrap_err();
        names_each(&err, &["a", "b"]);
    }

    #[test]
    fn above_exact() {
        assert!(
            above(
                "a",
                b(0.0, 0.0, 10.0, 10.0),
                "b",
                b(0.0, 10.0, 10.0, 10.0),
                0.0
            )
            .is_ok()
        );
        let err = above(
            "a",
            b(0.0, 0.0, 10.0, 10.0),
            "b",
            b(0.0, 9.0, 10.0, 10.0),
            0.0,
        )
        .unwrap_err();
        names_each(&err, &["a", "b"]);
    }

    #[test]
    fn left_aligned_with_tolerance() {
        assert!(
            left_aligned(
                "a",
                b(10.0, 0.0, 5.0, 5.0),
                "b",
                b(12.0, 0.0, 5.0, 5.0),
                TOL
            )
            .is_ok()
        );
        let err = left_aligned(
            "a",
            b(10.0, 0.0, 5.0, 5.0),
            "b",
            b(13.0, 0.0, 5.0, 5.0),
            TOL,
        )
        .unwrap_err();
        names_each(&err, &["a", "b"]);
    }

    #[test]
    fn right_aligned_with_tolerance() {
        assert!(
            right_aligned(
                "a",
                b(0.0, 0.0, 10.0, 5.0),
                "b",
                b(0.0, 0.0, 12.0, 5.0),
                TOL
            )
            .is_ok()
        );
        let err = right_aligned(
            "a",
            b(0.0, 0.0, 10.0, 5.0),
            "b",
            b(0.0, 0.0, 13.0, 5.0),
            TOL,
        )
        .unwrap_err();
        names_each(&err, &["a", "b"]);
    }

    #[test]
    fn right_edge_at_window_exact() {
        assert!(right_edge_at_window("a", b(90.0, 0.0, 10.0, 5.0), win(100.0, 50.0), 0.0).is_ok());
        let err =
            right_edge_at_window("a", b(80.0, 0.0, 10.0, 5.0), win(100.0, 50.0), 0.0).unwrap_err();
        names_each(&err, &["a"]);
    }

    #[test]
    fn bottom_edge_at_window_exact() {
        assert!(
            bottom_edge_at_window("a", b(0.0, 40.0, 10.0, 10.0), win(100.0, 50.0), 0.0).is_ok()
        );
        let err = bottom_edge_at_window("a", b(0.0, 30.0, 10.0, 10.0), win(100.0, 50.0), 0.0)
            .unwrap_err();
        names_each(&err, &["a"]);
    }

    #[test]
    fn left_and_top_edges() {
        assert!(left_edge_at_window("a", b(1.0, 0.0, 10.0, 10.0), win(100.0, 50.0), TOL).is_ok());
        assert!(left_edge_at_window("a", b(3.0, 0.0, 10.0, 10.0), win(100.0, 50.0), TOL).is_err());
        assert!(top_edge_at("a", b(0.0, 20.0, 10.0, 10.0), 20.0, 0.0).is_ok());
        let err = top_edge_at("a", b(0.0, 21.0, 10.0, 10.0), 20.0, 0.0).unwrap_err();
        names_each(&err, &["a"]);
    }

    #[test]
    fn width_between_is_inclusive() {
        assert!(width_between("a", b(0.0, 0.0, 300.0, 10.0), 300.0, 420.0).is_ok());
        assert!(width_between("a", b(0.0, 0.0, 420.0, 10.0), 300.0, 420.0).is_ok());
        let over = width_between("a", b(0.0, 0.0, 421.0, 10.0), 300.0, 420.0).unwrap_err();
        names_each(&over, &["a"]);
        assert!(width_between("a", b(0.0, 0.0, 299.0, 10.0), 300.0, 420.0).is_err());
    }

    #[test]
    fn height_between_is_inclusive() {
        assert!(height_between("a", b(0.0, 0.0, 10.0, 24.0), 24.0, 32.0).is_ok());
        assert!(height_between("a", b(0.0, 0.0, 10.0, 33.0), 24.0, 32.0).is_err());
    }

    #[test]
    fn centered_x_with_tolerance() {
        assert!(centered_x("a", b(450.0, 0.0, 100.0, 10.0), win(1000.0, 50.0), TOL).is_ok());
        let err = centered_x("a", b(400.0, 0.0, 100.0, 10.0), win(1000.0, 50.0), TOL).unwrap_err();
        names_each(&err, &["a"]);
    }

    #[test]
    fn inside_message_names_boxes_and_numbers() {
        let err = inside(
            "search.hint",
            b(10.0, 4.0, 30.0, 16.0),
            "search.field",
            b(0.0, 0.0, 20.0, 26.0),
            TOL,
        )
        .unwrap_err();
        assert_eq!(
            err,
            "search.hint (10,4 30x16) is not inside search.field (0,0 20x26), tolerance 2"
        );
    }
}
