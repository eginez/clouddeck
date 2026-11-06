# Contributing to CloudDeck

Thanks for your interest in CloudDeck. Bug reports, documentation fixes, and
pull requests are welcome.

## Getting started

CloudDeck is a Rust workspace with two crates:

- `crates/clouddeck-core`: the platform-independent domain layer (EC2 models,
  search, profiles, loading, and the AWS SDK adapter). It builds and tests
  without a display.
- `crates/clouddeck-app`: the GPUI desktop shell, built as the `clouddeck`
  binary.

Install a recent stable Rust toolchain, then see the [README](README.md) for
the desktop libraries GPUI needs on Linux.

## Build and test

Run these before opening a pull request. They match the checks the release
workflow runs in CI before it builds:

```bash
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Core tests use fixtures and injected services and never contact AWS. Do not add
tests that need real AWS credentials or network access.

## Code quality gate

The workspace is checked with [rustqual](https://github.com/SaschaOnTour/rustqual)
1.8.3, which is required: install it with
`cargo install rustqual --version 1.8.3 --locked`. Its settings live in
`rustqual.toml`, and the accepted state lives in `.rustqual-baseline.json`.

`cargo test --workspace` runs the gate (`crates/clouddeck-core/tests/quality_gate.rs`),
so CI and the release workflow enforce it too. It fails when the quality score
drops below the baseline, and when any architecture rule is violated, even if the
score is unchanged. Existing findings do not fail it. The architecture rules in
`rustqual.toml` enforce the crate dependency direction: `clouddeck-provider-api` <-
`clouddeck-extension-api` <- `clouddeck-core` <- `clouddeck-app`. The example
extension `clouddeck-extension-toy` sits outside that chain and the app may not
import it.

To run the gate's checks by hand, from the repository root:

```bash
rustqual . --compare .rustqual-baseline.json --fail-on-regression --no-fail
```

If rustqual is not on `PATH`, set `CLOUDDECK_RUSTQUAL` to the binary. For local
runs only, `CLOUDDECK_SKIP_QUALITY=1` skips the gate. CI, the release workflow,
and automated workers must never set it.

The baseline may only improve. A change that lowers the score or adds findings
is rejected. Fixing findings and then regenerating the baseline with
`rustqual . --save-baseline .rustqual-baseline.json` is fine. Only the
maintainer or the planner edits the baseline, and never to hide a regression.
Each suppression in `rustqual.toml` carries a written reason.

Shell scripts in `scripts/` are checked with `shellcheck --severity=warning`,
and CI runs the same check before each release.

## Pull requests

- Keep each pull request focused on one change, and describe what it changes
  and why in the description.
- Add or update tests for behaviour you change.
- Keep the read only guarantee in mind: CloudDeck must not add cloud mutations
  or store credentials.
- Use clear commit messages written in the imperative mood.

## Automated contributors

This section applies to coding agents and automated workers as well as to
people. The repository deliberately has no CLAUDE.md or AGENTS.md: this file,
the rest of CONTRIBUTING.md, the README, and `docs/` are the repository-level
guidance.

- Follow "Build and test" and "Code quality gate" above. Do not copy their
  commands elsewhere, and keep them in step with the checks CI runs.
- Never set `CLOUDDECK_SKIP_QUALITY`, and never edit `.rustqual-baseline.json`.
- Keep the read only guarantee: add no cloud mutations, store no credentials,
  and write no tests that need real AWS credentials or network access.
- The layout rules in `crates/clouddeck-app/src/layout_check/` define the
  expected UI. A failing rule means the UI is wrong, unless the maintainer has
  changed the design for that rule. Never mark a rule `known_bug` to make it pass, and treat a
  box that measures 0x0 as a UI bug. If you believe a rule itself is wrong, stop
  and raise it with whoever assigned the work instead of changing the rule.
- Task-specific instructions come from whoever assigned the work. This section
  covers only what holds for every change to this repository.

## Licensing of contributions

CloudDeck is dual licensed under MIT OR Apache-2.0. By submitting a contribution
you agree that it is licensed under the same terms, as described in the
License section of the [README](README.md).

## Conduct and security

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md). Report security
vulnerabilities privately as described in [SECURITY.md](SECURITY.md), not in a
public issue or pull request.
