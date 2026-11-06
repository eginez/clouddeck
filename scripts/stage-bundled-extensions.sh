#!/usr/bin/env bash
# Builds the first-party extensions and lays each out as a bundled extension folder:
#   <target>/<profile>/extensions-bundled/<id>/{extension.toml,lib<id>.<so|dylib>}
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
first_party=(aws gcp)

case "$(uname -s)" in
    Darwin) library_ext="dylib" ;;
    *) library_ext="so" ;;
esac

packages=()
for id in "${first_party[@]}"; do packages+=(-p "clouddeck-$id"); done
cargo build --manifest-path "$root_dir/Cargo.toml" --locked --offline ${profile_flag[@]+"${profile_flag[@]}"} "${packages[@]}"

bundled="$target_dir/$profile/extensions-bundled"
rm -rf "$bundled"
for id in "${first_party[@]}"; do
    dest="$bundled/$id"
    install -d -m 0755 "$dest"
    install -m 0644 "$root_dir/extensions/$id/extension.toml" "$dest/extension.toml"
    library="$dest/lib$id.$library_ext"
    install -m 0755 "$target_dir/$profile/libclouddeck_$id.$library_ext" "$library"
    if [[ "$library_ext" == dylib ]]; then
        codesign --force --sign - "$library"
    fi
    echo "Staged $dest"
done
