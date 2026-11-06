# clouddeck-aws

The Amazon Web Services provider for [CloudDeck](https://github.com/eginez/clouddeck): EC2 instance listing
through the AWS SDK, IAM Identity Center sign-in, and caller identity checks.

- Built as a cdylib and loaded by the app as a bundled extension. The app does not link this crate.
- `scripts/stage-bundled-extensions.sh [debug|release]` builds it and lays out
  `<target>/<profile>/extensions-bundled/aws/` with `extension.toml` and `libaws.so` (`libaws.dylib` on macOS).
  The app discovers that folder next to its executable; on macOS it lives in `Contents/PlugIns`.
- The crate depends on `clouddeck-provider-kit` for shared listing and profile code, not on `clouddeck-core`
  (core is a dev-dependency for the integration tests only).

## Test

```sh
cargo test -p clouddeck-aws
```

Tests are offline. Pagination, error classification and fleet behaviour run against scripted page sources.
Integration tests in `tests/` cover the built-in list and the demo fallback.
