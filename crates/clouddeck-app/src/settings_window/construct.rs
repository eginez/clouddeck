use super::*;

impl SettingsWindow {
    pub fn new(
        settings: Settings,
        providers: Arc<ProviderRegistry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Search settings")
                .clean_on_escape()
        });
        let mut selects = Vec::new();
        let mut subscriptions = vec![cx.observe(&search, |_, _, cx| cx.notify())];
        subscriptions.push(cx.on_app_quit(|this: &mut Self, _| {
            this.flush_pending_save();
            std::future::ready(())
        }));
        for id in SettingId::ALL {
            let items = choices(id);
            if items.is_empty() {
                continue;
            }
            let selected = current(id, &settings)
                .and_then(|pick| items.iter().position(|choice| choice.pick == pick))
                .map(IndexPath::new);
            let state = cx.new(|cx| SelectState::new(items, selected, window, cx));
            subscriptions.push(cx.subscribe_in(
                &state,
                window,
                move |this: &mut Self, _, event: &SelectEvent<Vec<Choice>>, _, cx| {
                    if let SelectEvent::Confirm(Some(pick)) = event {
                        this.choose(id, pick.clone(), cx);
                    }
                },
            ));
            selects.push((id, state));
        }
        let provider_categories = providers.settings_categories();
        let mut provider_inputs = Vec::new();
        let mut provider_selects = Vec::new();
        for category in &provider_categories {
            for def in &category.settings {
                let key = field_key(&category.extension, &def.key);
                match &def.kind {
                    SettingKind::Choice(options) => {
                        let selected = display_text(providers.setting_value(&key.0, &key.1));
                        let state = choice_state(options, &selected, window, cx);
                        let extension = category.extension.clone();
                        let def = def.clone();
                        subscriptions.push(cx.subscribe_in(
                            &state,
                            window,
                            move |this: &mut Self,
                                  _,
                                  event: &SelectEvent<Vec<ProviderChoice>>,
                                  _,
                                  cx| {
                                if let SelectEvent::Confirm(Some(value)) = event {
                                    this.edit_provider_choice(&extension, &def, value, cx);
                                }
                            },
                        ));
                        provider_selects.push((key, state));
                    }
                    SettingKind::Bool => {}
                    _ => {
                        let text = display_text(providers.setting_value(&key.0, &key.1));
                        let state = text_state(text, def.secret, window, cx);
                        let extension = category.extension.clone();
                        let def = def.clone();
                        subscriptions.push(cx.subscribe_in(
                            &state,
                            window,
                            move |this: &mut Self, state, event: &InputEvent, _, cx| {
                                if matches!(event, InputEvent::Change) {
                                    let text = state.read(cx).value().to_string();
                                    this.edit_provider_text(&extension, &def, &text, cx);
                                }
                            },
                        ));
                        provider_inputs.push((key, state));
                    }
                }
            }
        }
        Self {
            settings,
            category: SETTINGS_CATEGORIES[0],
            providers,
            provider_categories,
            provider: None,
            provider_inputs,
            provider_selects,
            setting_errors: BTreeMap::new(),
            search,
            selects,
            _subscriptions: subscriptions,
            path: settings_path(),
            unsaved: false,
            save_generation: 0,
            note: None,
            note_generation: 0,
            #[cfg(test)]
            rendered: Vec::new(),
            #[cfg(test)]
            rendered_changes: Vec::new(),
            #[cfg(test)]
            rendered_dots: Vec::new(),
            #[cfg(test)]
            rendered_search: false,
            #[cfg(test)]
            rendered_categories: Vec::new(),
        }
    }
}
