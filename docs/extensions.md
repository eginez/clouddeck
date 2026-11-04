# Provider extensions: host notes

Extensions are native libraries (`.so` on Linux, `.dylib` on macOS) loaded at runtime from
`~/.config/clouddeck/extensions/<id>/`, next to an `extension.toml`. There is no sandbox: an
extension runs with the user's permissions. Capabilities in the manifest are declarations the
user must also grant; they limit what the host's helpers do, not what the library can do itself.

## Writing an extension

Extension crates depend on `clouddeck-extension-api` from git, pinned to the tag for the API version
you build against:

```toml
[dependencies]
clouddeck-extension-api = { git = "https://github.com/eginez/clouddeck", tag = "extension-api-v1" }
```

- There is one tag per API version: `extension-api-v<N>`, where `<N>` is `API_VERSION` in
  `crates/clouddeck-extension-api/src/lib.rs`. `scripts/extension-api-tag.sh` prints the expected tag,
  and given a tag it fails unless the tag matches.
- The API is not published to crates.io. Only the latest API version is supported, so an extension
  built against an older tag is refused with a rebuild message.
- Build the crate as a `cdylib` with `panic = "unwind"`. A library that reports `panic=abort` is
  refused at load.

## Loading

- Discovery reads `extension.toml` and the host state file only. Host state (enabled, grants,
  crash count) lives in `~/.config/clouddeck/extension-state/<id>.json`, outside the extension
  folder; a `state.json` shipped inside an extension folder is never read. No library is loaded
  until a provider is first used.
- An extension whose `api_version` differs from CloudDeck's is refused with a "Rebuild <id> for
  CloudDeck <version>" message.
- A library that reports `panic=abort` is refused. Panics must unwind so they can be turned into
  provider errors at the boundary.
- Updates and removals take effect on restart. Libraries are never unloaded.
- Crash-loop guard: a library that is loaded is recorded as a pending launch until the host calls
  `ProviderRegistry::shutdown` on a clean exit. A session that ends without that call (a crash, a
  force-quit, a kill) counts as a crash against every extension loaded in it. After two such
  sessions in a row the extension is disabled. The app quit path must call
  `ProviderRegistry::shutdown` once it adopts the registry.

## Managing extensions

`clouddeck extension <command>` manages folders under `~/.config/clouddeck/extensions/`:

- `install <id>` installs an extension listed in the signed extension index. It needs the index.
- `install <https://github.com/owner/repo> [--trust-key <base64>]` installs from a GitHub
  repository. A repository that is not in the index needs `--trust-key` with its author's public
  key. If the index cannot be fetched (for example a 404), `--trust-key` is the only way to install
  from a repository; without it the install is refused.
- `update <id>` installs a newer release from the recorded repository. Nothing updates on its own.
  If the index cannot be fetched, the update is checked against the author key pinned at install.
- `dev <path>` installs a local folder as a development extension. It is copied, not linked.
- `list`, `enable <id>`, `disable <id>`, `remove <id>`.

Install and update fetch the platform asset of the latest GitHub release, check its sha256, and
verify the ed25519 signature of `manifest.json` against the author's key before anything is
unpacked. The old folder is renamed aside before the new one is renamed into place, and restored if
that second rename fails. A `state.json` in the archive is ignored.

- Author keys are pinned per extension id on first install. A release signed by a different key
  is refused. Rerun with `--accept-new-key <base64>` only after you have checked the change; that
  disables the extension until you enable it again.
- Installed extensions start disabled. `enable` is refused in this build: enabling external
  extensions stays off until security review B (clouddeck-rs-7ah1) closes.
- `remove` deletes the folder and keeps the author key pin, so a reinstall cannot silently change
  keys. Enable, disable, install, update and remove take effect on restart.
- To reset the pin on purpose, run `remove <id>`, then delete
  `~/.config/clouddeck/extension-state/<id>.json`. The next install pins the author key you offer
  (from the index or `--trust-key`), so check who now publishes the extension first. Deleting the
  file also clears the enabled and dev flags, granted capabilities and crash counts.
- Install, update, dev and remove hold a lock at `~/.config/clouddeck/extension-state/.install.lock`.
  A second one started while the first runs is refused. If no operation is running, a leftover
  lock file is harmless.
- Extensions are not sandboxed. Install only from authors you trust.

## Resource ids and state

`describe(connection, kind, id)` receives only the connection, the kind and a `ResourceId`. It has no
zone, region or other scope. If a cloud's names are unique only within a parent (GCP names are unique
per zone), encode the parent in the id, for example `zone/name` (`us-east1-b/web-1`).

- `Instance.id` must equal the `ResourceId` that `describe` receives for that resource. `describe`
  must accept every id that `list` returns.
- Ids must be unique within a connection. Use the parent-qualified form whenever the bare name
  is not.
- For an id it does not know, `describe` returns an error of kind `NotFound` or `Unsupported`.

`StateKind` maps a provider's states as follows:

- `Terminated` means gone for good: it cannot be started again.
- A state the provider calls "terminated" but that can be started again maps to `Stopped`. GCP's
  `TERMINATED` is an example: it is a stopped VM, so it maps to `Stopped`, with `raw` set to
  `TERMINATED`.
- Keep the provider's own text in `State.raw`, whichever kind it maps to.

## Selections and dimension trees

A dimension value with `parent: None` is a root. A value with `parent: Some(p)` is a child of `p`.
Values form a forest: parents must exist and there must be no cycles.

- Selecting a parent selects all of its descendants. `region = [us]` lists everything under `us`,
  including `us-east` when that is a child of `us`.
- Selecting a child alone lists only that child's resources. Its parent is implied for identity and
  labelling, but it does not widen the listing.
- Selecting a parent together with some of its children lists the same resources as selecting the
  parent alone.
- `list()` works at the finest level. The provider expands a selection into finest-level cells
  internally, and no resource is listed under two finest-level values.
- `CellFailure.cell` names the finest-level values that failed. Every value it names must be covered
  by the selection: either selected itself or a descendant of a selected value.

The conformance kit checks these rules for every hierarchical dimension and every kind: parents exist,
there are no cycles, finest-level leaves list disjoint resources, a parent lists exactly the union of its children,
a parent with its children equals the parent alone, and failure cells are covered by the selection and
are not parents. Which resource belongs to which value is the provider's business, so the kit cannot
check that a child's resources are the right ones, only that they are consistent with the tree.

## Deadlines

Each provider call runs on its own thread. When the deadline passes the call returns a timeout
and the extension's `cancelled()` answers true; the library itself cannot be interrupted.

## macOS

- Libraries that CloudDeck's own downloader fetches must not carry the quarantine attribute,
  or Gatekeeper blocks them when loaded.
- Apple silicon requires at least an ad-hoc code signature on the library.
- If CloudDeck is notarized with the hardened runtime, it needs the
  `com.apple.security.cs.disable-library-validation` entitlement to load third-party libraries.
- The loader has been checked on Linux only. Verify on macOS arm64 before relying on it. Run the
  extension-host-macos workflow (workflow_dispatch) to check.
