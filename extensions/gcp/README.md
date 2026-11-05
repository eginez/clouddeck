# clouddeck-gcp

A Google Cloud (Compute Engine) provider extension for [CloudDeck](https://github.com/eginez/clouddeck).
It lists VM instances across the projects your Application Default Credentials can see.

- Connections are projects (Cloud Resource Manager v1, ACTIVE only).
- One dimension, `zone`, is a tree: regions are roots, zones are their children. Selecting a region selects its zones.
- Instances come from Compute `aggregatedList`. A scope that reports a warning (other than the benign
  `NO_RESULTS_ON_PAGE`) becomes a per-zone failure instead of failing the whole listing.
- Instance ids are `zone/name`, because GCP names are unique only per zone.

Status: local-only first cut, API version 2. Not published.

## Build

```sh
cargo build --release      # target/release/libclouddeck_gcp.so (.dylib on macOS)
cargo test                 # offline: recorded JSON fixtures + the API conformance kit
```

The extension API is a path dependency on a sibling `../clouddeck` checkout. See `Cargo.toml` for the
git line to switch to once the `extension-api-v2` tag exists. The crate is built with `panic = "unwind"`
as the host requires.

## Install for development

Put the built library next to `extension.toml` in a folder, then:

```sh
mkdir -p dist/gcp && cp extension.toml dist/gcp/ && cp target/release/libclouddeck_gcp.so dist/gcp/
clouddeck extension dev dist/gcp
```

Installed extensions start disabled; enabling external extensions is gated in current CloudDeck builds.

## Credentials

Run `gcloud auth application-default login` first. The extension reads `$GOOGLE_APPLICATION_CREDENTIALS`,
or `~/.config/gcloud/application_default_credentials.json`, and exchanges the refresh token at
`https://oauth2.googleapis.com/token`. Access tokens are cached in memory only; nothing is written to disk.
Only credentials of type `authorized_user` work today. The Cloud Resource Manager and Compute APIs must be
enabled for the quota project in your ADC file.

## Not done

Browser sign-in (needs a registered OAuth client), service accounts, metadata-server credentials, and kinds other
than instances. See `docs/TODO.md`.
