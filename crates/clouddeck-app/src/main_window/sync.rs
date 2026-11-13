use super::*;

impl MainWindow {
    pub(super) fn sync_context_bar(&mut self, cx: &mut Context<Self>) {
        let region_label = self.context_model.summary(REGION);
        let failed_regions: Vec<String> = self
            .failures
            .iter()
            .map(|failure| failure.region.clone())
            .collect();
        let fleet = self.fleet.read(cx);
        let last_success_text =
            updated_text(fleet.view_model.last_success(), local_offset_minutes());
        let shown_error = match fleet.view_model.error() {
            Some(error) => Some(error.clone()),
            None if fleet.view_model.is_loading() => self.last_load_error.clone(),
            None => None,
        };
        let load_error_text = match &shown_error {
            Some(error) if !fleet.view_model.instances().is_empty() => Some(format!(
                "Refresh failed: {}",
                region_failure_reason(error, fleet.retry_in_seconds)
            )),
            _ => None,
        };
        let load_failed = shown_error.is_some()
            && fleet.view_model.instances().is_empty()
            && !fleet.view_model.is_loading();
        self.last_load_error = shown_error;
        let provider_short_name = self.provider_short_name.clone();
        let connection_label = self.connection_label.clone();
        let sign_in = self.sign_in;
        self.context_bar.update(cx, |bar, cx| {
            if bar.region_label == region_label
                && bar.failed_regions == failed_regions
                && bar.last_success_text == last_success_text
                && bar.load_error_text == load_error_text
                && bar.load_failed == load_failed
                && bar.provider_short_name == provider_short_name
                && bar.connection_label == connection_label
                && bar.sign_in == sign_in
            {
                return;
            }
            bar.region_label = region_label;
            bar.failed_regions = failed_regions;
            bar.last_success_text = last_success_text;
            bar.load_error_text = load_error_text;
            bar.load_failed = load_failed;
            bar.provider_short_name = provider_short_name;
            bar.connection_label = connection_label;
            bar.sign_in = sign_in;
            cx.notify();
        });
    }

    pub(super) fn center_state_for(&self, cx: &App) -> Option<(CenterState, CenterText)> {
        let fleet = self.fleet.read(cx);
        let input = CenterInput {
            is_loading: fleet.view_model.is_loading(),
            loaded_count: fleet.view_model.instances().len(),
            shown_count: fleet.visible_rows().len(),
            error: fleet.view_model.error().cloned(),
            failure: fleet.view_model.load_failure().cloned(),
        };
        let state = center_state(&input)?;
        let context = CenterContext {
            profile: fleet.profile.clone(),
            scope_label: self.context_model.summary(REGION),
            scope_is_all: self.context_model.is_all(REGION),
            query: fleet.query().to_string(),
            last_good: None,
            retry_in_seconds: fleet.retry_in_seconds,
        };
        let text = center_text(&state, &context);
        Some((state, text))
    }
}
