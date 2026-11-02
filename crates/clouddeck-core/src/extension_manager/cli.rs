use std::io::Write;

use super::install::{Listing, Manager, Outcome};

const USAGE: &str = "usage:
  clouddeck extension install <id>
  clouddeck extension install <https://github.com/owner/repo> [--trust-key <base64>]
  clouddeck extension update <id>
  clouddeck extension dev <path>
  clouddeck extension list
  clouddeck extension enable <id>
  clouddeck extension disable <id>
  clouddeck extension remove <id>
  options: --accept-new-key <base64>";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Install {
        target: String,
        trust_key: Option<String>,
        accept: Option<String>,
    },
    Update {
        id: String,
        accept: Option<String>,
    },
    Dev(String),
    List,
    Enable(String),
    Disable(String),
    Remove(String),
}

pub fn run(args: &[String], out: &mut dyn Write) -> i32 {
    let command = match parse(args) {
        Ok(command) => command,
        Err(message) => {
            let _ = writeln!(out, "error: {message}\n{USAGE}");
            return 2;
        }
    };
    match execute(&Manager::github(), command, out) {
        Ok(()) => 0,
        Err(message) => {
            let _ = writeln!(out, "error: {message}");
            1
        }
    }
}

fn parse(args: &[String]) -> Result<Command, String> {
    let mut positional = Vec::new();
    let mut trust_key = None;
    let mut accept = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--trust-key" => trust_key = Some(value(&mut rest, arg)?),
            "--accept-new-key" => accept = Some(value(&mut rest, arg)?),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            _ => positional.push(arg.clone()),
        }
    }
    let mut words = positional.into_iter();
    let verb = words.next().ok_or("missing command")?;
    let operand = words.next();
    if words.next().is_some() {
        return Err(format!("too many arguments for {verb}"));
    }
    let needs =
        |operand: Option<String>| operand.ok_or_else(|| format!("{verb} needs an argument"));
    let refuse_extra = |given: bool, option: &str| {
        if given {
            Err(format!("{option} does not apply to {verb}"))
        } else {
            Ok(())
        }
    };
    match verb.as_str() {
        "install" => Ok(Command::Install {
            target: needs(operand)?,
            trust_key,
            accept,
        }),
        "update" => {
            refuse_extra(trust_key.is_some(), "--trust-key")?;
            Ok(Command::Update {
                id: needs(operand)?,
                accept,
            })
        }
        "dev" | "list" | "enable" | "disable" | "remove" => {
            refuse_extra(trust_key.is_some(), "--trust-key")?;
            refuse_extra(accept.is_some(), "--accept-new-key")?;
            match verb.as_str() {
                "list" => match operand {
                    None => Ok(Command::List),
                    Some(_) => Err("list takes no argument".to_string()),
                },
                "dev" => Ok(Command::Dev(needs(operand)?)),
                "enable" => Ok(Command::Enable(needs(operand)?)),
                "disable" => Ok(Command::Disable(needs(operand)?)),
                _ => Ok(Command::Remove(needs(operand)?)),
            }
        }
        other => Err(format!("unknown command {other}")),
    }
}

fn value(rest: &mut std::slice::Iter<'_, String>, flag: &str) -> Result<String, String> {
    rest.next()
        .cloned()
        .ok_or_else(|| format!("{flag} needs a value"))
}

fn execute(manager: &Manager, command: Command, out: &mut dyn Write) -> Result<(), String> {
    let io_err = |error: std::io::Error| error.to_string();
    match command {
        Command::Install {
            target,
            trust_key,
            accept,
        } => {
            let outcome = if target.starts_with("https://") {
                manager.install_from_url(&target, trust_key.as_deref(), accept.as_deref())?
            } else {
                if trust_key.is_some() {
                    return Err("--trust-key applies only to a repository URL".to_string());
                }
                manager.install_from_index(&target, accept.as_deref())?
            };
            report(out, outcome).map_err(io_err)
        }
        Command::Update { id, accept } => {
            report(out, manager.update(&id, accept.as_deref())?).map_err(io_err)
        }
        Command::Dev(path) => {
            report(out, manager.install_dev(std::path::Path::new(&path))?).map_err(io_err)
        }
        Command::List => {
            let (listings, refusals) = manager.list()?;
            for listing in &listings {
                writeln!(out, "{}", listing_line(listing)).map_err(io_err)?;
            }
            for refusal in &refusals {
                writeln!(out, "refused: {refusal:?}").map_err(io_err)?;
            }
            Ok(())
        }
        Command::Enable(id) => {
            manager.enable(&id)?;
            writeln!(out, "{id} is enabled; it takes effect on restart").map_err(io_err)
        }
        Command::Disable(id) => {
            manager.disable(&id)?;
            writeln!(out, "{id} is disabled; it takes effect on restart").map_err(io_err)
        }
        Command::Remove(id) => {
            manager.remove(&id)?;
            writeln!(out, "{id} is removed; it takes effect on restart").map_err(io_err)
        }
    }
}

fn report(out: &mut dyn Write, outcome: Outcome) -> std::io::Result<()> {
    match outcome {
        Outcome::Installed(installed) => writeln!(
            out,
            "installed {} {} ({} capabilities); it stays disabled until enabled",
            installed.id,
            installed.version,
            installed.capabilities.len()
        ),
        Outcome::UpToDate { version } => writeln!(out, "already up to date at {version}"),
    }
}

fn listing_line(listing: &Listing) -> String {
    let mut line = format!(
        "{} {} {}",
        listing.id,
        listing.version,
        if listing.enabled {
            "enabled"
        } else {
            "disabled"
        }
    );
    if listing.dev {
        line.push_str(" dev");
    }
    if listing.crash_disabled {
        line.push_str(" crash-disabled");
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn parses_each_command() {
        assert_eq!(
            parse(&args(&["install", "toy"])).unwrap(),
            Command::Install {
                target: "toy".to_string(),
                trust_key: None,
                accept: None
            }
        );
        assert_eq!(
            parse(&args(&[
                "install",
                "https://github.com/a/b",
                "--trust-key",
                "KEY"
            ]))
            .unwrap(),
            Command::Install {
                target: "https://github.com/a/b".to_string(),
                trust_key: Some("KEY".to_string()),
                accept: None
            }
        );
        assert_eq!(
            parse(&args(&["update", "toy", "--accept-new-key", "NEW"])).unwrap(),
            Command::Update {
                id: "toy".to_string(),
                accept: Some("NEW".to_string())
            }
        );
        assert_eq!(
            parse(&args(&["dev", "./toy"])).unwrap(),
            Command::Dev("./toy".to_string())
        );
        assert_eq!(parse(&args(&["list"])).unwrap(), Command::List);
        assert_eq!(
            parse(&args(&["enable", "toy"])).unwrap(),
            Command::Enable("toy".to_string())
        );
        assert_eq!(
            parse(&args(&["disable", "toy"])).unwrap(),
            Command::Disable("toy".to_string())
        );
        assert_eq!(
            parse(&args(&["remove", "toy"])).unwrap(),
            Command::Remove("toy".to_string())
        );
    }

    #[test]
    fn rejects_unknown_options_commands_and_missing_values() {
        assert!(
            parse(&args(&["install", "toy", "--force"]))
                .unwrap_err()
                .contains("unknown option")
        );
        assert!(
            parse(&args(&["sign", "toy"]))
                .unwrap_err()
                .contains("unknown command")
        );
        assert!(
            parse(&args(&["install", "toy", "--trust-key"]))
                .unwrap_err()
                .contains("needs a value")
        );
        assert!(
            parse(&args(&["enable"]))
                .unwrap_err()
                .contains("needs an argument")
        );
        assert!(parse(&args(&["enable", "toy", "--trust-key", "K"])).is_err());
        assert!(parse(&args(&[])).is_err());
    }
}
