//! Named UI states built in fresh windows.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use clouddeck_core::{
    ComputeService, DirectoryScenarioCatalog, EC2Instance, EC2InstanceTag, MockComputeService,
    RegionScope, ScenarioService, TestScenarioKind,
};
use gpui::{AppContext, Bounds, Entity, TestAppContext, VisualTestContext, point, px, size};

use crate::fleet_state::FleetState;
use crate::main_window::{MainWindow, window_options};
use crate::view_models::InstanceListViewModel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum State {
    /// Fleet loaded with nothing selected.
    MainIdle,
    /// Fleet loaded with the first visible row selected.
    MainRowSelected,
    /// Fleet loaded with no instances.
    MainEmpty,
    /// Fleet load failed outright.
    MainFailure,
    /// Two regions where one fails.
    MainPartial,
    /// A fleet of 120 instances.
    MainLarge,
}

/// The fixture scenario catalog shipped with clouddeck-core.
fn catalog() -> DirectoryScenarioCatalog {
    DirectoryScenarioCatalog::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../clouddeck-core/testdata/ec2-scenarios"
    ))
}

/// Resolves a fixture-backed scenario kind.
fn resolve(kind: TestScenarioKind) -> ScenarioService {
    ScenarioService::for_kind(kind, &catalog()).expect("fixture scenario should load")
}

/// 120 instances cloned from the first small-happy-path instance.
fn large_fleet() -> Vec<EC2Instance> {
    let resolved = resolve(TestScenarioKind::SmallHappyPath);
    let scenario = resolved.scenario().expect("fixture-backed scenario");
    let (profile, region) = scenario
        .default_selection()
        .expect("fixture has a default selection");
    let template = scenario
        .instances(Some(&profile), &region)
        .expect("fixture instances should load")
        .into_iter()
        .next()
        .expect("fixture has at least one instance");
    (0..120)
        .map(|n| {
            let mut instance = template.clone();
            instance.instance_id = format!("i-large-{n:03}");
            instance.state = Some(if n % 2 == 0 { "running" } else { "stopped" }.to_string());
            instance.tags.retain(|tag| tag.key != "Name");
            instance.tags.push(EC2InstanceTag {
                key: "Name".to_string(),
                value: format!("large-instance-{n:03}"),
            });
            instance
        })
        .collect()
}

/// The compute service and region list that back `state`.
fn service_for(state: State) -> (Box<dyn ComputeService + Send + Sync>, Vec<String>) {
    let us_east = vec!["us-east-1".to_string()];
    match state {
        State::MainIdle | State::MainRowSelected => (
            Box::new(resolve(TestScenarioKind::SmallHappyPath).into_service()),
            us_east,
        ),
        State::MainEmpty => (Box::new(MockComputeService::new(vec![], None)), us_east),
        State::MainFailure => (
            Box::new(MockComputeService::new(
                vec![],
                Some(std::io::Error::other("Simulated demo load failure").into()),
            )),
            us_east,
        ),
        State::MainPartial => (
            Box::new(resolve(TestScenarioKind::MultiRegionPartialFailure).into_service()),
            vec![
                "us-east-1".to_string(),
                "eu-west-1".to_string(),
                "ap-southeast-2".to_string(),
            ],
        ),
        State::MainLarge => (
            Box::new(MockComputeService::new(large_fleet(), None)),
            us_east,
        ),
    }
}

/// Pumps the test dispatcher until `condition` holds, panicking after about a second.
fn wait_until(cx: &mut TestAppContext, mut condition: impl FnMut(&mut TestAppContext) -> bool) {
    for _ in 0..200 {
        cx.run_until_parked();
        if condition(cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("condition was not satisfied within the timeout");
}

/// Builds `state` in a fresh main window of the given size, with its fleet loaded.
pub(crate) fn build<'a>(
    state: State,
    width: f32,
    height: f32,
    cx: &'a mut TestAppContext,
) -> (
    Entity<FleetState>,
    Entity<MainWindow>,
    &'a mut VisualTestContext,
) {
    let (service, regions) = service_for(state);
    let vm = InstanceListViewModel::new(service);
    let fleet = cx.new(|_| FleetState::new(vm, "default".to_string(), true));
    fleet.update(cx, |fleet, cx| fleet.refresh(cx));
    wait_until(cx, |cx| {
        fleet.read_with(cx, |fleet, _| {
            !fleet.view_model.is_loading()
                && (fleet.view_model.last_success().is_some() || fleet.view_model.error().is_some())
        })
    });

    let main_slot: Rc<RefCell<Option<Entity<MainWindow>>>> = Rc::new(RefCell::new(None));
    let bounds = Bounds::new(point(px(0.0), px(0.0)), size(px(width), px(height)));
    let slot_for_window = main_slot.clone();
    let fleet_for_window = fleet.clone();
    let window = cx.update(|cx| {
        cx.open_window(window_options(bounds), move |window, cx| {
            gpui_component::init(cx);
            let scope = RegionScope::all(&regions);
            let main = cx.new(|cx| MainWindow::new(fleet_for_window, regions, scope, window, cx));
            *slot_for_window.borrow_mut() = Some(main.clone());
            cx.new(|cx| gpui_component::Root::new(main, window, cx))
        })
        .unwrap()
    });
    let cx = VisualTestContext::from_window(window.into(), cx).into_mut();
    cx.run_until_parked();
    let main = main_slot
        .borrow()
        .as_ref()
        .expect("main window entity should be created")
        .clone();

    // AppShell normally copies partial failures into the window; no shell here.
    let failures = fleet.read_with(cx, |fleet, _| fleet.view_model.partial_failures().to_vec());
    main.update(cx, |main, cx| {
        main.failures = failures;
        cx.notify();
    });
    cx.run_until_parked();

    if state == State::MainRowSelected {
        let key = fleet.read_with(cx, |fleet, _| {
            fleet
                .visible_rows()
                .first()
                .expect("fleet has a visible row")
                .id()
        });
        fleet.update(cx, |fleet, cx| fleet.select(Some(key), cx));
        cx.run_until_parked();
    }

    (fleet, main, cx)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [State; 6] = [
        State::MainIdle,
        State::MainRowSelected,
        State::MainEmpty,
        State::MainFailure,
        State::MainPartial,
        State::MainLarge,
    ];

    #[gpui::test]
    fn every_state_renders_root_and_statusbar(cx: &mut TestAppContext) {
        for state in ALL {
            let (_fleet, _main, window_cx) = build(state, 1280.0, 760.0, cx);
            assert!(
                window_cx.debug_bounds("main.root").is_some(),
                "{state:?}: main.root"
            );
            assert!(
                window_cx.debug_bounds("main.statusbar").is_some(),
                "{state:?}: main.statusbar"
            );
        }
    }

    #[gpui::test]
    fn selected_row_shows_the_inspector(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::MainRowSelected, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("main.inspector").is_some());
    }

    #[gpui::test]
    fn idle_window_has_no_inspector(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::MainIdle, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("main.inspector").is_none());
    }

    #[gpui::test]
    fn empty_and_failure_show_the_center_state(cx: &mut TestAppContext) {
        for state in [State::MainEmpty, State::MainFailure] {
            let (_fleet, _main, window_cx) = build(state, 1280.0, 760.0, cx);
            assert!(
                window_cx.debug_bounds("main.center-state").is_some(),
                "{state:?}: main.center-state"
            );
        }
    }

    #[gpui::test]
    fn large_fleet_lists_every_instance(cx: &mut TestAppContext) {
        let (fleet, _main, cx) = build(State::MainLarge, 1280.0, 760.0, cx);

        assert_eq!(
            fleet.read_with(cx, |fleet, _| fleet.visible_rows().len()),
            120
        );
    }

    #[gpui::test]
    fn partial_failure_shows_the_failure_bar(cx: &mut TestAppContext) {
        let (_fleet, _main, cx) = build(State::MainPartial, 1280.0, 760.0, cx);

        assert!(cx.debug_bounds("bar.failure").is_some());
    }
}
