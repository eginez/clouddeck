use super::*;

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LaunchOptions {
    pub demo: bool,
    pub scenario: DemoFleet,
    /// Content size of the main window, from `--size WxH`.
    pub size: Option<(f32, f32)>,
    /// State to open the main window in, from `--render-state <Name>`.
    pub state: Option<State>,
    /// Print the version and exit, from --version or -V.
    pub print_version: bool,
    /// Open the onboarding window at launch, from `--onboarding [provider]`; the inner
    /// option is the provider to select, the first one when absent.
    pub onboarding: Option<Option<String>>,
}

/// Parses `WxH` (positive whole numbers) into a window content size.
pub(super) fn parse_size(text: &str) -> Option<(f32, f32)> {
    let (width, height) = text.split_once('x')?;
    let width: u16 = width.parse().ok().filter(|w| *w > 0)?;
    let height: u16 = height.parse().ok().filter(|h| *h > 0)?;
    Some((f32::from(width), f32::from(height)))
}

/// Parses launch arguments (without the program name). `--render-state` implies `--demo`
/// and the scenario that state needs unless `--demo-scenario` is also given.
pub(crate) fn parse_launch_options(
    args: impl IntoIterator<Item = String>,
) -> Result<LaunchOptions, String> {
    let mut demo = false;
    let mut explicit_scenario = None;
    let mut size = None;
    let mut state = None;
    let mut print_version = false;
    let mut onboarding = None;
    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => demo = true,
            "--version" | "-V" => print_version = true,
            "--onboarding" => {
                onboarding = Some(args.next_if(|next| !next.starts_with("--")));
            }
            "--demo-scenario" => {
                explicit_scenario = Some(match args.next().as_deref() {
                    Some("empty") => DemoFleet::Empty,
                    Some("failure") => DemoFleet::Failure,
                    Some("partial") => DemoFleet::Partial,
                    Some("large") => DemoFleet::Large,
                    Some("huge") => DemoFleet::Huge,
                    _ => DemoFleet::Happy,
                });
                demo = true;
            }
            "--size" => {
                let value = args.next().unwrap_or_default();
                size = Some(parse_size(&value).ok_or_else(|| {
                    format!("invalid --size '{value}': expected WxH, e.g. 520x760")
                })?);
            }
            "--render-state" => {
                let value = args.next().unwrap_or_default();
                state = Some(State::from_name(&value).ok_or_else(|| {
                    format!(
                        "unknown --render-state '{value}'; valid names: {}",
                        State::names()
                    )
                })?);
                demo = true;
            }
            _ => {}
        }
    }
    let scenario = explicit_scenario
        .or_else(|| state.map(State::demo_scenario))
        .unwrap_or(DemoFleet::Happy);
    Ok(LaunchOptions {
        demo,
        scenario,
        size,
        state,
        print_version,
        onboarding,
    })
}

pub(super) const USAGE: &str = "usage: clouddeck [--demo] [--demo-scenario <name>] [--size WxH] [--render-state <Name>] [--onboarding [provider]] [--version]";

/// Launch options from the process arguments; exits with status 2 on a usage error
/// and status 0 after printing the version.
pub(crate) fn launch_options() -> LaunchOptions {
    let options = parse_launch_options(std::env::args().skip(1)).unwrap_or_else(|message| {
        eprintln!("clouddeck: {message}");
        eprintln!("{USAGE}");
        std::process::exit(2)
    });
    if options.print_version {
        println!("clouddeck {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }
    options
}
