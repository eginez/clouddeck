use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const PINNED_VERSION: &str = "1.8.3";
const INSTALL_HINT: &str = "cargo install rustqual --version 1.8.3 --locked";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn rustqual_binary() -> String {
    match std::env::var("CLOUDDECK_RUSTQUAL") {
        Ok(path) => std::fs::canonicalize(&path)
            .map(|p| p.display().to_string())
            .unwrap_or(path),
        Err(_) => "rustqual".to_string(),
    }
}

fn missing_rustqual(detail: &str) -> ! {
    panic!(
        "rustqual {PINNED_VERSION} is required by the quality gate ({detail}).\n\
         Install it with: {INSTALL_HINT}\n\
         or point CLOUDDECK_RUSTQUAL at the binary.\n\
         CLOUDDECK_SKIP_QUALITY=1 skips the gate for local runs only."
    )
}

fn check_version(bin: &str) {
    let output = Command::new(bin)
        .arg("--version")
        .output()
        .unwrap_or_else(|e| missing_rustqual(&format!("cannot run `{bin}`: {e}")));
    let found = String::from_utf8_lossy(&output.stdout);
    if found.trim() != format!("rustqual {PINNED_VERSION}") {
        missing_rustqual(&format!("found `{}`", found.trim()));
    }
}

fn run(bin: &str, root: &Path, args: &[&str]) -> Output {
    Command::new(bin)
        .current_dir(root)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run rustqual: {e}"))
}

fn tail(output: &Output, lines: usize) -> String {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let all: Vec<&str> = text.lines().collect();
    all[all.len().saturating_sub(lines)..].join("\n")
}

fn architecture_findings(findings: &str) -> Vec<&str> {
    findings
        .lines()
        .filter(|line| line.split_whitespace().nth(1) == Some("ARCHITECTURE"))
        .collect()
}

fn baseline_iosp_violations(root: &Path) -> u64 {
    let text = std::fs::read_to_string(root.join(".rustqual-baseline.json"))
        .expect("read .rustqual-baseline.json");
    let baseline: Value = serde_json::from_str(&text).expect("parse .rustqual-baseline.json");
    baseline["violations"]
        .as_u64()
        .expect("baseline `violations` count")
}

fn iosp_violation_sites(report: &Value) -> Vec<String> {
    report["functions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|f| f["classification"] == "violation")
        .map(|f| {
            let name = match f["parent_type"].as_str() {
                Some(parent) => format!("{parent}::{}", f["name"].as_str().unwrap_or("?")),
                None => f["name"].as_str().unwrap_or("?").to_string(),
            };
            format!(
                "  {name} at {}:{}",
                f["file"].as_str().unwrap_or("?"),
                f["line"]
            )
        })
        .collect()
}

#[test]
fn rustqual_gate_holds_against_baseline() {
    if std::env::var("CLOUDDECK_SKIP_QUALITY").as_deref() == Ok("1") {
        assert!(
            std::env::var_os("CI").is_none(),
            "CLOUDDECK_SKIP_QUALITY=1 is local use only and is refused under CI"
        );
        eprintln!("quality gate skipped: CLOUDDECK_SKIP_QUALITY=1 (local use only)");
        return;
    }

    let bin = rustqual_binary();
    check_version(&bin);
    // Run from the repository root: rustqual.toml globs and the baseline paths are root-relative.
    let root = workspace_root();

    let compare = run(
        &bin,
        &root,
        &[
            ".",
            "--compare",
            ".rustqual-baseline.json",
            "--fail-on-regression",
            "--no-fail",
        ],
    );
    assert!(
        compare.status.success(),
        "quality score regressed against .rustqual-baseline.json\n{}",
        tail(&compare, 60)
    );

    // The score can rise while one architecture rule breaks, so architecture findings get zero tolerance.
    let findings = run(&bin, &root, &[".", "--findings", "--no-fail"]);
    assert!(
        findings.status.success(),
        "rustqual --findings failed\n{}",
        tail(&findings, 60)
    );
    let text = String::from_utf8_lossy(&findings.stdout);
    let violations = architecture_findings(&text);
    assert!(
        violations.is_empty(),
        "architecture violations found (zero tolerance):\n{}",
        violations.join("\n")
    );

    let baseline = baseline_iosp_violations(&root);
    let report = run(&bin, &root, &[".", "--json", "--no-fail"]);
    assert!(
        report.status.success(),
        "rustqual --json failed\n{}",
        tail(&report, 60)
    );
    let report: Value =
        serde_json::from_slice(&report.stdout).expect("rustqual --json output is valid JSON");
    let current = report["summary"]["violations"]
        .as_u64()
        .expect("rustqual summary `violations` count");
    assert!(
        current <= baseline,
        "IOSP violations rose from {baseline} (.rustqual-baseline.json) to {current}; split the new functions into logic and call-only parts:\n{}",
        iosp_violation_sites(&report).join("\n")
    );
}
