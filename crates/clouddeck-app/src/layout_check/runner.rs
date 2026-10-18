//! The test that checks all layout rules.

use std::cell::RefCell;
use std::collections::HashSet;

use gpui::{TestAppContext, px, size};

use super::rules::{Rule, all_rules, evaluate};
use super::states::{self, State};
use crate::ui_record;

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

/// Panics listing every rule id that appears more than once.
fn assert_unique_ids(rules: &[Rule]) {
    let mut seen = HashSet::new();
    let mut duplicates: Vec<&str> = rules
        .iter()
        .filter(|rule| !seen.insert(rule.id))
        .map(|rule| rule.id)
        .collect();
    duplicates.sort_unstable();
    duplicates.dedup();
    assert!(
        duplicates.is_empty(),
        "duplicate rule ids: {}",
        duplicates.join(", ")
    );
}

#[gpui::test]
fn layout_rules_hold(cx: &mut TestAppContext) {
    let rules = all_rules();
    assert_unique_ids(&rules);

    let mut groups: Vec<((State, (f32, f32)), Vec<&Rule>)> = Vec::new();
    for rule in &rules {
        let key = (rule.state, rule.window);
        match groups.iter_mut().find(|(group, _)| *group == key) {
            Some((_, members)) => members.push(rule),
            None => groups.push((key, vec![rule])),
        }
    }

    let mut problems = Vec::new();
    for ((state, (w, h)), members) in groups {
        cx.update(|app| ui_record::clear(app));
        let (_fleet, _main, vcx) = states::build(state, w, h, cx);
        let vcx = RefCell::new(vcx);
        let lookup = |sel: &'static str| vcx.borrow_mut().debug_bounds(sel);
        let records =
            |sel: &'static str| vcx.borrow_mut().update(|_, app| ui_record::recorded(app, sel));
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
    }

    assert!(problems.is_empty(), "\n{}", problems.join("\n"));
}

/// Every named state has at least one layout rule.
#[gpui::test]
fn layout_rules_cover_states(_cx: &mut TestAppContext) {
    const ALL: [State; 15] = [
        State::MainIdle,
        State::MainRowSelected,
        State::MainEmpty,
        State::MainFailure,
        State::MainPartial,
        State::MainLarge,
        State::SearchFocusedEmpty,
        State::SearchTypingSt,
        State::SearchError,
        State::OverlayProfile,
        State::OverlayRegion,
        State::OverlayPalette,
        State::OverlayFailure,
        State::OverlayRowMenu,
        State::Welcome,
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
        assert_unique_ids(&[rule("a", None), rule("b", None)]);
        let result = std::panic::catch_unwind(|| {
            assert_unique_ids(&[rule("a", None), rule("dup", None), rule("dup", None)]);
        });
        let message = result
            .expect_err("duplicate ids should panic")
            .downcast::<String>()
            .expect("assert! panics with a String");
        assert!(message.contains("dup"), "{message}");
    }
}
