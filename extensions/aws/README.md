# clouddeck-aws

The Amazon Web Services provider for [CloudDeck](https://github.com/eginez/clouddeck): EC2 instance listing
through the AWS SDK, IAM Identity Center sign-in, and caller identity checks.

- Statically linked into the app for now. `extension.toml` describes the provider for the future bundled
  loader, which does not load it yet.
- The crate depends on `clouddeck-core` for shared listing and profile code. Core does not depend on this crate;
  the app registers it through `builtin_providers` or `aws_provider`.

## Test

```sh
cargo test -p clouddeck-aws
```

Tests are offline. Pagination, error classification and fleet behaviour run against scripted page sources.
Integration tests in `tests/` cover the built-in list and the demo fallback.
