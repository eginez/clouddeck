#!/usr/bin/env bash
# Prints the release version for a vX.Y.Z tag, failing unless it matches crates/clouddeck-app/Cargo.toml.
# With no tag argument, prints the Cargo.toml version.
set -euo pipefail

root_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_version="$(awk -F'"' '/^version = / { print $2; exit }' "$root_dir/crates/clouddeck-app/Cargo.toml")"
[[ -n "$cargo_version" ]] || { echo "release-version: no version in crates/clouddeck-app/Cargo.toml" >&2; exit 1; }

for manifest in "$root_dir"/crates/*/Cargo.toml; do
    crate_version="$(awk -F'"' '/^version = / { print $2; exit }' "$manifest")"
    [[ "$crate_version" == "$cargo_version" ]] || {
        echo "release-version: $(basename "$(dirname "$manifest")") is $crate_version but clouddeck-app is $cargo_version" >&2
        exit 1
    }
done

tag="${1:-}"
if [[ -z "$tag" ]]; then
    echo "$cargo_version"
    exit 0
fi

[[ "$tag" =~ ^v([0-9]+\.[0-9]+\.[0-9]+)$ ]] || { echo "release-version: tag '$tag' is not vX.Y.Z" >&2; exit 1; }
version="${BASH_REMATCH[1]}"
[[ "$version" == "$cargo_version" ]] || {
    echo "release-version: tag $tag does not match Cargo.toml version $cargo_version" >&2
    exit 1
}
echo "$version"
