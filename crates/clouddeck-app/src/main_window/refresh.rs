use super::*;

impl MainWindow {
    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.fleet.update(cx, |fleet, cx| fleet.refresh(cx));
    }

    pub(super) fn refresh_if_due(&mut self, cx: &mut Context<Self>) {
        let fleet = self.fleet.read(cx);
        if fleet.retry_in_seconds.is_some() {
            return;
        }
        let now = fleet.now(cx);
        let due = should_auto_refresh(
            fleet.view_model.last_success(),
            now,
            self.auto_refresh,
            self.refresh_interval_minutes,
            self.window_active,
            fleet.view_model.is_loading(),
        );
        if due {
            self.refresh(cx);
        }
    }

    pub(super) fn refresh_on_focus_if_due(&mut self, cx: &mut Context<Self>) {
        let fleet = self.fleet.read(cx);
        let now = fleet.now(cx);
        let due = should_refresh_on_focus(
            fleet.view_model.last_success(),
            now,
            self.refresh_on_focus,
            self.refresh_interval_minutes,
            fleet.view_model.is_loading(),
        );
        if due {
            self.refresh(cx);
        }
    }
}
