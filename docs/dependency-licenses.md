# Dependency licenses

This page lists the licenses declared by every crate in CloudDeck's resolved
dependency graph, and explains the few that need a note before distributing
CloudDeck under `MIT OR Apache-2.0`.

The inventory comes from the locked dependency graph:

```bash
cargo metadata --format-version 1 --locked \
  | jq -r '[.packages[] | (.license // "MISSING")] | group_by(.) | map("\(.[0])\t\(length)") | .[]'
```

The metadata lists 825 packages, including the two workspace crates and the
vendored `blade-graphics` patch. Each license string is the license field as the
crate declares it in its manifest. Some strings are not strictly valid SPDX
(for example `MIT/Apache-2.0`). The license files themselves were not reviewed
crate by crate.

## Declared license expressions

| License expression | Packages |
|---|---:|
| `Apache-2.0` | 45 |
| `Apache-2.0 / MIT` | 1 |
| `Apache-2.0 AND ISC` | 1 |
| `Apache-2.0 OR BSL-1.0` | 1 |
| `Apache-2.0 OR GPL-2.0-only` | 1 |
| `Apache-2.0 OR ISC OR MIT` | 4 |
| `Apache-2.0 OR MIT` | 80 |
| `Apache-2.0 WITH LLVM-exception` | 1 |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | 7 |
| `Apache-2.0/MIT` | 7 |
| `BSD-2-Clause` | 4 |
| `BSD-2-Clause OR Apache-2.0 OR MIT` | 2 |
| `BSD-3-Clause` | 12 |
| `BSD-3-Clause OR Apache-2.0` | 2 |
| `CC0-1.0` | 4 |
| `CC0-1.0 OR Apache-2.0` | 1 |
| `CC0-1.0 OR MIT-0 OR Apache-2.0` | 1 |
| `CDLA-Permissive-2.0` | 2 |
| `ISC` | 5 |
| `MIT` | 182 |
| `MIT / Apache-2.0` | 4 |
| `MIT OR Apache-2.0` | 356 |
| `MIT OR Apache-2.0 OR BSD-1-Clause` | 1 |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | 2 |
| `MIT OR Apache-2.0 OR Zlib` | 8 |
| `MIT OR Zlib OR Apache-2.0` | 1 |
| `MIT/Apache-2.0` | 39 |
| `MPL-2.0` | 3 |
| `Unicode-3.0` | 18 |
| `Unlicense OR MIT` | 8 |
| `Unlicense/MIT` | 2 |
| `Zlib` | 3 |
| `Zlib OR Apache-2.0 OR MIT` | 9 |
| `0BSD` | 2 |
| `0BSD OR MIT OR Apache-2.0` | 1 |
| `(Apache-2.0 OR MIT) AND BSD-3-Clause` | 1 |
| `(MIT OR Apache-2.0) AND NCSA` | 1 |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 |
| `ISC AND (Apache-2.0 OR ISC)` | 1 |
| `ISC AND (Apache-2.0 OR ISC) AND Apache-2.0 AND MIT AND BSD-3-Clause AND (Apache-2.0 OR ISC OR MIT) AND (Apache-2.0 OR ISC OR MIT-0)` | 1 |

No package is missing a license declaration.

## Crates that need a note

### MPL-2.0 (file-level copyleft)

- `cbindgen` 0.28.0: build-time tooling, not linked into the application.
- `dwrote` 0.11.5: DirectWrite bindings used by GPUI on Windows.
- `option-ext` 0.2.0: home-directory lookup.

MPL-2.0 applies per file. Distributing CloudDeck under `MIT OR Apache-2.0` is
compatible as long as these crates remain unmodified. If one is modified, its
source must be made available under MPL-2.0. CloudDeck does not modify them.

### Alternatives chosen from a dual-license expression

- `r-efi` 5.3.0 and 6.0.0 declare `MIT OR Apache-2.0 OR LGPL-2.1-or-later`.
  CloudDeck takes the MIT or Apache-2.0 option, so no LGPL obligation applies.
- `self_cell` 1.3.0 declares `Apache-2.0 OR GPL-2.0-only`. CloudDeck takes the
  Apache-2.0 option, so no GPL obligation applies.

### Combined (`AND`) expressions

`ring` 0.17.14, `aws-lc-sys` 0.44.0, and `aws-lc-rs` 1.18.0 combine several
permissive licenses (ISC, Apache-2.0, BSD-3-Clause, MIT). Binary distributions
must keep their notices. The other `AND` expressions in the table are also
permissive.

### Data licenses

- `webpki-roots` 0.26.11 and 1.0.11 declare `CDLA-Permissive-2.0`, a permissive
  data license for the bundled root certificate set. Binary distributions must
  keep its attribution.

### Vendored patch

- `blade-graphics` 0.7.1 is patched from `third_party/blade-graphics` and
  declares MIT. The upstream licence text and copyright line (Dzmitry Malyshau,
  2022) ship in `third_party/blade-graphics/LICENSE`; the local changes are
  described in `third_party/README.md`, and the top-level `NOTICE` lists the
  vendored code.

## Conclusion

No dependency declares GPL, AGPL, LGPL-only, SSPL, or a proprietary license
without a permissive alternative. No package is missing a license declaration.
The MPL-2.0 crates are file-scoped copyleft that CloudDeck does not modify. No
follow-up work is needed for distribution under `MIT OR Apache-2.0`.

Rerun the command above whenever `Cargo.lock` changes, and update this page
when a new license expression appears.
