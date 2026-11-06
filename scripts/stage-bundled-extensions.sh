#!/usr/bin/env bash
# Builds the first-party AWS extension and lays it out as a bundled extension folder:
#   <target>/<profile>/extensions-bundled/aws/{extension.toml,libaws.<so|dylib>}
# The app discovers that folder next to its executable at launch.
set -euo pipefail

profile="${1:-debug}"
case "$profile" in
    debug) profile_flag=() ;;
    release) profile_flag=(--release) ;;
    *) echo "usage: $0 [debug|release]" >&2; exit 2 ;;
esac

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd -- "$script_dir/.." && pwd)"
target_dir="${CARGO_TARGET_DIR:-$root_dir/target}"

case "$(uname -s)" in
    Darwin) library_ext="dylib" ;;
    *) library_ext="so" ;;
esac

cargo build --manifest-path "$root_dir/Cargo.toml" --locked --offline ${profile_flag[@]+"${profile_flag[@]}"} -p clouddeck-aws

dest="$target_dir/$profile/extensions-bundled/aws"
rm -rf "$dest"
install -d -m 0755 "$dest"
install -m 0644 "$root_dir/extensions/aws/extension.toml" "$dest/extension.toml"
install -m 0755 "$target_dir/$profile/libclouddeck_aws.$library_ext" "$dest/libaws.$library_ext"

echo "Staged $dest"
