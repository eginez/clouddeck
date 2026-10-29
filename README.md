# CloudDeck

CloudDeck is a read only browser for your EC2 fleet. It is a desktop app
written in Rust with the GPUI toolkit, for macOS and Linux. It lists instances
across AWS profiles and regions in one searchable table, so you can find a
machine without clicking through the console. It gets its data by running the
AWS CLI, which keeps sign-in, SSO tokens, and pagination in the tool you
already use. CloudDeck never stores credentials and never changes anything in
your cloud.

## Features

- One table for the instances of the selected profile across several regions.
- Search by words or by `field:value` filters, with autocomplete and error hints.
- An inspector pane with the details of the selected instance.
- A failure popover when a region does not load. Regions that succeed keep
  their results, and you can retry one region or all of them.
- A welcome window and a Connect screen that list your AWS profiles and can
  start IAM Identity Center (SSO) sign-in in the browser.
- A command palette, profile and region pickers, and a Settings window.
- Light and dark themes.
- A `--demo` mode that needs no AWS account.
- Only profile and region names are saved between runs.

## Screenshots

Light theme:

![CloudDeck main window, light theme](design/images/01-main-window-light.png)

Dark theme:

![CloudDeck main window, dark theme](design/images/02-main-window-dark.png)

These are renders of the design mockup in `design/`, not captures of the
running app.

## Install

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

To try CloudDeck without AWS, start it with `--demo` (see
[Quick start](#quick-start)).

## Quick start

### Try the demo

Run the demo to explore the app without AWS credentials:

```bash
cargo run -p clouddeck-app -- --demo
```

### Use your own profiles

Without `--demo`, the first launch with no saved context opens a welcome
window that lists the profiles in `~/.aws/config`. Choose a profile and press
Enter to open it. Set `AWS_CONFIG_FILE` to read a different config file.
CloudDeck remembers the last profile and regions in
`$XDG_CONFIG_HOME/clouddeck/context.json` (or `$HOME/.config/clouddeck/context.json`).
The file contains only profile and region names.

Switch profiles with `Switch Profile…` (⌘P; Ctrl on Linux). The profile
picker's `Connect to AWS…` entry opens the Connect screen, where `Rescan`
re-reads the profile list and `Sign in with browser` starts IAM Identity
Center (SSO) sign-in for profiles that support it.

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
A profile's region metadata is used for the initial selection.

If something does not load, see [docs/troubleshooting.md](docs/troubleshooting.md).

## Search syntax

Type words to search instances, or narrow results with `field:value` filters.
Put a `-` in front of a filter to exclude matches.

```text
web                  instances whose metadata matches "web"
state:running        only running instances
type:t3.*            any t3 instance size
-state:stopped       everything except stopped instances
```

The supported fields are `name`, `id`, `state`, `type`, `az`, `region`, `ip`,
`private`, `public`, `vpc`, `subnet`, `sg`, `ami`, `key`, `age`, `has`, and
`tag`. They are defined in `crates/clouddeck-core/src/query/tokenize.rs`.

## Requirements

- AWS CLI v2 on `PATH`, `/opt/homebrew/bin/aws`, `/usr/local/bin/aws`, or
  `/usr/bin/aws` for live discovery.
- An AWS profile configured in `~/.aws/config` and `~/.aws/credentials` (or
  the paths selected by `AWS_CONFIG_FILE` and `AWS_SHARED_CREDENTIALS_FILE`).
  SSO profiles must be logged in with `aws sso login --profile <name>` before
  refreshing.
- Read only permission for `ec2:DescribeInstances` in each selected region.
- On Linux, a Vulkan driver, libxkbcommon, fontconfig, and a Wayland or X11
  session (see `packaging/linux/README.txt`).
- To build from source: Rust stable. The repository's `Cargo.lock` may require
  a recent stable compiler.

## Build from source and test

```bash
cargo build -p clouddeck-app
cargo run -p clouddeck-app            # live AWS profiles
cargo run -p clouddeck-app -- --demo  # no AWS needed
```

The fast, credential free core gate runs on Linux and locally:

```bash
cargo test -p clouddeck-core
cargo build -p clouddeck-core
```

The GPUI shell has its own Linux compile and test check when the host has the
desktop libraries GPUI needs:

```bash
cargo check -p clouddeck-app
cargo test -p clouddeck-app
```

Core tests use vendored EC2 scenario fixtures and injected services, so they do
not contact AWS. They cover malformed CLI JSON, profile parsing without secret
retention, atomic context persistence, multi-term metadata search, filtering,
load failure transitions, and concurrent fleet timeouts.

## Project layout

- `crates/clouddeck-core`: the platform independent domain layer. It owns EC2
  models, filtering, profile and region context, load state, the best effort
  fleet aggregator, and the AWS CLI adapter. It builds and tests without a
  display.
- `crates/clouddeck-app`: the GPUI desktop shell, built as the `clouddeck`
  binary.
- `packaging/`: files for the Linux archive.
- `scripts/`: packaging and release scripts.
- `design/`: the UI mockup and its rendered images.
- `third_party/`: vendored dependencies (see its README).
- `docs/`: troubleshooting and dependency license notes.

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
