# CloudDeck Rust

CloudDeck is a read only EC2 fleet browser. The Rust workspace separates the
Linux testable domain layer (`clouddeck-core`) from the GPUI desktop shell
(`clouddeck-app`). The core owns EC2 models, filtering, profile and region
context, load state, and the best effort fleet aggregator. Live AWS discovery
uses the AWS CLI v2 as a subprocess so the CLI remains responsible for SSO,
credential providers, and pagination; CloudDeck does not store credentials or
perform cloud mutations.

## Prerequisites

- Rust stable (the repository's `Cargo.lock` may require a recent stable
  compiler).
- AWS CLI v2 on `PATH`, `/opt/homebrew/bin/aws`, `/usr/local/bin/aws`, or
  `/usr/bin/aws` for live discovery.
- An AWS profile configured in `~/.aws/config` and `~/.aws/credentials` (or
  the paths selected by `AWS_CONFIG_FILE` and
  `AWS_SHARED_CREDENTIALS_FILE`). SSO profiles must be logged in with
  `aws sso login --profile <name>` before refreshing.
- Read only permission for `ec2:DescribeInstances` in each selected region.

The first MVP offers a curated region list and a `fleet` view over
`us-east-1`, `us-west-2`, `eu-west-1`, and `eu-central-1`. A profile's region
metadata is used for the initial selection; the fleet view can report partial
region failures while retaining successful results.

## Run

Use the demo to exercise the desktop shell without AWS credentials:

```bash
cargo run -p clouddeck-app -- --demo
```

Run without `--demo` to load profiles from the AWS CLI configuration and make
read only `describe-instances` requests. The selected profile and region are
remembered in `$XDG_CONFIG_HOME/clouddeck/context.json` or
`$HOME/.config/clouddeck/context.json`; the file contains only profile and
region names.

## Test and build

The fast, credential free core gate runs on Linux and locally:

```bash
cargo test -p clouddeck-core
cargo build -p clouddeck-core
```

The GPUI shell has its own Linux compile and test check when the host has the
desktop libraries required by GPUI:

```bash
cargo check -p clouddeck-app
cargo test -p clouddeck-app
```

Core tests use vendored EC2 scenario fixtures and injected services, so they do
not contact AWS. They cover malformed CLI JSON, profile parsing without secret
retention, atomic context persistence, multi-term metadata search, filtering,
load failure transitions, and concurrent fleet timeouts.
