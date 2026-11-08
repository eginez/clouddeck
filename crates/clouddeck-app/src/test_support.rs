use crate::fleet_state::FleetState;
use gpui::{Entity, TestAppContext};
use std::time::Duration;

pub(crate) fn wait_until(
    cx: &mut TestAppContext,
    mut condition: impl FnMut(&mut TestAppContext) -> bool,
) {
    for _ in 0..200 {
        cx.run_until_parked();
        if condition(cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("condition was not satisfied within the timeout");
}

pub(crate) fn load_fleet(cx: &mut TestAppContext, state: &Entity<FleetState>) {
    state.update(cx, |state, cx| state.refresh(cx));
    wait_until(cx, |cx| {
        state.read_with(cx, |state, _| {
            !state.view_model.instances().is_empty() && !state.view_model.is_loading()
        })
    });
}
