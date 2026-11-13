use super::*;

#[gpui::test]
fn every_category_row_has_the_same_icon_gap(cx: &mut TestAppContext) {
    let (_view, cx) = rooted(cx, Settings::default());
    let pairs = [
        ("settings.cat-icon.0", "settings.cat-label.0"),
        ("settings.cat-icon.1", "settings.cat-label.1"),
        ("settings.cat-icon.2", "settings.cat-label.2"),
        ("settings.cat-icon.3", "settings.cat-label.3"),
        ("settings.cat-icon.4", "settings.cat-label.4"),
        ("settings.cat-icon.5", "settings.cat-label.5"),
        ("settings.cat-icon.6", "settings.cat-label.6"),
        ("settings.cat-icon.7", "settings.cat-label.7"),
        ("settings.cat-icon.8", "settings.cat-label.8"),
    ];
    for (icon_selector, label_selector) in pairs {
        let icon = cx.debug_bounds(icon_selector).expect("icon rendered");
        let label = cx.debug_bounds(label_selector).expect("label rendered");
        let gap = f32::from(label.origin.x - (icon.origin.x + icon.size.width));
        assert!((gap - 8.0).abs() < 0.5, "{label_selector}: gap {gap}px");
    }
}

#[gpui::test]
fn renders_the_tree_and_the_appearance_pane(cx: &mut TestAppContext) {
    let (_view, cx) = rooted(cx, Settings::default());

    assert!(cx.debug_bounds("settings.tree").is_some());
    assert!(cx.debug_bounds("settings.search").is_some());
    for selector in [
        "settings.cat.0",
        "settings.cat.1",
        "settings.cat.2",
        "settings.cat.3",
        "settings.cat.4",
        "settings.cat.5",
        "settings.cat.6",
        "settings.cat.7",
        "settings.cat.8",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "{selector}");
    }
    assert!(cx.debug_bounds("settings.pane").is_some());
    assert!(cx.debug_bounds("settings.item.Theme").is_some());
}

#[gpui::test]
fn choosing_a_category_shows_its_settings(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());

    click(cx, "settings.cat.1");

    assert_eq!(view.read_with(cx, |view, _| view.category), "Fleet");
    assert!(cx.debug_bounds("settings.item.AutoRefresh").is_some());
    assert!(view.read_with(cx, |view, _| {
        view.rendered.contains(&SettingId::AutoRefresh)
    }));
    assert!(!view.read_with(cx, |view, _| view.rendered.contains(&SettingId::Theme)));
}

#[gpui::test]
fn switch_toggles_the_setting_and_emits_changed(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());
    let events = recorded_events(&view, cx);

    click(cx, "settings.cat.1");
    click(cx, "settings.control.AutoRefresh");

    assert!(!view.read_with(cx, |view, _| view.settings.auto_refresh));
    assert!(matches!(
        events.borrow().as_slice(),
        [SettingsEvent::Changed(settings)] if !settings.auto_refresh
    ));
}

#[gpui::test]
fn a_change_inside_the_debounce_window_is_saved_on_quit(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());
    let path = view.read_with(cx, |view, _| view.path.clone());

    view.update(cx, |view, cx| {
        view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx)
    });
    cx.run_until_parked();
    assert!(!path.exists(), "the save should wait out the debounce");

    cx.cx.update(|cx| cx.shutdown());

    assert_eq!(Settings::load(&path).theme, Theme::Dark);
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[gpui::test]
fn done_saves_the_change_before_the_window_closes(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());
    let path = view.read_with(cx, |view, _| view.path.clone());

    view.update(cx, |view, cx| {
        view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx)
    });
    click(cx, "settings.done");

    assert_eq!(Settings::load(&path).theme, Theme::Dark);
    if let Some(dir) = path.parent() {
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[gpui::test]
fn refresh_interval_is_inert_while_auto_refresh_is_off(cx: &mut TestAppContext) {
    let settings = Settings {
        auto_refresh: false,
        ..Settings::default()
    };
    let (view, cx) = rooted(cx, settings);
    let events = recorded_events(&view, cx);

    click(cx, "settings.cat.1");
    click(cx, "settings.control.RefreshInterval");
    view.update(cx, |view, cx| {
        view.choose(SettingId::RefreshInterval, Pick::RefreshInterval(10), cx)
    });

    assert_eq!(
        view.read_with(cx, |view, _| view.settings.refresh_interval_minutes),
        5
    );
    assert!(events.borrow().is_empty());
}

#[gpui::test]
fn choosing_a_theme_updates_settings_and_emits_changed(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());
    let events = recorded_events(&view, cx);

    view.update(cx, |view, cx| {
        view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx)
    });

    assert_eq!(
        view.read_with(cx, |view, _| view.settings.theme),
        Theme::Dark
    );
    assert!(matches!(
        events.borrow().as_slice(),
        [SettingsEvent::Changed(settings)] if settings.theme == Theme::Dark
    ));
}

#[gpui::test]
fn opening_again_reuses_the_open_window(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);

    assert!(
        cx.update(|cx| {
            open_settings_window(
                cx,
                Settings::default(),
                Arc::new(ProviderRegistry::default()),
            )
        })
        .is_some()
    );
    assert!(
        cx.update(|cx| {
            open_settings_window(
                cx,
                Settings::default(),
                Arc::new(ProviderRegistry::default()),
            )
        })
        .is_some()
    );

    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
}

#[gpui::test]
fn changed_value_shows_markers_until_reset(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());

    view.update(cx, |view, cx| {
        view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx)
    });

    assert!(cx.debug_bounds("settings.changed.Theme").is_some());
    assert!(cx.debug_bounds("settings.reset.Theme").is_some());
    assert!(cx.debug_bounds("settings.cat-dot.0").is_some());

    click(cx, "settings.reset.Theme");

    assert_eq!(
        view.read_with(cx, |view, _| view.settings.theme),
        Theme::FollowSystem
    );
    assert!(!view.read_with(cx, |view, _| {
        view.rendered_changes.contains(&SettingId::Theme)
    }));
    assert!(!view.read_with(cx, |view, _| view.rendered_dots.contains(&0)));
}

#[gpui::test]
fn restore_defaults_resets_every_setting(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());
    view.update(cx, |view, cx| {
        view.choose(SettingId::Theme, Pick::Theme(Theme::Dark), cx);
        view.toggle(SettingId::ShowUpdatedTime, cx);
    });

    click(cx, "settings.restore");

    assert_eq!(
        view.read_with(cx, |view, _| view.settings.clone()),
        Settings::default()
    );
}

#[gpui::test]
fn keymap_lists_every_shortcut(cx: &mut TestAppContext) {
    let (_view, cx) = rooted(cx, Settings::default());

    click(cx, "settings.cat.4");

    assert!(cx.debug_bounds("settings.keymap.9").is_some());
    assert!(cx.debug_bounds("settings.keymap.10").is_none());
}

#[gpui::test]
fn about_pane_shows_version_and_update_button(cx: &mut TestAppContext) {
    let (_view, cx) = rooted(cx, Settings::default());

    click(cx, "settings.cat.8");

    assert!(cx.debug_bounds("about.version").is_some());
    assert!(cx.debug_bounds("about.check-updates").is_some());
    let icon = cx.debug_bounds("about.icon").expect("about icon rendered");
    assert!(icon.size.width > px(0.) && icon.size.height > px(0.));
}

#[gpui::test]
fn about_pane_opens_the_about_window(cx: &mut TestAppContext) {
    let (_view, cx) = rooted(cx, Settings::default());
    cx.update(|_, app| app_menus::install(app));
    click(cx, "settings.cat.8");
    let before = cx.update(|_, app| app.windows().len());

    click(cx, "about.open");

    assert_eq!(cx.update(|_, app| app.windows().len()), before + 1);
}

#[gpui::test]
fn plugins_pane_opens_the_plugins_window(cx: &mut TestAppContext) {
    let (_view, cx) = rooted(cx, Settings::default());
    cx.update(|_, app| app_menus::install(app));
    click(cx, "settings.cat.7");
    let before = cx.update(|_, app| app.windows().len());

    click(cx, "plugins.open");

    assert_eq!(cx.update(|_, app| app.windows().len()), before + 1);
}
