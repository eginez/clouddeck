use super::*;

impl MainWindow {
    pub(super) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.fleet.update(cx, |fleet, cx| {
            fleet.sidebar_open = !fleet.sidebar_open;
            cx.notify();
        });
    }

    pub(super) fn toggle_inspector(&mut self, cx: &mut Context<Self>) {
        self.fleet.update(cx, |fleet, cx| {
            fleet.inspector_open = !fleet.inspector_open;
            cx.notify();
        });
    }

    pub(super) fn close_panes(&mut self, cx: &mut Context<Self>) {
        self.fleet.update(cx, |fleet, cx| {
            fleet.sidebar_open = false;
            fleet.inspector_open = false;
            cx.notify();
        });
    }

    pub(super) fn sidebar_pane(&self, overlay: bool) -> AnyElement {
        let pane = div()
            .debug_selector(|| "main.sidebar".into())
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .child(self.sidebar.clone());
        if overlay {
            pane.absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .into_any_element()
        } else {
            pane.into_any_element()
        }
    }

    pub(super) fn inspector_pane(&self, overlay: bool) -> AnyElement {
        let pane = div()
            .debug_selector(|| "main.inspector".into())
            .w(px(INSPECTOR_WIDTH))
            .h_full()
            .child(self.inspector.clone());
        if overlay {
            pane.absolute()
                .top_0()
                .bottom_0()
                .right_0()
                .max_w(relative(OVERLAY_MAX_FRACTION))
                .into_any_element()
        } else {
            pane.into_any_element()
        }
    }
}
