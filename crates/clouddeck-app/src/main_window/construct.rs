use super::*;

impl MainWindow {
    /// The toolbar search field, for focusing it in a named render state.
    pub(crate) fn search_field(&self) -> Entity<SearchField> {
        self.search.clone()
    }

    /// The overlay drawn by the last render, if any.
    #[cfg(test)]
    pub(crate) fn rendered_overlay(&self) -> Option<&'static str> {
        self.rendered_overlay
    }

    /// Whether the sidebar and inspector were drawn by the last render.
    #[cfg(test)]
    pub(crate) fn rendered_panes(&self) -> (bool, bool) {
        (self.rendered.sidebar, self.rendered.inspector)
    }

    pub fn new(
        fleet: Entity<FleetState>,
        available_regions: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        (!cx.has_global::<MenusInstalled>()).then(|| install_menus(cx));
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window);
        let viewport = window.viewport_size();
        let rules = layout_rules(f32::from(viewport.width), f32::from(viewport.height));
        fleet.update(cx, |fleet, _| {
            fleet.sidebar_open &= rules.panes_open_by_default;
            fleet.inspector_open &= rules.panes_open_by_default;
        });
        let search = cx.new(|cx| SearchField::new(fleet.clone(), window, cx));
        let toolbar = cx.new(|cx| Toolbar::new(fleet.clone(), search.clone(), cx));
        let sidebar = cx.new(|cx| Sidebar::new(fleet.clone(), cx));
        let table = cx.new(|cx| InstanceTable::new(fleet.clone(), cx));
        let inspector = cx.new(|cx| InspectorPane::new(fleet.clone(), cx));
        let context_bar = cx.new(|cx| ContextBar::new(fleet.clone(), cx));
        let center = cx.new(|_| {
            CenterStateView::new(
                CenterState::Loading,
                CenterText {
                    title: String::new(),
                    body: String::new(),
                    action: None,
                    command: None,
                },
            )
        });

        cx.subscribe(&center, |this, _, event: &CenterEvent, cx| match event {
            CenterEvent::Action(
                CenterState::SignedOut | CenterState::Error { .. } | CenterState::Throttled,
            ) => this.refresh(cx),
            CenterEvent::Action(CenterState::NoMatch) => {
                this.fleet.update(cx, |fleet, cx| fleet.set_query("", cx))
            }
            CenterEvent::Action(CenterState::EmptyFleet) => this.select_all_regions(cx),
            CenterEvent::Action(CenterState::Loading | CenterState::Offline) => {}
        })
        .detach();
        cx.observe(&fleet, |_, _, cx| cx.notify()).detach();
        cx.subscribe(&toolbar, |this, _, event: &ToolbarEvent, cx| match event {
            ToolbarEvent::ToggleSidebar => this.toggle_sidebar(cx),
            ToolbarEvent::FiltersClicked => {}
        })
        .detach();
        cx.subscribe_in(
            &context_bar,
            window,
            |this, _, event: &ContextBarEvent, window, cx| match event {
                ContextBarEvent::ToggleSidebar => this.toggle_sidebar(cx),
                ContextBarEvent::ToggleInspector => this.toggle_inspector(cx),
                ContextBarEvent::Refresh => this.refresh(cx),
                ContextBarEvent::OpenProfilePicker => {
                    this.toggle_overlay(Overlay::Profile, window, cx)
                }
                ContextBarEvent::OpenProviderPicker => {
                    this.toggle_overlay(Overlay::Provider, window, cx)
                }
                ContextBarEvent::OpenRegionPicker => {
                    this.toggle_overlay(Overlay::Region, window, cx)
                }
                ContextBarEvent::OpenFailureDetails => {
                    this.toggle_overlay(Overlay::Failure, window, cx)
                }
                ContextBarEvent::SignIn => {
                    let provider = this.context_model.active_provider().to_string();
                    cx.emit(MainWindowEvent::SignIn(provider));
                }
            },
        )
        .detach();
        cx.subscribe_in(
            &table,
            window,
            |this, _, event: &InstanceTableEvent, window, cx| {
                let InstanceTableEvent::RowContextMenu { key, position } = event;
                this.open_row_menu(key.clone(), *position, window, cx);
            },
        )
        .detach();
        cx.observe_window_activation(window, |this, window, cx| {
            this.window_active = window.is_window_active();
            if this.window_active {
                this.refresh_on_focus_if_due(cx);
            }
        })
        .detach();

        let settings = Settings::load(&settings_path());
        apply_appearance(settings.theme, window, cx);
        let appearance_observer = cx.observe_window_appearance(window, |this, window, cx| {
            apply_appearance(this.theme, window, cx);
        });
        let auto_refresh_loop = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(AUTO_REFRESH_TICK).await;
                if this.update(cx, |this, cx| this.refresh_if_due(cx)).is_err() {
                    break;
                }
            }
        });

        let mut context_model = ContextModel::new(Vec::new(), None, "");
        context_model.set_dimensions(vec![region_dimension(&available_regions)]);
        Self {
            fleet,
            available_regions,
            failures: Vec::new(),
            profiles: Vec::new(),
            provider_short_name: "AWS".to_string(),
            connection_label: "Profile".to_string(),
            sign_in: false,
            provider_menu: Vec::new(),
            context_model,
            overlay: None,
            toolbar,
            sidebar,
            table,
            inspector,
            context_bar,
            center,
            search,
            focus_handle,
            overlay_view: None,
            overlay_subscription: None,
            region_dirty: false,
            last_narrow: Some(rules.narrow),
            last_load_error: None,
            auto_refresh: settings.auto_refresh,
            refresh_interval_minutes: settings.refresh_interval_minutes,
            refresh_on_focus: settings.refresh_on_focus,
            window_active: window.is_window_active(),
            theme: settings.theme,
            _appearance_observer: appearance_observer,
            _auto_refresh_loop: auto_refresh_loop,
            #[cfg(test)]
            rendered: RenderedPanes::default(),
            #[cfg(test)]
            rendered_overlay: None,
        }
    }

    pub fn apply_settings(
        &mut self,
        settings: &Settings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.auto_refresh = settings.auto_refresh;
        self.refresh_interval_minutes = settings.refresh_interval_minutes;
        self.refresh_on_focus = settings.refresh_on_focus;
        self.theme = settings.theme;
        apply_appearance(self.theme, window, cx);
        cx.notify();
    }
}
