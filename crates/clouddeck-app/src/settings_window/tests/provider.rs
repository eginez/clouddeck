use super::*;

#[gpui::test]
fn extensions_with_settings_get_a_category_after_the_builtins(cx: &mut TestAppContext) {
    let (view, cx) = provider_window(cx);

    let names = view.read_with(cx, |view, _| {
        view.provider_categories
            .iter()
            .map(|category| category.name.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(names, ["Sample", "Toy"]);
    assert!(cx.debug_bounds("settings.cat.9").is_some());
    assert!(cx.debug_bounds("settings.cat.10").is_some());
    assert_eq!(category_icon(9), AppIcon::Plug);
}

#[gpui::test]
fn provider_category_shows_one_row_per_declared_setting(cx: &mut TestAppContext) {
    let (view, cx) = provider_window(cx);
    click(cx, "settings.cat.9");

    assert_eq!(
        view.read_with(cx, |view, _| view.provider.clone())
            .as_deref(),
        Some("sample")
    );
    for row in [
        "settings.provider.item.sample.region_label",
        "settings.provider.item.sample.config_path",
        "settings.provider.item.sample.verbose",
        "settings.provider.item.sample.retries",
        "settings.provider.item.sample.timeout_secs",
        "settings.provider.item.sample.mode",
        "settings.provider.item.sample.token",
    ] {
        assert!(cx.debug_bounds(row).is_some(), "{row}");
    }
    assert!(
        cx.debug_bounds("settings.provider.control.sample.token")
            .is_some()
    );
}

#[gpui::test]
fn editing_a_provider_text_setting_stores_it_in_the_registry(cx: &mut TestAppContext) {
    let (view, cx) = provider_window(cx);
    view.update_in(cx, |view, window, cx| {
        view.show_provider("sample", window, cx)
    });
    cx.run_until_parked();

    view.update_in(cx, |view, window, cx| {
        view.type_provider_text("sample", "region_label", "Europe", window, cx)
    });
    cx.run_until_parked();

    assert_eq!(
        view.read_with(cx, |view, _| view
            .providers
            .setting_value("sample", "region_label")),
        Some(SettingValue::Text("Europe".into()))
    );
    assert!(
        cx.debug_bounds("settings.provider.changed.sample.region_label")
            .is_some()
    );
}

#[gpui::test]
fn editing_a_provider_setting_asks_the_app_to_reload_that_provider(cx: &mut TestAppContext) {
    let (view, cx) = provider_window(cx);
    let events = recorded_events(&view, cx);
    view.update_in(cx, |view, window, cx| {
        view.show_provider("sample", window, cx)
    });
    cx.run_until_parked();

    view.update_in(cx, |view, window, cx| {
        view.type_provider_text("sample", "region_label", "Europe", window, cx)
    });
    cx.run_until_parked();

    assert!(
        events
            .borrow()
            .contains(&SettingsEvent::ProviderSettingChanged {
                extension: "sample".into(),
                key: Some("region_label".into()),
            })
    );
}

#[gpui::test]
fn a_missing_folder_is_flagged_but_kept(cx: &mut TestAppContext) {
    let (view, cx) = provider_window(cx);
    view.update_in(cx, |view, window, cx| {
        view.show_provider("toy", window, cx);
        view.type_provider_text("toy", "config_dir", "/clouddeck-missing-folder", window, cx);
    });
    cx.run_until_parked();

    assert_eq!(
        view.read_with(cx, |view, _| view
            .providers
            .setting_host_text("toy", "config_dir")),
        Some("/clouddeck-missing-folder".to_string())
    );
    assert!(
        cx.debug_bounds("settings.provider.missing.toy.config_dir")
            .is_some()
    );
    assert!(
        cx.debug_bounds("settings.provider.error.toy.config_dir")
            .is_none()
    );
}

#[gpui::test]
fn clearing_a_folder_leaves_it_empty_without_a_warning(cx: &mut TestAppContext) {
    let (view, cx) = provider_window(cx);
    view.update_in(cx, |view, window, cx| {
        view.show_provider("toy", window, cx);
        view.type_provider_text("toy", "config_dir", "/clouddeck-missing-folder", window, cx);
    });
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        view.clear_provider_folder("toy", "config_dir", window, cx)
    });
    cx.run_until_parked();

    assert_eq!(
        view.read_with(cx, |view, _| view
            .providers
            .setting_host_text("toy", "config_dir")),
        Some(String::new())
    );
}

#[test]
fn only_a_set_folder_that_is_not_a_directory_is_missing() {
    assert!(!folder_is_missing(None));
    assert!(!folder_is_missing(Some(String::new())));
    assert!(folder_is_missing(Some("/clouddeck-missing-folder".into())));
    assert!(!folder_is_missing(Some(
        std::env::temp_dir().display().to_string()
    )));
}

#[gpui::test]
fn invalid_provider_value_shows_an_error_and_is_not_saved(cx: &mut TestAppContext) {
    let (view, cx) = provider_window(cx);
    view.update_in(cx, |view, window, cx| {
        view.show_provider("sample", window, cx)
    });
    cx.run_until_parked();

    view.update_in(cx, |view, window, cx| {
        view.type_provider_text("sample", "retries", "99", window, cx)
    });
    cx.run_until_parked();

    assert!(
        cx.debug_bounds("settings.provider.error.sample.retries")
            .is_some()
    );
    assert!(!view.read_with(cx, |view, _| {
        view.providers.setting_changed("sample", "retries")
    }));
}

#[gpui::test]
fn search_finds_provider_settings_and_clicking_the_hit_opens_its_category(
    cx: &mut TestAppContext,
) {
    let (view, cx) = provider_window(cx);

    type_query(&view, cx, "timeout");

    assert!(
        cx.debug_bounds("settings.provider.item.sample.timeout_secs")
            .is_some()
    );
    click(cx, "settings.cat.9");
    assert_eq!(
        view.read_with(cx, |view, _| view.provider.clone())
            .as_deref(),
        Some("sample")
    );
    assert!(
        cx.debug_bounds("settings.provider.item.sample.region_label")
            .is_some()
    );
}

#[gpui::test]
fn restore_defaults_resets_only_the_selected_provider(cx: &mut TestAppContext) {
    let (view, cx) = provider_window(cx);
    view.update_in(cx, |view, window, cx| view.show_provider("toy", window, cx));
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        view.type_provider_text("toy", "display_name", "Hi", window, cx)
    });
    view.update_in(cx, |view, window, cx| {
        view.show_provider("sample", window, cx)
    });
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        view.type_provider_text("sample", "region_label", "Europe", window, cx)
    });
    cx.run_until_parked();

    click(cx, "settings.restore");

    assert!(!view.read_with(cx, |view, _| {
        view.providers.setting_changed("sample", "region_label")
    }));
    assert!(view.read_with(cx, |view, _| {
        view.providers.setting_changed("toy", "display_name")
    }));
}

#[gpui::test]
fn restore_defaults_with_a_search_active_leaves_built_in_settings_alone(
    cx: &mut TestAppContext,
) {
    let (view, cx) = provider_window(cx);
    view.update_in(cx, |view, window, cx| {
        view.show_provider("sample", window, cx)
    });
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        view.type_provider_text("sample", "region_label", "Europe", window, cx)
    });
    view.update(cx, |view, cx| {
        view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx)
    });
    type_query(&view, cx, "timeout");
    let before = view.read_with(cx, |view, _| view.settings.clone());

    click(cx, "settings.restore");

    assert_eq!(view.read_with(cx, |view, _| view.settings.clone()), before);
    assert!(!view.read_with(cx, |view, _| {
        view.providers.setting_changed("sample", "region_label")
    }));
}
