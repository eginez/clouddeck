use super::*;

#[test]
fn minimum_window_width_fits_the_columns_that_are_never_dropped() {
    let fixed: f32 = clouddeck_core::visible_columns(0.0)
        .iter()
        .map(|column| column.default_width())
        .sum();
    assert!(
        MAIN_WINDOW_MIN_WIDTH >= fixed,
        "{MAIN_WINDOW_MIN_WIDTH} < {fixed}"
    );
}

#[gpui::test]
fn wide_window_with_nothing_selected_shows_panes_and_table(cx: &mut TestAppContext) {
    let (_fleet, _main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

    assert!(cx.debug_bounds("main.toolbar").is_some());
    assert!(cx.debug_bounds("main.sidebar").is_some());
    assert!(cx.debug_bounds("main.table").is_some());
    assert!(cx.debug_bounds("main.statusbar").is_some());
    assert!(cx.debug_bounds("main.inspector").is_none());
    assert!(cx.debug_bounds("main.scrim").is_none());
}

#[gpui::test]
fn selecting_a_row_shows_the_inspector(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

    click_center_of("table.row.us-east-1:i-1", cx);

    assert!(rendered(&main, cx).inspector);
    assert!(cx.debug_bounds("main.inspector").is_some());
}

#[gpui::test]
fn narrow_window_shows_inspector_as_overlay_with_scrim(cx: &mut TestAppContext) {
    let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 700.0, 760.0);
    assert!(!rendered(&main, cx).inspector);

    fleet.update(cx, |fleet, cx| {
        fleet.select(Some("us-east-1:i-1".to_string()), cx)
    });
    cx.run_until_parked();

    assert!(rendered(&main, cx).inspector);
    assert!(rendered(&main, cx).scrim);
    assert!(cx.debug_bounds("main.inspector").is_some());
}

#[gpui::test]
fn widening_a_narrow_window_reopens_the_sidebar(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

    cx.simulate_resize(size(px(700.0), px(760.0)));
    cx.run_until_parked();
    assert!(!rendered(&main, cx).sidebar);

    cx.simulate_resize(size(px(1280.0), px(760.0)));
    cx.run_until_parked();
    assert!(rendered(&main, cx).sidebar);
    assert!(!rendered(&main, cx).scrim);
}

#[gpui::test]
fn query_with_no_matches_shows_the_centered_state_instead_of_the_table(cx: &mut TestAppContext) {
    let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    assert!(!rendered(&main, cx).center_state);

    set_query(&fleet, "zzz-no-match", cx);

    assert!(rendered(&main, cx).center_state);
}

#[gpui::test]
fn empty_fleet_shows_the_centered_state(cx: &mut TestAppContext) {
    let (_fleet, _main, cx) = rooted_main_window(cx, vec![], 1280.0, 760.0);

    assert!(cx.debug_bounds("main.center-state").is_some());
    assert!(cx.debug_bounds("main.table").is_none());
}

fn center_state_of(main: &Entity<MainWindow>, cx: &mut VisualTestContext) -> Option<CenterState> {
    main.read_with(cx, |main, cx| {
        main.center_state_for(cx).map(|(state, _)| state)
    })
}

#[gpui::test]
fn throttled_load_shows_the_throttled_centered_state(cx: &mut TestAppContext) {
    let provider = fixtures::failing_provider("Rate exceeded");
    let (_fleet, main, cx) = rooted_main_window_with_service(cx, provider, 1280.0, 760.0);

    assert_eq!(center_state_of(&main, cx), Some(CenterState::Throttled));
}

#[gpui::test]
fn expired_credentials_load_shows_the_signed_out_centered_state(cx: &mut TestAppContext) {
    let provider =
        fixtures::failing_provider("The security token included in the request is expired");
    let (_fleet, main, cx) = rooted_main_window_with_service(cx, provider, 1280.0, 760.0);

    assert_eq!(center_state_of(&main, cx), Some(CenterState::SignedOut));
}

#[gpui::test]
fn signed_out_retry_reloads_the_fleet(cx: &mut TestAppContext) {
    let calls = Arc::new(AtomicUsize::new(0));
    let service = counting_service(
        &calls,
        Some("The security token included in the request is expired"),
    );
    let (_fleet, main, cx) = rooted_main_window_with_service(cx, service, 1280.0, 760.0);
    assert_eq!(center_state_of(&main, cx), Some(CenterState::SignedOut));

    click_center_of("center.action", cx);
    wait_until(cx, |cx| {
        loads(&calls) == 2 && main.read_with(cx, |main, cx| main.center_state_for(cx).is_none())
    });

    assert_eq!(loads(&calls), 2);
    assert_eq!(center_state_of(&main, cx), None);
}

#[gpui::test]
fn error_retry_reloads_the_fleet(cx: &mut TestAppContext) {
    let calls = Arc::new(AtomicUsize::new(0));
    let service = counting_service(&calls, Some("boom"));
    let (_fleet, main, cx) = rooted_main_window_with_service(cx, service, 1280.0, 760.0);
    assert!(matches!(
        center_state_of(&main, cx),
        Some(CenterState::Error { .. })
    ));

    click_center_of("center.action", cx);
    wait_until(cx, |cx| {
        loads(&calls) == 2 && main.read_with(cx, |main, cx| main.center_state_for(cx).is_none())
    });

    assert_eq!(loads(&calls), 2);
    assert_eq!(center_state_of(&main, cx), None);
}

#[gpui::test]
fn throttled_retry_now_reloads_the_fleet(cx: &mut TestAppContext) {
    let calls = Arc::new(AtomicUsize::new(0));
    let service = counting_service(&calls, Some("Rate exceeded"));
    let (_fleet, main, cx) = rooted_main_window_with_service(cx, service, 1280.0, 760.0);
    assert_eq!(center_state_of(&main, cx), Some(CenterState::Throttled));

    click_center_of("center.action", cx);
    wait_until(cx, |cx| {
        loads(&calls) == 2 && main.read_with(cx, |main, cx| main.center_state_for(cx).is_none())
    });

    assert_eq!(loads(&calls), 2);
    assert_eq!(center_state_of(&main, cx), None);
}

#[gpui::test]
fn no_match_clear_filters_clears_the_query(cx: &mut TestAppContext) {
    let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    set_query(&fleet, "zzz-no-match", cx);
    assert_eq!(center_state_of(&main, cx), Some(CenterState::NoMatch));

    click_center_of("center.action", cx);

    assert_eq!(
        fleet.read_with(cx, |fleet, _| fleet.query().to_string()),
        ""
    );
    assert_eq!(center_state_of(&main, cx), None);
}

#[gpui::test]
fn empty_fleet_select_all_regions_emits_regions_changed(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, vec![], 1280.0, 760.0);
    main.update(cx, |main, cx| {
        let available = vec!["us-east-1".to_string(), "eu-west-1".to_string()];
        main.available_regions = available.clone();
        main.context_model
            .set_dimensions(vec![region_dimension(&available)]);
        set_region_values(&mut main.context_model, &["us-east-1".to_string()]);
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(center_state_of(&main, cx), Some(CenterState::EmptyFleet));
    let events: Rc<RefCell<Vec<MainWindowEvent>>> = Rc::new(RefCell::new(Vec::new()));
    let sink = events.clone();
    cx.update(|_, cx| {
        cx.subscribe(&main, move |_, event: &MainWindowEvent, _| {
            sink.borrow_mut().push(event.clone())
        })
        .detach();
    });

    click_center_of("center.action", cx);

    assert_eq!(
        *events.borrow(),
        vec![MainWindowEvent::RegionsChanged(vec![
            "us-east-1".to_string(),
            "eu-west-1".to_string(),
        ])]
    );
    assert!(main.read_with(cx, |main, _| main.context_model.is_all(REGION)));
}

#[gpui::test]
fn narrow_window_closes_panes_and_scrim_click_closes_overlays(cx: &mut TestAppContext) {
    let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);

    cx.simulate_resize(size(px(700.0), px(760.0)));
    cx.run_until_parked();
    assert!(!rendered(&main, cx).sidebar);

    fleet.update(cx, |fleet, cx| {
        fleet.sidebar_open = true;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(
        rendered(&main, cx),
        RenderedPanes {
            sidebar: true,
            inspector: false,
            scrim: true,
            center_state: false,
        }
    );

    click_center_of("main.scrim", cx);

    assert!(!rendered(&main, cx).sidebar);
    assert!(!rendered(&main, cx).scrim);
}

#[gpui::test]
fn toolbar_sidebar_button_hides_the_sidebar(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    assert!(rendered(&main, cx).sidebar);

    click_center_of("toolbar.sidebar", cx);

    assert!(!rendered(&main, cx).sidebar);
}
