#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd -- "$script_dir/.." && pwd)"

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
    echo "package-linux.sh must run on Linux x86_64" >&2
    exit 1
fi

# On fleet-vm, run with CARGO_PROFILE_RELEASE_DEBUG=0 and -j2 to keep the build within memory.
cargo build --manifest-path "$root_dir/Cargo.toml" --locked --release -p clouddeck-app

target_dir="${CARGO_TARGET_DIR:-$root_dir/target}"
binary="$target_dir/release/clouddeck"
version="$(awk -F'"' '/^version = / { print $2; exit }' "$root_dir/crates/clouddeck-app/Cargo.toml")"
[[ -n "$version" ]] || { echo "could not read version from Cargo.toml" >&2; exit 1; }

stage_dir="$root_dir/dist/linux-stage"
archive="$root_dir/dist/CloudDeck-$version-linux-x86_64.tar.gz"

rm -rf "$stage_dir"
mkdir -p "$stage_dir"
install -m 0755 "$binary" "$stage_dir/clouddeck"
strip "$stage_dir/clouddeck"
install -m 0644 "$root_dir/packaging/linux/clouddeck.desktop" "$stage_dir/clouddeck.desktop"
install -m 0644 "$root_dir/design/icon/clouddeck-icon-1024.png" "$stage_dir/clouddeck.png"
iconset="$root_dir/design/icon/AppIcon.iconset"
for size in 16 32 128 256 512 1024; do
    case "$size" in
        1024) source="icon_512x512@2x.png" ;;
        *) source="icon_${size}x${size}.png" ;;
    esac
    icon_dir="$stage_dir/share/icons/hicolor/${size}x${size}/apps"
    mkdir -p "$icon_dir"
    install -m 0644 "$iconset/$source" "$icon_dir/clouddeck.png"
done
install -m 0644 "$root_dir/packaging/linux/README.txt" "$stage_dir/README.txt"

tar -czf "$archive" -C "$stage_dir" .

echo "Packaged $archive"
sha256sum "$archive"
