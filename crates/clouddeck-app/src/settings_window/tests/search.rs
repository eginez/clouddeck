use super::*;

#[gpui::test]
fn search_shows_matching_settings_and_keymap_rows(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());

    type_query(&view, cx, "refresh");

    assert!(cx.debug_bounds("settings.search-crumb").is_some());
    assert!(cx.debug_bounds("settings.item.AutoRefresh").is_some());
    assert!(cx.debug_bounds("settings.item.RefreshOnFocus").is_some());
    assert!(cx.debug_bounds("settings.keymap.0").is_some());
    assert!(!view.read_with(cx, |view, _| view.rendered.contains(&SettingId::Theme)));
    assert_eq!(
        view.read_with(cx, |view, cx| search_settings(&view.query(cx)).title()),
        "4 results"
    );
}

#[gpui::test]
fn search_with_no_matches_shows_the_empty_message(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());

    type_query(&view, cx, "zzz");

    assert!(cx.debug_bounds("settings.no-match").is_some());
}

#[gpui::test]
fn clearing_the_query_restores_the_normal_pane(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());
    type_query(&view, cx, "refresh");

    type_query(&view, cx, "");

    assert!(!view.read_with(cx, |view, _| view.rendered_search));
    assert!(cx.debug_bounds("settings.item.Theme").is_some());
}

#[gpui::test]
fn search_tree_shows_only_categories_with_results(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());
    type_query(&view, cx, "refresh");

    assert!(!view.read_with(cx, |view, _| view.rendered_categories.contains(&0)));
    assert!(cx.debug_bounds("settings.cat.1").is_some());
    assert!(cx.debug_bounds("settings.cat.4").is_some());

    click(cx, "settings.cat.1");

    assert_eq!(view.read_with(cx, |view, _| view.category), "Fleet");
    assert!(!view.read_with(cx, |view, _| view.rendered_search));
    assert!(cx.debug_bounds("settings.item.AutoRefresh").is_some());
}

#[gpui::test]
fn escape_clears_the_search(cx: &mut TestAppContext) {
    let (view, cx) = rooted(cx, Settings::default());
    type_query(&view, cx, "refresh");
    let search = view.read_with(cx, |view, _| view.search.clone());
    search.update_in(cx, |state, window, cx| state.focus(window, cx));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert!(!view.read_with(cx, |view, _| view.rendered_search));
    assert!(cx.debug_bounds("settings.item.Theme").is_some());
}
