//! Test-only recorder of what views render (text, colour tokens) per debug selector; a no-op in release builds.

#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
use gpui::Global;
use gpui::App;

/// What one tagged element rendered; `None` fields were not recorded.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recorded {
    pub text: Option<String>,
    pub fg: Option<&'static str>,
    pub bg: Option<&'static str>,
    pub font: Option<&'static str>,
}

/// Recorded elements keyed by debug selector.
#[cfg(test)]
#[derive(Default)]
struct Recordings(HashMap<String, Recorded>);

#[cfg(test)]
impl Global for Recordings {}

/// Stores what the element with `selector` rendered; re-recording replaces the entry.
#[cfg(test)]
pub fn record(cx: &mut App, selector: impl Into<String>, rec: Recorded) {
    cx.default_global::<Recordings>().0.insert(selector.into(), rec);
}

/// Release builds keep no recordings, so this does nothing.
#[cfg(not(test))]
#[inline]
#[allow(dead_code)]
pub fn record(_cx: &mut App, _selector: impl Into<String>, _rec: Recorded) {}

/// What the element with `selector` last rendered, or `None` if nothing was recorded for it.
#[cfg(test)]
pub fn recorded(cx: &App, selector: &str) -> Option<Recorded> {
    cx.try_global::<Recordings>()
        .and_then(|recordings| recordings.0.get(selector).cloned())
}

/// Drops every recording.
#[cfg(test)]
pub fn clear(cx: &mut App) {
    cx.default_global::<Recordings>().0.clear();
}

#[cfg(test)]
mod tests {
    use gpui::TestAppContext;

    use super::*;

    fn run_label() -> Recorded {
        Recorded {
            text: Some("Running".to_string()),
            fg: Some("runtext"),
            bg: Some("run"),
            font: Some("ui"),
        }
    }

    fn stop_label() -> Recorded {
        Recorded {
            text: Some("Stopped".to_string()),
            fg: Some("dim"),
            bg: None,
            font: Some("mono"),
        }
    }

    #[gpui::test]
    fn records_two_selectors_and_returns_them(cx: &mut TestAppContext) {
        cx.update(|cx| {
            record(cx, "status.run", run_label());
            record(cx, String::from("status.stop"), stop_label());
        });

        cx.read(|cx| {
            assert_eq!(recorded(cx, "status.run"), Some(run_label()));
            assert_eq!(recorded(cx, "status.stop"), Some(stop_label()));
        });
    }

    #[gpui::test]
    fn re_recording_replaces_the_previous_entry(cx: &mut TestAppContext) {
        cx.update(|cx| {
            record(cx, "status.run", run_label());
            record(cx, "status.run", stop_label());
        });

        cx.read(|cx| assert_eq!(recorded(cx, "status.run"), Some(stop_label())));
    }

    #[gpui::test]
    fn clear_empties_the_recorder(cx: &mut TestAppContext) {
        cx.update(|cx| {
            record(cx, "status.run", run_label());
            clear(cx);
        });

        cx.read(|cx| assert_eq!(recorded(cx, "status.run"), None));
    }

    #[gpui::test]
    fn a_missing_selector_is_none(cx: &mut TestAppContext) {
        cx.update(|cx| record(cx, "status.run", run_label()));

        cx.read(|cx| assert_eq!(recorded(cx, "missing"), None));
    }
}
