mod shell;
mod theme;

use gpui::{App, Application, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};

use shell::AppShell;

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(800.0), px(600.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(|_| AppShell),
        )
        .unwrap();
        cx.activate(true);
    });
}
