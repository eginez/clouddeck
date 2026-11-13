use super::*;

impl MainWindow {
    pub(crate) fn toggle_overlay(
        &mut self,
        overlay: Overlay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.overlay.as_ref() == Some(&overlay) {
            self.close_overlay(window, cx);
        } else {
            self.open_overlay(overlay, window, cx);
        }
    }

    pub(crate) fn open_overlay(
        &mut self,
        overlay: Overlay,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(overlay, Overlay::Profile | Overlay::Region)
            && self.fleet.read(cx).view_model.is_all()
        {
            return;
        }
        let (view, subscription): (AnyView, Subscription) = match &overlay {
            Overlay::Profile => {
                let profiles = self.profiles.clone();
                let current = self.fleet.read(cx).profile.clone();
                let picker = cx.new(|cx| ProfilePicker::new(profiles, current, window, cx));
                let subscription = cx.subscribe_in(
                    &picker,
                    window,
                    |this, _, event: &ProfilePickerEvent, window, cx| match event {
                        ProfilePickerEvent::Picked(name) => {
                            this.close_overlay(window, cx);
                            cx.emit(MainWindowEvent::PickProfile(name.clone()));
                        }
                        ProfilePickerEvent::ConnectToAws => {
                            this.close_overlay(window, cx);
                            cx.emit(MainWindowEvent::SignIn("aws".to_string()));
                        }
                        ProfilePickerEvent::Dismissed => this.close_overlay(window, cx),
                    },
                );
                (picker.into(), subscription)
            }
            Overlay::Provider => {
                let rows = self.provider_menu.clone();
                let picker = cx.new(|_| ProviderPicker::new(rows));
                let subscription = cx.subscribe_in(
                    &picker,
                    window,
                    |this, _, event: &ProviderPickerEvent, window, cx| match event {
                        ProviderPickerEvent::Picked(id) => {
                            this.close_overlay(window, cx);
                            cx.emit(MainWindowEvent::PickProvider(id.clone()));
                        }
                    },
                );
                (picker.into(), subscription)
            }
            Overlay::Region => {
                let model = self.region_picker_model(cx);
                let picker = cx.new(|cx| DimensionPicker::new(model, REGION, window, cx));
                let subscription = cx.subscribe_in(
                    &picker,
                    window,
                    |this, _, event: &DimensionPickerEvent, window, cx| match event {
                        DimensionPickerEvent::Changed(values) => {
                            this.context_model.set_values(REGION, values.clone());
                            this.region_dirty = true;
                            cx.notify();
                        }
                        DimensionPickerEvent::Dismissed => this.close_overlay(window, cx),
                    },
                );
                (picker.into(), subscription)
            }
            Overlay::Palette => {
                let context = self.command_context(cx);
                let palette = cx.new(|cx| CommandPalette::new(context, window, cx));
                let subscription = cx.subscribe_in(
                    &palette,
                    window,
                    |this, _, event: &PaletteEvent, window, cx| match event {
                        PaletteEvent::Run(id) => {
                            this.close_overlay(window, cx);
                            this.run_command(*id, window, cx);
                        }
                        PaletteEvent::Dismissed => this.close_overlay(window, cx),
                    },
                );
                (palette.into(), subscription)
            }
            Overlay::RowMenu { .. } => {
                let context = self.command_context(cx);
                let menu = cx.new(|_| RowContextMenu::new(context));
                let subscription = cx.subscribe_in(
                    &menu,
                    window,
                    |this, _, event: &RowMenuEvent, window, cx| match event {
                        RowMenuEvent::Run(id) => {
                            this.close_overlay(window, cx);
                            this.run_command(*id, window, cx);
                        }
                        RowMenuEvent::Dismissed => this.close_overlay(window, cx),
                    },
                );
                (menu.into(), subscription)
            }
            Overlay::Failure => {
                let (failures, refreshed_at, other_regions) = self.failure_popover_data(cx);
                let popover =
                    cx.new(|_| FailurePopover::new(failures, refreshed_at, other_regions));
                let subscription = cx.subscribe_in(
                    &popover,
                    window,
                    |this, _, event: &FailurePopoverEvent, window, cx| match event {
                        FailurePopoverEvent::Retry(region) => {
                            cx.emit(MainWindowEvent::Retry(Some(region.clone())));
                        }
                        FailurePopoverEvent::RetryAll => {
                            cx.emit(MainWindowEvent::Retry(None));
                        }
                        FailurePopoverEvent::Copied => {}
                        FailurePopoverEvent::Dismissed => this.close_overlay(window, cx),
                    },
                );
                (popover.into(), subscription)
            }
        };
        self.overlay = Some(overlay);
        self.overlay_view = Some(view);
        self.overlay_subscription = Some(subscription);
        cx.notify();
    }

    pub(super) fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_region_scope(cx);
        self.overlay = None;
        self.overlay_view = None;
        self.overlay_subscription = None;
        self.focus_handle.focus(window);
        cx.notify();
    }

    /// Announces a region selection once the picker that made it is no longer open,
    /// so the shell rebuilds the fleet without tearing down the picker mid-interaction.
    pub(super) fn commit_region_scope(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.region_dirty) {
            cx.emit(MainWindowEvent::RegionsChanged(region_values(
                &self.context_model,
            )));
        }
    }

    pub(super) fn select_all_regions(&mut self, cx: &mut Context<Self>) {
        self.context_model.select_all(REGION);
        self.region_dirty = false;
        cx.emit(MainWindowEvent::RegionsChanged(region_values(
            &self.context_model,
        )));
        cx.notify();
    }

    pub(crate) fn open_row_menu(
        &mut self,
        key: String,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.fleet
            .update(cx, |fleet, cx| fleet.select(Some(key.clone()), cx));
        self.open_overlay(Overlay::RowMenu { key, position }, window, cx);
    }
}
