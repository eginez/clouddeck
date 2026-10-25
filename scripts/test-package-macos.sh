#!/usr/bin/env bash
# Run scripts/package-macos.sh in a temp copy of the repo with a fake uname (Darwin)
# and a fake cargo that rejects --manifest-path before the subcommand, as real cargo does.
# Never touches the real dist/.
set -uo pipefail

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
mkdir -p "$R/scripts" "$R/crates/clouddeck-app" "$T/bin"
cp "$SRC/scripts/package-macos.sh" "$R/scripts/"
cp "$SRC/crates/clouddeck-app/Cargo.toml" "$R/crates/clouddeck-app/"
touch "$R/Cargo.toml"

cat >"$T/bin/uname" <<'EOF'
#!/bin/sh
if [ "$1" = "-s" ]; then echo Darwin; else /usr/bin/uname "$@"; fi
EOF
cat >"$T/bin/cargo" <<'EOF'
#!/bin/sh
if [ "$1" != "build" ]; then
    echo "error: unexpected argument '$1' found" >&2
    exit 2
fi
manifest=""
prev=""
for arg in "$@"; do
    [ "$prev" = "--manifest-path" ] && manifest="$arg"
    prev="$arg"
done
[ -n "$manifest" ] || { echo "fake cargo: no --manifest-path" >&2; exit 2; }
root="$(dirname "$manifest")"
mkdir -p "$root/target/release" && cp /bin/true "$root/target/release/clouddeck"
EOF
cat >"$T/bin/hdiutil" <<'EOF'
#!/bin/sh
echo "hdiutil $*" >> "$HDIUTIL_LOG"
out=""
for arg in "$@"; do out="$arg"; done
echo "fake dmg" > "$out"
EOF
chmod +x "$T/bin/uname" "$T/bin/cargo" "$T/bin/hdiutil"

export HDIUTIL_LOG="$T/hdiutil.log"
PATH="$T/bin:$PATH" "$R/scripts/package-macos.sh" >"$T/out.log" 2>&1
check "package-macos.sh succeeds with cargo build --manifest-path first" test $? -eq 0

VERSION="$(awk -F'"' '/^version = / { print $2; exit }' "$R/crates/clouddeck-app/Cargo.toml")"
check "CloudDeck.app binary is installed" test -x "$R/dist/CloudDeck.app/Contents/MacOS/clouddeck"
check "Info.plist carries the crate version" grep -q "<string>$VERSION</string>" "$R/dist/CloudDeck.app/Contents/Info.plist"
check "versioned macOS tarball is produced" test -s "$R/dist/CloudDeck-$VERSION-macos-aarch64.tar.gz"
check "tarball has CloudDeck.app at its root" \
    bash -c "tar -tzf '$R/dist/CloudDeck-$VERSION-macos-aarch64.tar.gz' | grep -qx 'CloudDeck.app/Contents/MacOS/clouddeck'"
check "versioned macOS dmg is produced" test -s "$R/dist/CloudDeck-$VERSION-macos-aarch64.dmg"
check "dmg is built from the CloudDeck.app bundle" grep -q -e "-srcfolder $R/dist/CloudDeck.app" "$HDIUTIL_LOG"

echo "$PASS passed, $FAIL failed"
[[ "$FAIL" -eq 0 ]]
