use super::*;

#[gpui::test]
fn dark_active_palette_renders_the_main_window(cx: &mut TestAppContext) {
    let (_, _main, cx) = rooted_main_window(cx, three_instances(), 1200.0, 800.0);
    cx.update(|_, cx| cx.set_global(ActivePalette(Palette::dark())));
    cx.update(|_, cx| cx.refresh_windows());
    cx.run_until_parked();

    assert!(cx.debug_bounds("main.table").is_some());
    assert_eq!(
        cx.update(|_, cx| crate::tokens::palette(cx)),
        Palette::dark()
    );
}

#[gpui::test]
fn appearance_setting_drives_the_active_palette_and_component_theme(cx: &mut TestAppContext) {
    let (_, _main, cx) = rooted_main_window(cx, three_instances(), 1200.0, 800.0);

    cx.update(|window, cx| apply_appearance(Theme::Dark, window, cx));
    assert_eq!(
        cx.update(|_, cx| crate::tokens::palette(cx)),
        Palette::dark()
    );
    assert!(cx.update(|_, cx| gpui_component::Theme::global(cx).is_dark()));

    cx.update(|window, cx| apply_appearance(Theme::Light, window, cx));
    assert_eq!(
        cx.update(|_, cx| crate::tokens::palette(cx)),
        Palette::for_dark(false)
    );
    assert!(!cx.update(|_, cx| gpui_component::Theme::global(cx).is_dark()));
}

#[test]
fn window_options_sets_app_id_for_desktop_entry_matching() {
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(800.0), px(600.0)));
    assert_eq!(window_options(bounds).app_id, Some("clouddeck".to_string()));
}
