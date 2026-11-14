//! The test that checks all layout rules.

use std::cell::RefCell;

use gpui::{TestAppContext, px, size};

use super::assert_unique_ids;
use super::rules::{Rule, all_rules, evaluate};
use super::states::{self, State};
use crate::ui_record;

type RuleGroup<'r> = ((State, (f32, f32)), Vec<&'r Rule>);

#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Pass,
    Fail,
    Known,
    Fixed,
}

/// Classifies a rule result, where a known_bug rule is expected to fail.
fn classify(rule: &Rule, result: &Result<(), String>) -> Outcome {
    match (result.is_ok(), rule.known_bug) {
        (true, None) => Outcome::Pass,
        (false, None) => Outcome::Fail,
        (false, Some(_)) => Outcome::Known,
        (true, Some(_)) => Outcome::Fixed,
    }
}

#[gpui::test]
fn layout_rules_hold(cx: &mut TestAppContext) {
    let rules = all_rules();
    assert_unique_ids(&rules, |rule| rule.id, "rule");

    let mut groups: Vec<RuleGroup<'_>> = Vec::new();
    for rule in &rules {
        let key = (rule.state, rule.window);
        match groups.iter_mut().find(|(group, _)| *group == key) {
            Some((_, members)) => members.push(rule),
            None => groups.push((key, vec![rule])),
        }
    }

    let mut problems = Vec::new();
    for ((state, (w, h)), members) in groups {
        cx.update(ui_record::clear);
        let (_fleet, _main, vcx) = states::build(state, w, h, cx);
        let vcx = RefCell::new(vcx);
        let lookup = |sel: &'static str| vcx.borrow_mut().debug_bounds(sel);
        let records = |sel: &'static str| {
            vcx.borrow_mut()
                .update(|_, app| ui_record::recorded(app, sel))
        };
        let window = size(px(w), px(h));

        for rule in members {
            let result = evaluate(&rule.check, &lookup, &records, window);
            match classify(rule, &result) {
                Outcome::Pass => {}
                Outcome::Known => {
                    println!("KNOWN {}: {}", rule.id, rule.known_bug.unwrap_or_default())
                }
                Outcome::Fail => problems.push(format!(
                    "FAIL {} [{state:?} {w}x{h}]: {} — why: {}",
                    rule.id,
                    result.unwrap_err(),
                    rule.why
                )),
                Outcome::Fixed => {
                    problems.push(format!("FIXED? {} now passes; remove known_bug", rule.id))
                }
            }
        }
        // A window left open keeps redrawing (its table wipes the shared `table.*` recordings
        // on every render), so a later state would read whichever window drew last.
        vcx.borrow_mut().update(|window, _| window.remove_window());
    }

    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// Every named state has at least one layout rule.
#[gpui::test]
fn layout_rules_cover_states(_cx: &mut TestAppContext) {
    const ALL: [State; 56] = [
        State::MainIdle,
        State::MainRowSelected,
        State::MainEmpty,
        State::MainFailure,
        State::MainPartial,
        State::MainLarge,
        State::MainStateGrouped,
        State::MainStateGroupedDark,
        State::MainStateGroupedCollapsed,
        State::MainStateGroupedCollapsedDark,
        State::SearchFocusedEmpty,
        State::SearchTypingSt,
        State::SearchError,
        State::OverlayProfile,
        State::OverlayRegion,
        State::OverlayPalette,
        State::OverlayFailure,
        State::OverlayRowMenu,
        State::OverlayColumnMenu,
        State::OverlayColumnMenuDark,
        State::OnboardingReadyLight,
        State::OnboardingReadyDark,
        State::OnboardingReadyNarrow,
        State::OnboardingDetecting,
        State::OnboardingEmpty,
        State::OnboardingError,
        State::OnboardingErrorNarrow,
        State::OnboardingOpening,
        State::AboutLight,
        State::AboutDark,
        State::AboutRefusedPlugin,
        State::AboutLongPluginList,
        State::AboutLongPluginName,
        State::PluginsLight,
        State::PluginsDark,
        State::PluginsInstalled,
        State::PluginsRefused,
        State::PluginsEmptySearch,
        State::PluginsAvailableEmpty,
        State::PluginsRestart,
        State::PluginsNarrow,
        State::SettingsLight,
        State::SettingsDark,
        State::SettingsNarrow,
        State::SettingsNarrowDark,
        State::ProviderSettingsLight,
        State::ProviderSettingsDark,
        State::ProviderSettingsChanged,
        State::ProviderSettingsInvalid,
        State::ProviderSettingsSearch,
        State::ProviderFolderEmptyLight,
        State::ProviderFolderEmptyDark,
        State::ProviderFolderSetLight,
        State::ProviderFolderSetDark,
        State::ProviderFolderMissingLight,
        State::ProviderFolderMissingDark,
    ];
    let rules = all_rules();
    let uncovered: Vec<State> = ALL
        .into_iter()
        .filter(|state| !rules.iter().any(|rule| rule.state == *state))
        .collect();
    assert!(uncovered.is_empty(), "states without a rule: {uncovered:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout_check::rules::Check;

    fn rule(id: &'static str, known_bug: Option<&'static str>) -> Rule {
        Rule {
            id,
            state: State::MainIdle,
            window: (1280.0, 760.0),
            check: Check::Present("x"),
            why: "why",
            known_bug,
        }
    }

    #[test]
    fn known_bug_semantics() {
        let plain = rule("plain", None);
        let known = rule("known", Some("bead"));
        let ok: Result<(), String> = Ok(());
        let err: Result<(), String> = Err("boom".to_string());

        assert_eq!(classify(&plain, &ok), Outcome::Pass);
        assert_eq!(classify(&plain, &err), Outcome::Fail);
        assert_eq!(classify(&known, &err), Outcome::Known);
        assert_eq!(classify(&known, &ok), Outcome::Fixed);
    }

    #[test]
    fn duplicate_ids_panic() {
        assert_unique_ids(&[rule("a", None), rule("b", None)], |rule| rule.id, "rule");
        let result = std::panic::catch_unwind(|| {
            assert_unique_ids(
                &[rule("a", None), rule("dup", None), rule("dup", None)],
                |rule| rule.id,
                "rule",
            );
        });
        let message = result
            .expect_err("duplicate ids should panic")
            .downcast::<String>()
            .expect("assert! panics with a String");
        assert!(message.contains("dup"), "{message}");
    }
}
