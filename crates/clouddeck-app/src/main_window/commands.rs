use super::*;

impl MainWindow {
    pub(super) fn command_context(&self, cx: &App) -> CommandContext {
        let fleet = self.fleet.read(cx);
        let selected = fleet.selected_instance();
        CommandContext {
            has_selection: selected.is_some(),
            selection_has_private_ip: selected
                .as_ref()
                .is_some_and(|instance| instance.address(AddressKind::Private).is_some()),
            selection_has_public_ip: selected
                .as_ref()
                .is_some_and(|instance| instance.address(AddressKind::Public).is_some()),
            query_is_empty: fleet.query().is_empty(),
            has_failed_region: !self.failures.is_empty(),
        }
    }

    pub(super) fn region_picker_model(&self, cx: &App) -> ContextModel {
        let has_regions = self.context_model.dimensions().iter().any(|dim| {
            dim.id.as_str() == REGION
                && dim
                    .values
                    .iter()
                    .map(|v| v.id.as_str())
                    .eq(self.available_regions.iter().map(String::as_str))
        });
        let mut model = if has_regions {
            self.context_model.clone()
        } else {
            region_model(
                &self.available_regions,
                &self.fleet.read(cx).view_model.instances(),
            )
        };
        set_region_values(&mut model, &region_values(&self.context_model));
        model
    }

    pub(super) fn region_failures(&self, cx: &App) -> Vec<RegionFailure> {
        let instances = self.fleet.read(cx).view_model.instances();
        self.failures
            .iter()
            .map(|failure| RegionFailure {
                region: failure.region.clone(),
                message: failure.message.clone(),
                error: failure.error.clone(),
                stale_instances: stale_count(&instances, &failure.region),
                retry_in_seconds: None,
            })
            .collect()
    }

    pub(super) fn failure_popover_data(&self, cx: &App) -> (Vec<RegionFailure>, String, usize) {
        let failures = self.region_failures(cx);
        let refreshed_at = self
            .fleet
            .read(cx)
            .view_model
            .last_success()
            .map(|at| clock_text(at, local_offset_minutes()))
            .unwrap_or_default();
        let other_regions = self.available_regions.len().saturating_sub(failures.len());
        (failures, refreshed_at, other_regions)
    }

    pub(super) fn sync_failure_popover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay != Some(Overlay::Failure) {
            return;
        }
        let Some(popover) = self
            .overlay_view
            .clone()
            .and_then(|view| view.downcast::<FailurePopover>().ok())
        else {
            return;
        };
        let (failures, refreshed_at, other_regions) = self.failure_popover_data(cx);
        if failures.is_empty() {
            self.close_overlay(window, cx);
            return;
        }
        popover.update(cx, |popover, cx| {
            popover.failures = failures;
            popover.refreshed_at = refreshed_at;
            popover.other_regions = other_regions;
            cx.notify();
        });
    }

    pub(super) fn run_command(
        &mut self,
        id: CommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match id {
            CommandId::SwitchProfile => self.open_overlay(Overlay::Profile, window, cx),
            CommandId::SwitchRegion => self.open_overlay(Overlay::Region, window, cx),
            CommandId::Refresh => self.refresh(cx),
            CommandId::ToggleSidebar => self.toggle_sidebar(cx),
            CommandId::ToggleInspector => self.toggle_inspector(cx),
            CommandId::FilterRunning
            | CommandId::FilterStopped
            | CommandId::FilterPending
            | CommandId::ClearFilters => {
                let fleet = self.fleet.read(cx);
                let known = known_tag_keys(&fleet.view_model.instances());
                if let Some(query) = filter_command_query(id, fleet.query(), &known) {
                    self.fleet
                        .update(cx, |fleet, cx| fleet.set_query(&query, cx));
                }
            }
            CommandId::CopyId => {
                self.copy_selected(|instance| Some(native_id(instance).to_string()), cx)
            }
            CommandId::CopyPrivateIp => self.copy_selected(
                |instance| instance.address(AddressKind::Private).map(str::to_string),
                cx,
            ),
            CommandId::CopyPublicIp => self.copy_selected(
                |instance| instance.address(AddressKind::Public).map(str::to_string),
                cx,
            ),
            CommandId::OpenInConsole => self.open_selected_in_console(cx),
            CommandId::RetryFailedRegion => cx.emit(MainWindowEvent::Retry(None)),
            CommandId::ConnectToAws => cx.emit(MainWindowEvent::SignIn("aws".to_string())),
            CommandId::OpenSettings => cx.emit(MainWindowEvent::OpenSettings),
            CommandId::About => {
                crate::about_window::open(cx);
            }
            CommandId::Plugins => {
                crate::plugins_window::open(cx);
            }
        }
    }

    pub(super) fn copy_selected(
        &self,
        value: fn(&Instance) -> Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(instance) = self.fleet.read(cx).selected_instance() else {
            return;
        };
        let Some(text) = value(&instance) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        let flash = copied_flash(&text);
        self.fleet
            .update(cx, |fleet, cx| fleet.set_flash(flash, cx));
    }

    pub(super) fn open_selected_in_console(&self, cx: &mut Context<Self>) {
        let Some(instance) = self.fleet.read(cx).selected_instance() else {
            return;
        };
        let flash = match console_url_for(&instance) {
            Ok(url) => {
                cx.open_url(&url);
                OPEN_IN_CONSOLE_FLASH.to_string()
            }
            Err(error) => error.to_string(),
        };
        self.fleet
            .update(cx, |fleet, cx| fleet.set_flash(flash, cx));
    }

    pub(super) fn escape_pressed(
        &mut self,
        _: &EscapePressed,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let search_focused = self
            .search
            .read(cx)
            .input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window);
        let fleet = self.fleet.read(cx);
        let ctx = EscapeContext {
            overlay_open: self.overlay.is_some(),
            search_focused,
            query_empty: fleet.query().is_empty(),
            narrow: self.last_narrow == Some(true),
            sidebar_open: fleet.sidebar_open,
            has_selection: fleet.selected.is_some(),
        };
        match escape_action(&ctx) {
            EscapeAction::CloseOverlay => self.close_overlay(window, cx),
            EscapeAction::ClearSearch => self.fleet.update(cx, |fleet, cx| fleet.set_query("", cx)),
            EscapeAction::CloseSidebarOverlay => self.fleet.update(cx, |fleet, cx| {
                fleet.sidebar_open = false;
                cx.notify();
            }),
            EscapeAction::ClearSelection => {
                self.fleet.update(cx, |fleet, cx| fleet.select(None, cx))
            }
            EscapeAction::Nothing => {}
        }
    }
}
