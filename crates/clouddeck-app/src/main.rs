use gpui::{
    App, Application, Bounds, Context, IntoElement, Render, SharedString, Window, WindowBounds,
    WindowOptions, div, prelude::*, px, size,
};

struct RootView {
    text: SharedString,
}

impl Render for RootView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // `.debug_selector(...)` tags an element with a lookup key that
        // `VisualTestContext::debug_bounds` can query in headless tests. It
        // compiles to a no-op outside test/test-support builds, so it's safe
        // to leave on production render code. See
        // `brain/playbooks/gpui-view-testing.md` for the pattern this
        // demonstrates.
        div().debug_selector(|| "root-view".into()).child(
            div()
                .debug_selector(|| "status-text".into())
                .child(format!("CloudDeck: {}", &self.text)),
        )
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| RootView {
                    text: "loading...".into(),
                })
            },
        )
        .unwrap();
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn opens_root_window(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|_, _| RootView {
            text: "ready".into(),
        });
        cx.update(|_, cx| {
            assert_eq!(view.read(cx).text.to_string(), "ready");
        });
    }

    /// Worked example of the structural-assertion pattern described in
    /// `brain/playbooks/gpui-view-testing.md`: elements tagged with
    /// `.debug_selector(...)` show up (with real layout bounds) in
    /// `VisualTestContext::debug_bounds` once the window has drawn, and
    /// elements that were never rendered don't. This lets a headless test
    /// assert on rendered *structure* (which children exist, and where)
    /// rather than only on the view's own struct fields.
    #[gpui::test]
    fn renders_expected_child_structure(cx: &mut TestAppContext) {
        let (_view, cx) = cx.add_window_view(|_, _| RootView {
            text: "ready".into(),
        });

        assert!(
            cx.debug_bounds("root-view").is_some(),
            "root-view should be present in the rendered frame"
        );
        assert!(
            cx.debug_bounds("status-text").is_some(),
            "status-text child should be present in the rendered frame"
        );
        assert!(
            cx.debug_bounds("no-such-element").is_none(),
            "an element that was never rendered should not show up"
        );
    }
}
