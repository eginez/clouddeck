use super::*;

#[gpui::test]
fn switch_profile_opens_profile_overlay_and_escape_closes_it(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

    cx.dispatch_action(app_menus::SwitchProfile);
    cx.run_until_parked();
    assert_eq!(rendered_overlay(&main, cx), Some("main.overlay.profile"));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(rendered_overlay(&main, cx), None);
}

#[gpui::test]
fn palette_run_filter_stopped_sets_query_and_closes_overlay(cx: &mut TestAppContext) {
    let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

    cx.dispatch_action(app_menus::OpenCommandPalette);
    cx.run_until_parked();
    assert_eq!(rendered_overlay(&main, cx), Some("main.overlay.palette"));

    let palette = overlay_view::<CommandPalette>(&main, cx);
    palette.update(cx, |_, cx| {
        cx.emit(PaletteEvent::Run(CommandId::FilterStopped))
    });
    cx.run_until_parked();

    assert_eq!(
        fleet.read_with(cx, |fleet, _| fleet.query().to_string()),
        "state:stopped"
    );
    assert_eq!(rendered_overlay(&main, cx), None);
}

#[gpui::test]
fn about_palette_command_opens_the_about_window(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    let before = cx.update(|_, app| app.windows().len());

    cx.dispatch_action(app_menus::OpenCommandPalette);
    cx.run_until_parked();
    let palette = overlay_view::<CommandPalette>(&main, cx);
    palette.update(cx, |_, cx| cx.emit(PaletteEvent::Run(CommandId::About)));
    cx.run_until_parked();

    assert_eq!(cx.update(|_, app| app.windows().len()), before + 1);
}

#[gpui::test]
fn plugins_palette_command_opens_the_plugins_window(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    let before = cx.update(|_, app| app.windows().len());

    cx.dispatch_action(app_menus::OpenCommandPalette);
    cx.run_until_parked();
    let palette = overlay_view::<CommandPalette>(&main, cx);
    palette.update(cx, |_, cx| cx.emit(PaletteEvent::Run(CommandId::Plugins)));
    cx.run_until_parked();

    assert_eq!(cx.update(|_, app| app.windows().len()), before + 1);
}

#[gpui::test]
fn copy_instance_id_copies_the_selected_row_and_flashes(cx: &mut TestAppContext) {
    let (fleet, _main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    fleet.update(cx, |fleet, cx| {
        fleet.select(Some("us-east-1:i-1".to_string()), cx)
    });
    cx.run_until_parked();

    cx.dispatch_action(app_menus::CopyInstanceId);
    cx.run_until_parked();

    assert_eq!(
        cx.read_from_clipboard().and_then(|item| item.text()),
        Some("i-1".to_string())
    );
    assert!(fleet.read_with(cx, |fleet, _| fleet.flash.is_some()));
}

#[gpui::test]
fn clicking_failure_bar_opens_failure_overlay(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    main.update(cx, |main, cx| {
        main.failures = vec![ComputeServicePartialFailure {
            region: "eu-central-1".to_string(),
            message: "throttled".to_string(),
            error: CloudDeckError::Throttled,
        }];
        cx.notify();
    });
    cx.run_until_parked();

    click_center_of("bar.failure", cx);

    assert_eq!(rendered_overlay(&main, cx), Some("main.overlay.failure"));
}

#[gpui::test]
fn region_failures_keep_the_typed_error(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    main.update(cx, |main, cx| {
        main.failures = vec![ComputeServicePartialFailure {
            region: "eu-central-1".to_string(),
            message: "throttled".to_string(),
            error: CloudDeckError::Throttled,
        }];
        cx.notify();
    });
    cx.run_until_parked();

    let errors = main.read_with(cx, |main, cx| {
        main.region_failures(cx)
            .into_iter()
            .map(|failure| failure.error)
            .collect::<Vec<_>>()
    });

    assert_eq!(errors, vec![CloudDeckError::Throttled]);
}

#[gpui::test]
fn right_clicking_a_row_opens_row_menu_and_selects_it(cx: &mut TestAppContext) {
    let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    let row = cx
        .debug_bounds("table.row.us-east-1:i-2")
        .expect("row should render");

    cx.simulate_mouse_down(row.center(), MouseButton::Right, Default::default());
    cx.run_until_parked();

    assert_eq!(rendered_overlay(&main, cx), Some("main.overlay.rowmenu"));
    assert_eq!(
        fleet.read_with(cx, |fleet, _| fleet.selected.clone()),
        Some("us-east-1:i-2".to_string())
    );
}

#[gpui::test]
fn row_menu_opened_near_the_right_edge_stays_in_a_compact_window(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 520.0, 760.0);
    main.update_in(cx, |main, window, cx| {
        main.open_row_menu(
            "us-east-1:i-2".to_string(),
            point(px(500.0), px(100.0)),
            window,
            cx,
        )
    });
    cx.run_until_parked();

    let bounds = cx
        .debug_bounds("main.overlay.rowmenu")
        .expect("row menu should render");
    assert!(bounds.right() <= px(520.0));
    assert!(bounds.left() >= px(0.0));
}

#[gpui::test]
fn escape_with_selection_and_no_overlay_clears_selection(cx: &mut TestAppContext) {
    let (fleet, _main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    fleet.update(cx, |fleet, cx| {
        fleet.select(Some("us-east-1:i-1".to_string()), cx)
    });
    cx.run_until_parked();

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert_eq!(fleet.read_with(cx, |fleet, _| fleet.selected.clone()), None);
}

#[gpui::test]
fn escape_in_focused_search_clears_the_query(cx: &mut TestAppContext) {
    let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    cx.simulate_input("state:stopped");
    cx.run_until_parked();
    assert_eq!(query(&fleet, cx), "state:stopped");

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert_eq!(query(&fleet, cx), "");
    assert_eq!(search_input_text(&main, cx), "");
}

#[gpui::test]
fn escape_with_overlay_open_and_search_focused_closes_overlay_and_keeps_query(
    cx: &mut TestAppContext,
) {
    let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    fleet.update(cx, |fleet, cx| fleet.set_query("state:stopped", cx));
    cx.run_until_parked();

    cx.dispatch_action(app_menus::OpenCommandPalette);
    cx.run_until_parked();
    assert_eq!(rendered_overlay(&main, cx), Some("main.overlay.palette"));

    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    let input = main.read_with(cx, |main, cx| main.search.read(cx).input.clone());
    let focused = cx.update(|window, cx| input.read(cx).focus_handle(cx).is_focused(window));
    assert!(
        focused,
        "cmd-f should focus the search input over an open overlay"
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();

    assert_eq!(rendered_overlay(&main, cx), None);
    assert_eq!(query(&fleet, cx), "state:stopped");
}

#[gpui::test]
fn region_picker_change_emits_regions_changed_once_dismissed(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    let events: Rc<RefCell<Vec<MainWindowEvent>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&main, move |_, event: &MainWindowEvent, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach();
    });

    cx.dispatch_action(app_menus::SwitchRegion);
    cx.run_until_parked();
    let picker = overlay_view::<DimensionPicker>(&main, cx);
    picker.update(cx, |_, cx| {
        cx.emit(DimensionPickerEvent::Changed(vec![ValueId::from(
            "us-east-1",
        )]))
    });
    cx.run_until_parked();
    assert!(
        events.borrow().is_empty(),
        "the fleet should not reload while the picker is still open"
    );

    picker.update(cx, |_, cx| cx.emit(DimensionPickerEvent::Dismissed));
    cx.run_until_parked();

    assert_eq!(
        *events.borrow(),
        vec![MainWindowEvent::RegionsChanged(vec![
            "us-east-1".to_string()
        ])]
    );
}
