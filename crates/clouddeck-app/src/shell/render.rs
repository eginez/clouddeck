use super::*;

impl AppShell {
    pub(super) fn list_view(
        &self,
        providers: &[std::sync::Arc<dyn Provider>],
        fleet_regions: &[String],
    ) -> ListView {
        if self.context_model.shows_all_providers() {
            all_providers_view(
                providers,
                self.demo.then_some(self.demo_scenario),
                self.request_timeout,
                &self.context,
            )
        } else {
            self.active_provider_view(providers, fleet_regions)
        }
    }

    pub(super) fn active_provider_view(
        &self,
        providers: &[std::sync::Arc<dyn Provider>],
        fleet_regions: &[String],
    ) -> ListView {
        #[cfg(test)]
        let provider = self.test_service_override.clone().unwrap_or_else(|| {
            Self::provider_for_id(
                self.context_model.active_provider(),
                providers,
                &self.context,
                self.demo,
                self.demo_scenario,
                fleet_regions,
                self.request_timeout,
            )
        });
        #[cfg(not(test))]
        let provider = Self::provider_for_id(
            self.context_model.active_provider(),
            providers,
            &self.context,
            self.demo,
            self.demo_scenario,
            fleet_regions,
            self.request_timeout,
        );
        InstanceListViewModel::new(provider).into()
    }

    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.fleet.update(cx, |fleet, cx| fleet.refresh(cx));
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .debug_selector(|| "app-shell".into())
            .flex()
            .flex_col()
            .size_full()
            .bg(SurgicalTheme::workspace_canvas())
            .children(self.message.clone().map(|m| {
                div()
                    .px(px(16.0))
                    .text_color(SurgicalTheme::warning())
                    .child(m)
            }))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .min_w(px(0.0))
                    .w_full()
                    .child(self.main_window.clone()),
            )
            .on_action(cx.listener(|this, _: &Refresh, _, cx| this.refresh(cx)))
    }
}
