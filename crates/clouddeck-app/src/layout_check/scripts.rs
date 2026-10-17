//! Scripted flow runner.

use std::cell::RefCell;
use std::collections::HashSet;

use gpui::{
    Entity, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, px, size,
};

use super::flows_overlays;
use super::flows_search;
use super::rules::{Check, evaluate};
use super::states::{self, State};
use crate::fleet_state::FleetState;
use crate::main_window::MainWindow;

/// One user action in a script.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Step {
    /// Left-clicks the centre of the element with the selector.
    Click(&'static str),
    /// Right-clicks the centre of the element with the selector.
    RightClick(&'static str),
    /// Sends keystrokes such as "secondary-p" or "escape".
    Keys(&'static str),
    /// Types text into the focused element.
    Type(&'static str),
    /// Resizes the window to the given width and height in points.
    Resize(f32, f32),
}

/// One expectation checked after a step.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Expect {
    /// A geometric layout check against the current frame.
    Layout(Check),
    /// The overlay drawn by the main window, if any.
    Overlay(Option<&'static str>),
    /// Whether the sidebar and inspector are drawn by the main window.
    Panes { sidebar: bool, inspector: bool },
    /// The fleet's search query.
    Query(&'static str),
    /// The fleet's selected instance id.
    Selected(Option<&'static str>),
}

/// A named flow: a start state, then steps that each have expectations.
pub(crate) struct Script {
    /// Unique identifier reported on failure.
    pub id: &'static str,
    /// The named state the flow starts from.
    pub start: State,
    /// Window width and height in points.
    pub window: (f32, f32),
    /// Steps in order, each with the expectations checked after it.
    pub steps: Vec<(Step, Vec<Expect>)>,
    /// Why the flow matters, reported on failure.
    pub why: &'static str,
    /// Describes the real UI bug this flow currently trips on, if any.
    pub known_bug: Option<&'static str>,
}

const NO_MAIN_WINDOW: &str = "no main window in this state";

/// Runs `script` in a fresh window, stopping at the first failed expectation.
pub(crate) fn run_script(script: &Script, cx: &mut TestAppContext) -> Result<(), String> {
    let (mut width, mut height) = script.window;
    let (fleet, main, vcx) = states::build(script.start, width, height, cx);
    for (index, (step, expects)) in script.steps.iter().enumerate() {
        let fail = |error: String| format!("step {} ({step:?}): {error}", index + 1);
        if let Step::Resize(w, h) = *step {
            (width, height) = (w, h);
        }
        perform(*step, vcx).map_err(&fail)?;
        for expect in expects {
            check(expect, vcx, main.as_ref(), fleet.as_ref(), (width, height)).map_err(&fail)?;
        }
    }
    Ok(())
}

fn perform(step: Step, cx: &mut VisualTestContext) -> Result<(), String> {
    match step {
        Step::Click(sel) => {
            let center = centre_of(sel, cx)?;
            cx.simulate_click(center, Modifiers::none());
        }
        Step::RightClick(sel) => {
            let center = centre_of(sel, cx)?;
            cx.simulate_mouse_down(center, MouseButton::Right, Modifiers::none());
            cx.simulate_mouse_up(center, MouseButton::Right, Modifiers::none());
        }
        Step::Keys(keys) => cx.simulate_keystrokes(keys),
        Step::Type(text) => cx.simulate_input(text),
        Step::Resize(w, h) => cx.simulate_resize(size(px(w), px(h))),
    }
    cx.run_until_parked();
    Ok(())
}

fn centre_of(sel: &'static str, cx: &mut VisualTestContext) -> Result<Point<Pixels>, String> {
    cx.debug_bounds(sel)
        .map(|bounds| bounds.center())
        .ok_or_else(|| format!("{sel} was not rendered"))
}

fn check(
    expect: &Expect,
    cx: &mut VisualTestContext,
    main: Option<&Entity<MainWindow>>,
    fleet: Option<&Entity<FleetState>>,
    window: (f32, f32),
) -> Result<(), String> {
    match *expect {
        Expect::Layout(Check::Absent(_)) => Err("use Overlay/Panes for absence".to_string()),
        Expect::Layout(check) => {
            let cell = RefCell::new(&mut *cx);
            let lookup = |sel: &'static str| cell.borrow_mut().debug_bounds(sel);
            evaluate(&check, &lookup, size(px(window.0), px(window.1)))
        }
        Expect::Overlay(want) => {
            let main = main.ok_or_else(|| NO_MAIN_WINDOW.to_string())?;
            let got = main.read_with(cx, |main, _| main.rendered_overlay());
            if got == want {
                Ok(())
            } else {
                Err(format!("overlay is {got:?}, expected {want:?}"))
            }
        }
        Expect::Panes { sidebar, inspector } => {
            let main = main.ok_or_else(|| NO_MAIN_WINDOW.to_string())?;
            let got = main.read_with(cx, |main, _| main.rendered_panes());
            if got == (sidebar, inspector) {
                Ok(())
            } else {
                Err(format!(
                    "panes (sidebar, inspector) are {got:?}, expected {:?}",
                    (sidebar, inspector)
                ))
            }
        }
        Expect::Query(want) => {
            let fleet = fleet.ok_or_else(|| NO_MAIN_WINDOW.to_string())?;
            let got = fleet.read_with(cx, |fleet, _| fleet.query().to_string());
            if got == want {
                Ok(())
            } else {
                Err(format!("query is {got:?}, expected {want:?}"))
            }
        }
        Expect::Selected(want) => {
            let fleet = fleet.ok_or_else(|| NO_MAIN_WINDOW.to_string())?;
            let got = fleet.read_with(cx, |fleet, _| fleet.selected.clone());
            if got.as_deref() == want {
                Ok(())
            } else {
                Err(format!("selected is {got:?}, expected {want:?}"))
            }
        }
    }
}

/// Panics listing every script id that appears more than once.
fn assert_unique_ids(scripts: &[Script]) {
    let mut seen = HashSet::new();
    let mut duplicates: Vec<&str> = scripts
        .iter()
        .filter(|script| !seen.insert(script.id))
        .map(|script| script.id)
        .collect();
    duplicates.sort_unstable();
    duplicates.dedup();
    assert!(
        duplicates.is_empty(),
        "duplicate script ids: {}",
        duplicates.join(", ")
    );
}

#[gpui::test]
fn scripts_hold(cx: &mut TestAppContext) {
    let mut scripts = flows_overlays::scripts();
    scripts.extend(flows_search::scripts());
    assert_unique_ids(&scripts);

    let mut problems = Vec::new();
    for script in &scripts {
        match (run_script(script, cx), script.known_bug) {
            (Ok(()), None) => {}
            (Err(error), None) => {
                problems.push(format!("FAIL {} {error} — why: {}", script.id, script.why))
            }
            (Err(_), Some(bug)) => println!("KNOWN {}: {bug}", script.id),
            (Ok(()), Some(_)) => {
                problems.push(format!("FIXED? {} now passes; remove known_bug", script.id))
            }
        }
    }

    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn script_runner_opens_and_closes_profile_overlay(cx: &mut TestAppContext) {
        let script = Script {
            id: "self-profile-overlay",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![
                (
                    Step::Keys("secondary-p"),
                    vec![Expect::Overlay(Some("main.overlay.profile"))],
                ),
                (Step::Keys("escape"), vec![Expect::Overlay(None)]),
            ],
            why: "self-test",
            known_bug: None,
        };
        assert_eq!(run_script(&script, cx), Ok(()));
    }

    #[gpui::test]
    fn script_runner_rejects_layout_absent(cx: &mut TestAppContext) {
        let script = Script {
            id: "self-layout-absent",
            start: State::MainIdle,
            window: (1280.0, 760.0),
            steps: vec![(
                Step::Keys("secondary-p"),
                vec![Expect::Layout(Check::Absent("main.overlay.profile"))],
            )],
            why: "self-test",
            known_bug: None,
        };
        let err = run_script(&script, cx).unwrap_err();
        assert!(err.contains("use Overlay/Panes for absence"), "{err}");
    }
}
