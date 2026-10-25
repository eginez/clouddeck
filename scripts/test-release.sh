#!/usr/bin/env bash
# Tests scripts/release.sh against fake cargo, gh, ssh, scp and packagers in temp git repos.
# Never touches the real repo, its origin, or GitHub.
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

export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.com
export GIT_COMMITTER_NAME=test GIT_COMMITTER_EMAIL=test@example.com
export FAKE_LOG="$T/log"
mkdir -p "$T/bin" "$FAKE_LOG"

cat >"$T/bin/cargo" <<'EOF'
#!/bin/sh
case "$1" in
    run)
        out=""
        prev=""
        for arg in "$@"; do
            [ "$prev" = "--out" ] && out="$arg"
            prev="$arg"
        done
        echo '{"fake":true}' > "$out/manifest.json"
        echo 'fake-signature' > "$out/manifest.json.sig"
        ;;
    build|test) echo "cargo $*" >> "$FAKE_LOG/cargo.log" ;;
    *) exit 2 ;;
esac
EOF
cat >"$T/bin/gh" <<'EOF'
#!/bin/sh
echo "$*" >> "$FAKE_LOG/gh.log"
if [ "$1" = release ] && [ "$2" = create ]; then
    for arg in "$@"; do
        case "$arg" in
            */dist/*) basename "$arg" >> "$FAKE_LOG/gh-assets.log" ;;
        esac
    done
    echo "https://example.test/releases/tag/$3"
fi
EOF
cat >"$T/bin/ssh" <<'EOF'
#!/bin/sh
echo "ssh $*" >> "$FAKE_LOG/ssh.log"
EOF
cat >"$T/bin/scp" <<'EOF'
#!/bin/sh
echo "scp $*" >> "$FAKE_LOG/scp.log"
echo "remote tarball" > "$2/$(basename "$1")"
EOF
cat >"$T/bin/pkg-macos" <<'EOF'
#!/bin/sh
set -eu
v="$(awk -F'"' '/^version = / { print $2; exit }' crates/clouddeck-app/Cargo.toml)"
mkdir -p dist
echo "mac tarball $v" > "dist/CloudDeck-$v-macos-aarch64.tar.gz"
echo "mac dmg $v" > "dist/CloudDeck-$v-macos-aarch64.dmg"
EOF
cat >"$T/bin/pkg-linux" <<'EOF'
#!/bin/sh
set -eu
v="$(awk -F'"' '/^version = / { print $2; exit }' crates/clouddeck-app/Cargo.toml)"
mkdir -p dist
echo "linux tarball $v" > "dist/CloudDeck-$v-linux-x86_64.tar.gz"
EOF
chmod +x "$T/bin"/*
echo "fake-key" > "$T/key"
echo "notes" > "$T/notes.md"

# shellcheck source=/dev/null
source "$SRC/scripts/release.sh"
set +e

fixture() { # name; prints a work repo whose origin is a bare repo beside it
    local base="$T/$1"
    rm -rf "$base"
    mkdir -p "$base/repo/crates/clouddeck-app" "$base/repo/crates/clouddeck-core"
    git init -q --bare -b main "$base/origin.git"
    git init -q -b main "$base/repo"
    printf '[package]\nname = "clouddeck-app"\nversion = "0.1.0"\n' > "$base/repo/crates/clouddeck-app/Cargo.toml"
    printf '[package]\nname = "clouddeck-core"\nversion = "0.1.0"\n' > "$base/repo/crates/clouddeck-core/Cargo.toml"
    echo "fixture" > "$base/repo/README.md"
    echo "# lock" > "$base/repo/Cargo.lock"
    git -C "$base/repo" add -A
    git -C "$base/repo" commit -q -m init
    git -C "$base/repo" remote add origin "$base/origin.git"
    git -C "$base/repo" push -q origin main
    echo "$base/repo"
}

release_in() { # repo args...; PKG_LINUX overrides the Linux packager (empty means the fleet path)
    local repo="$1"; shift
    rm -f "$FAKE_LOG"/*.log
    RELEASE_REPO_ROOT="$repo" RELEASE_CARGO="${CARGO_BIN-$T/bin/cargo}" RELEASE_GH="$T/bin/gh" \
        RELEASE_SSH="$T/bin/ssh" RELEASE_SCP="$T/bin/scp" RELEASE_PACKAGE_MACOS="$T/bin/pkg-macos" \
        RELEASE_PACKAGE_LINUX="${PKG_LINUX-$T/bin/pkg-linux}" RELEASE_KEY="$T/key" \
        RELEASE_SKIP_PLATFORM_CHECK=1 "$SRC/scripts/release.sh" "$@" >"$T/out.log" 2>&1
}

# helper functions
check "is_semver accepts 0.2.0" is_semver 0.2.0
check "is_semver accepts 1.10.3" is_semver 1.10.3
check "is_semver rejects 0.2" bash -c "! ( source '$SRC/scripts/release.sh'; is_semver 0.2 )"
check "is_semver rejects v0.2.0" bash -c "! ( source '$SRC/scripts/release.sh'; is_semver v0.2.0 )"
check "is_semver rejects prerelease 0.2.0-rc1" bash -c "! ( source '$SRC/scripts/release.sh'; is_semver 0.2.0-rc1 )"
check "is_semver rejects leading zero 01.2.3" bash -c "! ( source '$SRC/scripts/release.sh'; is_semver 01.2.3 )"
check "version_gt 0.2.0 over 0.1.9" version_gt 0.2.0 0.1.9
check "version_gt 0.10.0 over 0.9.0" version_gt 0.10.0 0.9.0
check "version_gt refuses equal 0.1.0" bash -c "! ( source '$SRC/scripts/release.sh'; version_gt 0.1.0 0.1.0 )"
check "version_gt refuses 0.0.9 under 0.1.0" bash -c "! ( source '$SRC/scripts/release.sh'; version_gt 0.0.9 0.1.0 )"

BUMP="$T/bump"
mkdir -p "$BUMP"
printf '[package]\nversion = "0.1.0"\n\n[dependencies]\nserde = { version = "1" }\n' > "$BUMP/Cargo.toml"
set_cargo_version "$BUMP/Cargo.toml" 0.2.0
check "set_cargo_version rewrites the package version" grep -qx 'version = "0.2.0"' "$BUMP/Cargo.toml"
check "set_cargo_version leaves dependency versions alone" grep -qx 'serde = { version = "1" }' "$BUMP/Cargo.toml"

# preflight refusals
R="$(fixture refuse-semver)"
release_in "$R" 0.2 "$T/notes.md"; rc=$?
check "invalid semver is refused" bash -c "test $rc -ne 0 && grep -q semver '$T/out.log'"

R="$(fixture refuse-stale)"
git -C "$R" tag v0.2.0 && git -C "$R" push -q origin v0.2.0
release_in "$R" 0.2.0 "$T/notes.md"; rc=$?
check "version equal to latest tag is refused" test "$rc" -ne 0
release_in "$R" 0.1.5 "$T/notes.md"; rc=$?
check "version below latest tag is refused" test "$rc" -ne 0

R="$(fixture refuse-dirty)"
echo x > "$R/dirty.txt"
release_in "$R" 0.2.0 "$T/notes.md"; rc=$?
check "dirty tree is refused" test "$rc" -ne 0

R="$(fixture refuse-branch)"
git -C "$R" checkout -q -b feature
release_in "$R" 0.2.0 "$T/notes.md"; rc=$?
check "wrong branch is refused" test "$rc" -ne 0

R="$(fixture refuse-unpushed)"
git -C "$R" commit -q --allow-empty -m local
release_in "$R" 0.2.0 "$T/notes.md"; rc=$?
check "main ahead of origin/main is refused" test "$rc" -ne 0

# dry-run leaves origin untouched and never publishes
R="$(fixture dry-run)"
before="$(git -C "$T/dry-run/origin.git" rev-parse main)"
release_in "$R" 0.2.0 "$T/notes.md" --dry-run; rc=$?
check "dry-run exits 0" test "$rc" -eq 0
check "dry-run leaves origin main unchanged" test "$before" = "$(git -C "$T/dry-run/origin.git" rev-parse main)"
check "dry-run creates no tag on origin" test -z "$(git -C "$T/dry-run/origin.git" tag -l v0.2.0)"
check "dry-run never calls gh release create" bash -c "! grep -q 'release create' '$FAKE_LOG/gh.log' 2>/dev/null"
check "dry-run leaves the local checkout at 0.1.0" grep -qx 'version = "0.1.0"' "$R/crates/clouddeck-app/Cargo.toml"

R="$(fixture dry-run-no-linux)"
PKG_LINUX='' release_in "$R" 0.2.0 "$T/notes.md" --dry-run; rc=$?
check "dry-run without a Linux packager exits 0" test "$rc" -eq 0
check "dry-run without a Linux packager never calls gh release create" bash -c "! grep -q 'release create' '$FAKE_LOG/gh.log' 2>/dev/null"

# full run against temp origin with fakes
R="$(fixture full-run)"
release_in "$R" 0.2.0 "$T/notes.md"; rc=$?
check "full run exits 0" test "$rc" -eq 0
check "full run bumps clouddeck-app to 0.2.0" grep -qx 'version = "0.2.0"' "$R/crates/clouddeck-app/Cargo.toml"
check "full run bumps clouddeck-core to 0.2.0" grep -qx 'version = "0.2.0"' "$R/crates/clouddeck-core/Cargo.toml"
check "full run pushes the release commit to origin main" test "$(git -C "$R" rev-parse main)" = "$(git -C "$T/full-run/origin.git" rev-parse main)"
check "full run pushes tag v0.2.0 to origin" test -n "$(git -C "$T/full-run/origin.git" tag -l v0.2.0)"
check "gh release create gets the five assets in order" bash -c "diff <(cat '$FAKE_LOG/gh-assets.log') <(printf '%s\n' \
    CloudDeck-0.2.0-macos-aarch64.tar.gz CloudDeck-0.2.0-macos-aarch64.dmg CloudDeck-0.2.0-linux-x86_64.tar.gz manifest.json manifest.json.sig) >/dev/null"
check "gh release create targets tag v0.2.0" grep -q '^release create v0.2.0 ' "$FAKE_LOG/gh.log"

R="$(fixture fleet-path)"
PKG_LINUX='' release_in "$R" 0.2.0 "$T/notes.md"; rc=$?
check "fleet path full run exits 0" test "$rc" -eq 0
check "fleet path checks out the pushed tag on fleet-vm" grep -q 'git checkout -q v0.2.0' "$FAKE_LOG/ssh.log"
check "fleet path builds Linux with package-linux.sh" grep -q 'scripts/package-linux.sh' "$FAKE_LOG/ssh.log"
check "fleet path copies the Linux tarball back from fleet-vm" \
    grep -q 'fleet-vm.example.invalid:~/src/release/dist/CloudDeck-0.2.0-linux-x86_64.tar.gz' "$FAKE_LOG/scp.log"
check "fleet path publishes the five assets in order" bash -c "diff '$FAKE_LOG/gh-assets.log' <(printf '%s\n' \
    CloudDeck-0.2.0-macos-aarch64.tar.gz CloudDeck-0.2.0-macos-aarch64.dmg CloudDeck-0.2.0-linux-x86_64.tar.gz manifest.json manifest.json.sig) >/dev/null"

R="$(fixture failed-tests)"
cat >"$T/bin/cargo-fail" <<'EOF'
#!/bin/sh
[ "$1" = test ] && exit 1
exit 0
EOF
chmod +x "$T/bin/cargo-fail"
CARGO_BIN="$T/bin/cargo-fail" release_in "$R" 0.2.0 "$T/notes.md"; rc=$?
check "failed tests abort the release" test "$rc" -ne 0
check "failed tests leave the checkout clean" test -z "$(git -C "$R" status --porcelain)"
check "failed tests leave no tag on origin" test -z "$(git -C "$T/failed-tests/origin.git" tag -l v0.2.0)"

echo "$PASS passed, $FAIL failed"
[[ "$FAIL" -eq 0 ]]
