use super::*;
use crate::shell::launch::{LaunchOptions, USAGE, parse_launch_options};

fn parse(args: &[&str]) -> Result<LaunchOptions, String> {
    parse_launch_options(args.iter().map(|arg| arg.to_string()))
}

#[test]
fn size_flag_parses_width_and_height() {
    let options = parse(&["--size", "520x760"]).unwrap();
    assert_eq!(options.size, Some((520.0, 760.0)));
    assert!(!options.demo);
}

#[test]
fn invalid_sizes_are_rejected() {
    for bad in ["520", "520x", "x760", "0x760", "axb", "520X760", "-1x5"] {
        assert!(parse(&["--size", bad]).is_err(), "{bad}");
    }
    assert!(parse(&["--size"]).is_err());
}

#[test]
fn unknown_render_state_lists_the_valid_names() {
    let error = parse(&["--render-state", "Nope"]).unwrap_err();
    assert!(error.contains("Nope") && error.contains("OverlayPalette"));
}

#[test]
fn render_state_implies_demo_and_its_scenario() {
    let options = parse(&["--render-state", "MainEmpty", "--size", "800x600"]).unwrap();
    assert_eq!(options.state, Some(State::MainEmpty));
    assert!(options.demo);
    assert_eq!(options.scenario, DemoFleet::Empty);
    assert_eq!(options.size, Some((800.0, 600.0)));
}

#[test]
fn huge_demo_scenario_is_parsed() {
    let options = parse(&["--demo-scenario", "huge"]).unwrap();
    assert_eq!(options.scenario, DemoFleet::Huge);
    assert!(options.demo);
}

#[test]
fn explicit_demo_scenario_beats_the_state_scenario() {
    let options = parse(&["--demo-scenario", "large", "--render-state", "MainEmpty"]).unwrap();
    assert_eq!(options.scenario, DemoFleet::Large);
}

#[test]
fn version_flag_sets_print_version() {
    assert!(parse(&["--version"]).unwrap().print_version);
}

#[test]
fn short_version_flag_sets_print_version() {
    assert!(parse(&["-V"]).unwrap().print_version);
}

#[test]
fn version_flag_combines_with_demo() {
    let options = parse(&["--demo", "--version"]).unwrap();
    assert!(options.print_version);
    assert!(options.demo);
}

#[test]
fn no_flags_leave_print_version_off() {
    assert!(!parse(&[]).unwrap().print_version);
}

#[test]
fn usage_line_lists_version_flag() {
    assert!(USAGE.contains("--version"));
}

#[test]
fn every_state_name_round_trips() {
    for state in State::ALL {
        assert_eq!(State::from_name(state.name()), Some(state));
    }
}
