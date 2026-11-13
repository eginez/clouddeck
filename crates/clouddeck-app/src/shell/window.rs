use super::*;

pub(crate) fn open_main_window(
    cx: &mut App,
    demo: bool,
    demo_scenario: DemoFleet,
) -> Option<WindowHandle<Root>> {
    open_main_window_with(cx, demo, demo_scenario, None, None)
}

/// Opens the main window at `size` (content size) and, if given, drives it into `state`
/// once the fleet has loaded, then forces a redraw so the first frame needs no input.
pub(crate) fn open_main_window_with(
    cx: &mut App,
    demo: bool,
    demo_scenario: DemoFleet,
    size: Option<(f32, f32)>,
    state: Option<State>,
) -> Option<WindowHandle<Root>> {
    let handle = open_window_sized(cx, demo, demo_scenario, size)?;
    if let Some(state) = state {
        apply_state_when_loaded(handle, state, cx);
        let _ = handle.update(cx, |_, window, _| window.refresh());
    }
    Some(handle)
}

pub(super) fn apply_state_when_loaded(handle: WindowHandle<Root>, state: State, cx: &mut App) {
    let parts = handle
        .update(cx, |root, _, cx| {
            let shell = root.view().clone().downcast::<AppShell>().ok()?;
            let shell = shell.read(cx);
            Some((shell.fleet.clone(), shell.main_window.clone()))
        })
        .ok()
        .flatten();
    let Some((fleet, main)) = parts else { return };
    let applied = std::rc::Rc::new(std::cell::Cell::new(false));
    cx.observe(&fleet.clone(), move |observed, cx| {
        if applied.get() || !fleet_loaded(observed.read(cx)) {
            return;
        }
        applied.set(true);
        let _ = handle.update(cx, |_, window, cx| {
            apply_render_state(state, &observed, &main, DEFAULT_ROW_MENU_ANCHOR, window, cx);
            main.update(cx, |_, cx| cx.notify());
            window.refresh();
        });
    })
    .detach();
}

pub(super) fn open_window_sized(
    cx: &mut App,
    demo: bool,
    demo_scenario: DemoFleet,
    size_override: Option<(f32, f32)>,
) -> Option<WindowHandle<Root>> {
    let restored = (!demo)
        .then(|| UiState::load(&clouddeck_core::ui_state_path()).window)
        .flatten()
        .map(|frame| {
            Bounds::new(
                point(px(frame.x), px(frame.y)),
                size(px(frame.width), px(frame.height)),
            )
        })
        .filter(|bounds| {
            cx.displays()
                .iter()
                .any(|display| display.bounds().intersects(bounds))
        });
    let bounds = match size_override {
        Some((width, height)) => Bounds::centered(None, size(px(width), px(height)), cx),
        None => restored.unwrap_or_else(|| Bounds::centered(None, size(px(1280.0), px(800.0)), cx)),
    };
    cx.open_window(crate::main_window::window_options(bounds), |window, cx| {
        let shell = cx.new(|cx| AppShell::build_with_scenario(window, demo, demo_scenario, cx));
        cx.new(|cx| gpui_component::Root::new(shell, window, cx))
    })
    .ok()
}
