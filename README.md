# CloudDeck Rust

CloudDeck is a read only EC2 fleet browser. The Rust workspace separates the
Linux testable domain layer (`clouddeck-core`) from the GPUI desktop shell
(`clouddeck-app`). The core owns EC2 models, filtering, profile and region
context, load state, and the best effort fleet aggregator. Live AWS discovery
uses the AWS CLI v2 as a subprocess so the CLI remains responsible for SSO,
credential providers, and pagination; CloudDeck does not store credentials or
perform cloud mutations.

## Quickstart

### Install

Linux (x86_64): extract the archive produced by `scripts/package-linux.sh`
(`dist/CloudDeck-<version>-linux-x86_64.tar.gz`) and run the app:

```bash
tar -xzf CloudDeck-<version>-linux-x86_64.tar.gz
./clouddeck
```

macOS (Apple silicon): `scripts/package-macos.sh` writes
`dist/CloudDeck-<version>-macos-aarch64.dmg` and a matching `.tar.gz` that
contains `CloudDeck.app`. Open the disk image and copy CloudDeck to
Applications.

To try CloudDeck without AWS, start it with `--demo` (see [Run](#run)).

### First launch

Without `--demo`, the first launch with no saved context opens a welcome
window that lists the profiles in `~/.aws/config`. Choose a profile and press
Enter to open it. Set `AWS_CONFIG_FILE` to read a different config file.
CloudDeck remembers the last profile and regions in
`$XDG_CONFIG_HOME/clouddeck/context.json`.

### Connect a profile

Switch profiles with `Switch Profile…` (⌘P; Ctrl on Linux). The profile picker's
`Connect to AWS…` entry opens the Connect screen, where `Rescan` re-reads the
profile list and `Sign in with browser` starts IAM Identity Center (SSO)
sign-in for profiles that support it.

For each selected profile and region, CloudDeck runs a read only
`aws ec2 describe-instances --profile <name> --region <region> --output json`
with page size and timeout flags. The AWS CLI handles credentials, SSO tokens,
and pagination.

### Choose regions

The fleet view loads `us-east-1`, `us-west-2`, `eu-west-1`, and
`eu-central-1` by default. Use `Switch Region…` (⌘⇧R) to choose the regions to
load from the full list. A region that fails keeps the results from the
regions that succeeded. The failure popover offers `Retry region` for one
failed region, or a `Retry` button per region plus `Retry all` for several.

### Search

Type words to search instances, or narrow results with `field:value` filters,
for example `state:running`, `type:t3.*`, or `-state:stopped` to exclude
stopped instances. The supported fields are `name`, `id`, `state`, `type`,
`az`, `region`, `ip`, `private`, `public`, `vpc`, `subnet`, `sg`, `ami`,
`key`, `age`, `has`, and `tag`, as defined in
`crates/clouddeck-core/src/query/tokenize.rs`.

If something does not load, see [docs/troubleshooting.md](docs/troubleshooting.md).

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

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to build, test, and send changes.
Report security issues as described in [SECURITY.md](SECURITY.md). Participation
is governed by the [Code of Conduct](CODE_OF_CONDUCT.md).

## License

CloudDeck is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in CloudDeck by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
