#!/usr/bin/env bash
# Run scripts/package-linux.sh in a temp copy of the repo with a fake cargo, then
# check the tarball's icons and desktop entry. Never touches the real dist/.
set -uo pipefail

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
    echo "skip - package-linux.sh targets Linux x86_64 only"
    exit 0
fi

SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
T="$(mktemp -d)"
PASS=0
FAIL=0
trap 'rm -rf "$T"' EXIT

ok() { PASS=$((PASS + 1)); echo "ok   - $1"; }
bad() { FAIL=$((FAIL + 1)); echo "FAIL - $1"; }
check() { # desc cmd...
    local d="$1"; shift
    if "$@"; then ok "$d"; else bad "$d"; fi
}

R="$T/repo"
mkdir -p "$R/scripts" "$R/design/icon" "$R/packaging" "$R/crates/clouddeck-app" "$T/bin"
cp "$SRC/scripts/package-linux.sh" "$R/scripts/"
cp -r "$SRC/design/icon/AppIcon.iconset" "$SRC/design/icon/clouddeck-icon-1024.png" "$R/design/icon/"
cp -r "$SRC/packaging/linux" "$R/packaging/"
cp "$SRC/crates/clouddeck-app/Cargo.toml" "$R/crates/clouddeck-app/"
cat >"$T/bin/cargo" <<'EOF'
#!/bin/sh
mkdir -p "$CARGO_TARGET_DIR/release" && cp /bin/true "$CARGO_TARGET_DIR/release/clouddeck"
EOF
chmod +x "$T/bin/cargo"

PATH="$T/bin:$PATH" CARGO_TARGET_DIR="$T/target" "$R/scripts/package-linux.sh" >"$T/out.log" 2>&1
check "package-linux.sh succeeds" test $? -eq 0

VERSION="$(awk -F'"' '/^version = / { print $2; exit }' "$R/crates/clouddeck-app/Cargo.toml")"
ARCHIVE="$R/dist/CloudDeck-$VERSION-linux-x86_64.tar.gz"
X="$T/extract"
mkdir -p "$X"
tar -xzf "$ARCHIVE" -C "$X"

for size in 16 32 128 256 512 1024; do
    check "hicolor ${size}x${size} icon is in the tarball" \
        test -f "$X/share/icons/hicolor/${size}x${size}/apps/clouddeck.png"
done

check "hicolor 1024 icon matches the 1024px render" \
    cmp -s "$X/share/icons/hicolor/1024x1024/apps/clouddeck.png" "$SRC/design/icon/AppIcon.iconset/icon_512x512@2x.png"
check "hicolor 16 icon matches the iconset source" \
    cmp -s "$X/share/icons/hicolor/16x16/apps/clouddeck.png" "$SRC/design/icon/AppIcon.iconset/icon_16x16.png"

check "clouddeck.desktop is in the tarball" test -f "$X/clouddeck.desktop"
check "clouddeck.desktop has Icon=clouddeck" grep -qx "Icon=clouddeck" "$X/clouddeck.desktop"
check "clouddeck.desktop has StartupWMClass=clouddeck" grep -qx "StartupWMClass=clouddeck" "$X/clouddeck.desktop"

echo "$PASS passed, $FAIL failed"
[[ "$FAIL" -eq 0 ]]
