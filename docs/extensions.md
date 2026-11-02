# Provider extensions: host notes

Extensions are native libraries (`.so` on Linux, `.dylib` on macOS) loaded at runtime from
`~/.config/clouddeck/extensions/<id>/`, next to an `extension.toml`. There is no sandbox: an
extension runs with the user's permissions. Capabilities in the manifest are declarations the
user must also grant; they limit what the host's helpers do, not what the library can do itself.

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
