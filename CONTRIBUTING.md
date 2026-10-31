# Contributing to CloudDeck

Thanks for your interest in CloudDeck. Bug reports, documentation fixes, and
pull requests are welcome.

## Getting started

CloudDeck is a Rust workspace with two crates:

- `crates/clouddeck-core`: the platform-independent domain layer (EC2 models,
  search, profiles, loading, and the AWS CLI adapter). It builds and tests
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

Shell scripts in `scripts/` are checked with `shellcheck --severity=warning`,
and CI runs the same check before each release.

## Pull requests

- Keep each pull request focused on one change, and describe what it changes
  and why in the description.
- Add or update tests for behaviour you change.
- Keep the read only guarantee in mind: CloudDeck must not add cloud mutations
  or store credentials.
- Use clear commit messages written in the imperative mood.

## Licensing of contributions

CloudDeck is dual licensed under MIT OR Apache-2.0. By submitting a contribution
you agree that it is licensed under the same terms, as described in the
License section of the [README](README.md).

## Conduct and security

Please follow the [Code of Conduct](CODE_OF_CONDUCT.md). Report security
vulnerabilities privately as described in [SECURITY.md](SECURITY.md), not in a
public issue or pull request.
