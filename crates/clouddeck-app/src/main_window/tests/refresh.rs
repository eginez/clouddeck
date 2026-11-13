use super::*;

#[gpui::test]
fn auto_refresh_loads_again_once_the_interval_passes_while_active(cx: &mut TestAppContext) {
    let calls = Arc::new(AtomicUsize::new(0));
    let (_fleet, main, cx) =
        rooted_main_window_with_service(cx, counting_service(&calls, None), 1280.0, 760.0);
    cx.update(|window, _| window.activate_window());
    set_refresh_policy(&main, true, false, cx);
    cx.run_until_parked();
    assert_eq!(loads(&calls), 1);

    cx.executor()
        .advance_clock(Duration::from_secs(5 * 60 + 30));
    wait_until(cx, |_| loads(&calls) == 2);

    cx.executor().advance_clock(Duration::from_secs(30));
    cx.run_until_parked();
    assert_eq!(loads(&calls), 2);
}

#[gpui::test]
fn auto_refresh_off_does_not_reload_on_the_interval(cx: &mut TestAppContext) {
    let calls = Arc::new(AtomicUsize::new(0));
    let (_fleet, main, cx) =
        rooted_main_window_with_service(cx, counting_service(&calls, None), 1280.0, 760.0);
    cx.update(|window, _| window.activate_window());
    set_refresh_policy(&main, false, false, cx);
    cx.run_until_parked();

    cx.executor()
        .advance_clock(Duration::from_secs(5 * 60 + 30));
    cx.run_until_parked();

    assert_eq!(loads(&calls), 1);
}

#[gpui::test]
fn regaining_focus_after_an_interval_away_reloads(cx: &mut TestAppContext) {
    let calls = Arc::new(AtomicUsize::new(0));
    let (_fleet, main, cx) =
        rooted_main_window_with_service(cx, counting_service(&calls, None), 1280.0, 760.0);
    cx.update(|window, _| window.activate_window());
    set_refresh_policy(&main, false, true, cx);
    cx.run_until_parked();
    assert_eq!(loads(&calls), 1);

    cx.deactivate_window();
    cx.executor().advance_clock(Duration::from_secs(5 * 60 + 1));
    cx.run_until_parked();
    cx.update(|window, _| window.activate_window());
    wait_until(cx, |_| loads(&calls) == 2);
}

#[gpui::test]
fn throttled_load_is_retried_after_the_backoff_delay(cx: &mut TestAppContext) {
    let calls = Arc::new(AtomicUsize::new(0));
    let (fleet, _main, cx) = rooted_main_window_with_service(
        cx,
        counting_service(&calls, Some("Rate exceeded")),
        1280.0,
        760.0,
    );
    assert_eq!(loads(&calls), 1);
    assert_eq!(
        fleet.read_with(cx, |fleet, _| fleet.retry_in_seconds),
        Some(2)
    );

    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(loads(&calls), 1);
    assert_eq!(
        fleet.read_with(cx, |fleet, _| fleet.retry_in_seconds),
        Some(1)
    );

    cx.executor().advance_clock(Duration::from_secs(1));
    wait_until(cx, |cx| {
        loads(&calls) == 2
            && fleet.read_with(cx, |fleet, _| {
                fleet.view_model.error().is_none()
                    && fleet.view_model.last_success().is_some()
                    && fleet.retry_in_seconds.is_none()
            })
    });
}

#[gpui::test]
fn updated_status_text_shows_the_last_successful_load(cx: &mut TestAppContext) {
    let (_fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    cx.run_until_parked();

    let text = main.read_with(cx, |main, cx| {
        main.context_bar.read(cx).last_success_text.clone()
    });
    assert!(text.starts_with("Updated "), "got {text:?}");
}

#[gpui::test]
fn failed_refresh_keeps_rows_and_reports_the_error_in_the_status_bar(cx: &mut TestAppContext) {
    let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    cx.run_until_parked();

    fleet.update(cx, |fleet, cx| {
        fleet
            .view_model
            .single_mut()
            .expect("single provider")
            .finish_load(failed_load("Rate exceeded"));
        cx.notify();
    });
    cx.run_until_parked();

    assert_eq!(
        fleet.read_with(cx, |fleet, _| fleet.view_model.instances().len()),
        3
    );
    let error_text = main.read_with(cx, |main, cx| {
        main.context_bar.read(cx).load_error_text.clone()
    });
    assert_eq!(error_text.as_deref(), Some("Refresh failed: Throttled"));
}

#[gpui::test]
fn refresh_error_stays_in_the_status_bar_until_the_next_load_finishes(cx: &mut TestAppContext) {
    let (fleet, main, cx) = rooted_main_window(cx, three_instances(), 1280.0, 760.0);
    cx.run_until_parked();

    fleet.update(cx, |fleet, cx| {
        fleet
            .view_model
            .single_mut()
            .expect("single provider")
            .finish_load(failed_load("Rate exceeded"));
        cx.notify();
    });
    cx.run_until_parked();

    fleet.update(cx, |fleet, cx| {
        let _ = fleet
            .view_model
            .single_mut()
            .expect("single provider")
            .begin_load();
        cx.notify();
    });
    cx.run_until_parked();
    let during_load = main.read_with(cx, |main, cx| {
        main.context_bar.read(cx).load_error_text.clone()
    });
    assert_eq!(during_load.as_deref(), Some("Refresh failed: Throttled"));

    fleet.update(cx, |fleet, cx| {
        fleet
            .view_model
            .single_mut()
            .expect("single provider")
            .finish_load(ListResult {
                resources: three_instances(),
                failures: vec![],
            });
        cx.notify();
    });
    cx.run_until_parked();
    let after_success = main.read_with(cx, |main, cx| {
        main.context_bar.read(cx).load_error_text.clone()
    });
    assert_eq!(after_success, None);
}
