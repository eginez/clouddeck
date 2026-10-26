# Vendored third-party code

## blade-graphics

- Upstream: https://github.com/kvark/blade (crate path `blade-graphics`)
- Version: 0.7.1 (crates.io), upstream commit 88b6c64cd7a32dd933acc4e5266808a07143ce84
- Licence: MIT, Copyright (c) 2022 Dzmitry Malyshau (see `blade-graphics/LICENSE`)
- Wired in through `[patch.crates-io]` in the workspace `Cargo.toml`.

### Local changes

1. compositeAlpha fallback (clouddeck-rs-sr7i.1, RR1 spike), in
   `blade-graphics/src/vulkan/surface.rs`. Upstream 0.7.1 hard-codes
   `VK_COMPOSITE_ALPHA_OPAQUE`, which Lavapipe lacks, giving a black window
   under Xvfb. The vendored copy picks from the surface's supported composite
   alpha modes and falls back to PRE_MULTIPLIED / INHERIT.
2. Clippy lint cleanups (bd-clouddeck-rs-o1vq), no behaviour change: elided
   lifetimes made explicit (`'_`) and `u64::max_value()` replaced by
   `u64::MAX`, in `src/vulkan/{command,init,mod,pipeline}.rs`.

Why vendored: no newer 0.7.x exists and 0.9 is not compatible with gpui 0.2.2.
Drop the vendored copy once an upstream release carries the fix.
