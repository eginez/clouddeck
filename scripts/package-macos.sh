#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd -- "$script_dir/.." && pwd)"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "package-macos.sh must run on macOS" >&2
    exit 1
fi

cargo build --manifest-path "$root_dir/Cargo.toml" --locked --offline --release --bin clouddeck

app_dir="$root_dir/dist/CloudDeck.app"
binary="$root_dir/target/release/clouddeck"
version="$(awk -F'"' '/^version = / { print $2; exit }' "$root_dir/crates/clouddeck-app/Cargo.toml")"

rm -rf "$app_dir"
mkdir -p "$app_dir/Contents/MacOS"
install -m 0755 "$binary" "$app_dir/Contents/MacOS/clouddeck"

cat > "$app_dir/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDisplayName</key>
    <string>CloudDeck</string>
    <key>CFBundleExecutable</key>
    <string>clouddeck</string>
    <key>CFBundleIdentifier</key>
    <string>com.clouddeck.rust.mvp</string>
    <key>CFBundleName</key>
    <string>CloudDeck</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

echo "Packaged $app_dir"
