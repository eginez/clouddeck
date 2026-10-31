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
export FAKE_GH_DRAFTS="$T/draft"
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
if [ "$1" = release ] && [ "$2" = upload ] && [ -n "${FAKE_GH_FAIL_UPLOAD:-}" ]; then
    exit 1
fi
if [ "$1" = release ] && [ "$2" = edit ] && [ -n "${FAKE_GH_FAIL_EDIT:-}" ]; then
    exit 1
fi
if [ "$1" = release ] && [ "$2" = view ]; then
    case "$*" in
        *isDraft*) echo "${FAKE_GH_DRAFT:-true}" ;;
        *assets*) ls -1 "$FAKE_GH_DRAFTS" ;;
    esac
fi
if [ "$1" = release ] && [ "$2" = download ]; then
    dir=""
    prev=""
    for arg in "$@"; do
        [ "$prev" = --dir ] && dir="$arg"
        prev="$arg"
    done
    cp "$FAKE_GH_DRAFTS"/* "$dir"/
fi
EOF
cat >"$T/bin/ssh" <<'EOF'
#!/bin/sh
echo "ssh $*" >> "$FAKE_LOG/ssh.log"
case "$2" in
    *"git clone"*) ;;
    *) exit 0 ;;
esac
if command -v sha256sum >/dev/null 2>&1; then
    echo "$(sha256sum "$FAKE_LOG/remote-tarball" | cut -d' ' -f1)"
else
    echo "$(shasum -a 256 "$FAKE_LOG/remote-tarball" | cut -d' ' -f1)"
fi
if [ -n "${FAKE_SSH_HEAD:-}" ]; then
    echo "$FAKE_SSH_HEAD"
    exit 0
fi
url="$(printf '%s' "$2" | sed -n "s/.*git clone -q --template= '\([^']*\)'.*/\1/p")"
tag="$(printf '%s' "$2" | sed -n 's/.*git checkout -q v\([0-9.]*\).*/\1/p')"
git --git-dir="$url" rev-parse "v$tag^{commit}"
EOF
cat >"$T/bin/scp" <<'EOF'
#!/bin/sh
echo "scp $*" >> "$FAKE_LOG/scp.log"
if [ -n "${FAKE_SCP_TAMPER:-}" ]; then
    echo "substituted tarball" > "$2/$(basename "$1")"
else
    cp "$FAKE_LOG/remote-tarball" "$2/$(basename "$1")"
fi
EOF
echo "remote tarball" > "$FAKE_LOG/remote-tarball"
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

release_in() { # repo args...; PKG_LINUX overrides the Linux packager (empty means the fleet path); BUILD_HOST overrides the build host
    local repo="$1"; shift
    rm -f "$FAKE_LOG"/*.log
    RELEASE_REPO_ROOT="$repo" RELEASE_CARGO="${CARGO_BIN-$T/bin/cargo}" RELEASE_GH="$T/bin/gh" \
        RELEASE_SSH="$T/bin/ssh" RELEASE_SCP="$T/bin/scp" RELEASE_PACKAGE_MACOS="$T/bin/pkg-macos" \
        RELEASE_PACKAGE_LINUX="${PKG_LINUX-$T/bin/pkg-linux}" RELEASE_KEY="${KEY_PATH-$T/key}" \
        RELEASE_LINUX_HOST="${BUILD_HOST-linux-build.example.com}" \
        RELEASE_CI_ATTEMPTS="${CI_ATTEMPTS-1}" RELEASE_CI_INTERVAL=0 \
        RELEASE_SKIP_PLATFORM_CHECK=1 "$SRC/scripts/release.sh" "$@" >"$T/out.log" 2>&1 </dev/null
}

seed_draft() { # version; the draft release CI would have built, one file per asset
    rm -rf "$FAKE_GH_DRAFTS"
    mkdir -p "$FAKE_GH_DRAFTS"
    echo "linux $1" > "$FAKE_GH_DRAFTS/CloudDeck-$1-linux-x86_64.tar.gz"
    echo "mac dmg $1" > "$FAKE_GH_DRAFTS/CloudDeck-$1-macos-aarch64.dmg"
    echo "mac tar $1" > "$FAKE_GH_DRAFTS/CloudDeck-$1-macos-aarch64.tar.gz"
}

ci_fixture() { # name; a repo whose origin has tag v0.2.0 at main and a seeded CI draft
    local R
    R="$(fixture "$1")"
    git -C "$R" tag v0.2.0 && git -C "$R" push -q origin v0.2.0
    seed_draft 0.2.0
    echo "$R"
}

gh_line() { grep -n "$1" "$FAKE_LOG/gh.log" | head -n1 | cut -d: -f1; }

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
check "gh release create pins the product repo" grep -q '^release create v0.2.0 -R eginez/clouddeck ' "$FAKE_LOG/gh.log"

R="$(fixture fleet-path)"
PKG_LINUX='' release_in "$R" 0.2.0 "$T/notes.md"; rc=$?
check "fleet path full run exits 0" test "$rc" -eq 0
check "fleet path checks out the pushed tag on the build host" grep -q 'git checkout -q v0.2.0' "$FAKE_LOG/ssh.log"
check "fleet path builds Linux with package-linux.sh" grep -q 'scripts/package-linux.sh' "$FAKE_LOG/ssh.log"
check "fleet path wipes the reused release clone before cloning" grep -q 'rm -rf ~/src/release "$CARGO_HOME"; git clone' "$FAKE_LOG/ssh.log"
check "fleet path clones without the user's git config or templates" grep -q 'GIT_CONFIG_GLOBAL=/dev/null.*git clone -q --template= ' "$FAKE_LOG/ssh.log"
check "fleet path builds with a clean cargo home" grep -q 'CARGO_HOME="$HOME/src/release-cargo"' "$FAKE_LOG/ssh.log"
check "fleet path clears git config env overrides before cloning" \
    grep -q 'GIT_CONFIG_COUNT=0.*unset GIT_TEMPLATE_DIR GIT_CONFIG_PARAMETERS.*git clone' "$FAKE_LOG/ssh.log"
check "fleet path clears rustc wrapper and flag overrides before building" \
    grep -q 'unset .*RUSTC_WRAPPER.*RUSTFLAGS.*CARGO_BUILD_RUSTC_WRAPPER.*CARGO_ENCODED_RUSTFLAGS.*LINKER' "$FAKE_LOG/ssh.log"
check "fleet path refuses a build under ancestor cargo config" grep -q 'would configure cargo' "$FAKE_LOG/ssh.log"
check "fleet path ancestor guard names .cargo/config.toml" grep -q '\$d/.cargo/config.toml' "$FAKE_LOG/ssh.log"

GUARD="$(bash -c 'source "$1"; printf %s "$CARGO_ANCESTOR_CONFIG_GUARD"' _ "$SRC/scripts/release.sh")"
GUARD_HOME="$T/guard-home"
mkdir -p "$GUARD_HOME/src/release" "$GUARD_HOME/.cargo"
echo '[build]' > "$GUARD_HOME/.cargo/config.toml"
guard_in() { (cd "$GUARD_HOME/src/release" && sh -c "$GUARD") >/dev/null 2>&1; }
if guard_in; then bad "ancestor guard fails under a home cargo config"; else ok "ancestor guard fails under a home cargo config"; fi
rm -f "$GUARD_HOME/.cargo/config.toml"
check "ancestor guard passes once the home cargo config is gone" guard_in
check "fleet path asserts the build commit equals the tag commit" grep -q 'git rev-parse HEAD)" = ' "$FAKE_LOG/ssh.log"
check "fleet path copies the Linux tarball back from the build host" \
    grep -q 'linux-build.example.com:~/src/release/dist/CloudDeck-0.2.0-linux-x86_64.tar.gz' "$FAKE_LOG/scp.log"
check "fleet path publishes the five assets in order" bash -c "diff '$FAKE_LOG/gh-assets.log' <(printf '%s\n' \
    CloudDeck-0.2.0-macos-aarch64.tar.gz CloudDeck-0.2.0-macos-aarch64.dmg CloudDeck-0.2.0-linux-x86_64.tar.gz manifest.json manifest.json.sig) >/dev/null"

R="$(fixture fleet-no-host)"
BUILD_HOST='' PKG_LINUX='' release_in "$R" 0.2.0 "$T/notes.md"; rc=$?
check "fleet path without RELEASE_LINUX_HOST is refused" test "$rc" -ne 0
check "fleet path without RELEASE_LINUX_HOST names the variable" grep -q 'RELEASE_LINUX_HOST' "$T/out.log"
check "fleet path without RELEASE_LINUX_HOST never reaches ssh" test ! -s "$FAKE_LOG/ssh.log"

R="$(fixture fleet-mismatch)"
export FAKE_SSH_HEAD=0000000000000000000000000000000000000000
PKG_LINUX='' release_in "$R" 0.2.0 "$T/notes.md"; rc=$?
unset FAKE_SSH_HEAD
check "fleet build of a different commit is refused" test "$rc" -ne 0
check "fleet build of a different commit names the mismatch" grep -q "built commit '0000" "$T/out.log"
check "fleet build of a different commit never copies a tarball back" test ! -s "$FAKE_LOG/scp.log"
check "fleet build of a different commit never publishes" bash -c "! grep -q 'release create' '$FAKE_LOG/gh.log' 2>/dev/null"

R="$(fixture fleet-tampered-tarball)"
export FAKE_SCP_TAMPER=1
PKG_LINUX='' release_in "$R" 0.2.0 "$T/notes.md"; rc=$?
unset FAKE_SCP_TAMPER
check "tarball substituted in transit is refused" test "$rc" -ne 0
check "substituted tarball names the sha256 mismatch" grep -q "does not match linux-build.example.com" "$T/out.log"
check "substituted tarball is never signed" test ! -e "$R/dist/manifest.json"
check "substituted tarball is never published" bash -c "! grep -q 'release create' '$FAKE_LOG/gh.log' 2>/dev/null"

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

# --from-ci: CI already built the draft; this side downloads, signs, then publishes
no_publish() { ! grep -qE 'release (upload|edit)' "$FAKE_LOG/gh.log" 2>/dev/null; }
upload_before_publish() {
    local up ed
    up="$(gh_line 'release upload')"
    ed="$(gh_line 'release edit')"
    [[ -n "$up" && -n "$ed" && "$up" -lt "$ed" ]]
}

R="$(ci_fixture ci-missing-asset)"
rm "$FAKE_GH_DRAFTS/CloudDeck-0.2.0-linux-x86_64.tar.gz"
release_in "$R" 0.2.0 "$T/notes.md" --from-ci --yes; rc=$?
check "from-ci missing asset is refused" test "$rc" -ne 0
check "from-ci missing asset names the draft mismatch" grep -q 'does not have exactly the CI assets' "$T/out.log"
check "from-ci missing asset never downloads" bash -c "! grep -q 'release download' '$FAKE_LOG/gh.log' 2>/dev/null"
check "from-ci missing asset never uploads or publishes" no_publish

R="$(ci_fixture ci-extra-asset)"
echo x > "$FAKE_GH_DRAFTS/CloudDeck-0.2.0-source.zip"
release_in "$R" 0.2.0 "$T/notes.md" --from-ci --yes; rc=$?
check "from-ci extra asset is refused" test "$rc" -ne 0
check "from-ci extra asset never uploads or publishes" no_publish

R="$(ci_fixture ci-published)"
export FAKE_GH_DRAFT=false
release_in "$R" 0.2.0 "$T/notes.md" --from-ci --yes; rc=$?
unset FAKE_GH_DRAFT
check "from-ci refuses an already published release" test "$rc" -ne 0
check "from-ci already published says so" grep -q 'already published' "$T/out.log"
check "from-ci already published never uploads or publishes" no_publish

R="$(ci_fixture ci-dry-run)"
release_in "$R" 0.2.0 "$T/notes.md" --from-ci --dry-run; rc=$?
check "from-ci dry-run exits 0" test "$rc" -eq 0
check "from-ci dry-run downloads the CI assets" grep -q '^release download v0.2.0' "$FAKE_LOG/gh.log"
check "from-ci dry-run signs the manifest" test -s "$R/dist/manifest.json.sig"
check "from-ci dry-run never uploads or publishes" no_publish

R="$(ci_fixture ci-publish)"
release_in "$R" 0.2.0 "$T/notes.md" --from-ci --yes; rc=$?
check "from-ci real run exits 0" test "$rc" -eq 0
check "from-ci real run uploads manifest and signature" grep -q 'release upload v0.2.0 .*manifest.json .*manifest.json.sig$' "$FAKE_LOG/gh.log"
check "from-ci real run publishes the draft with the notes" grep -q 'release edit v0.2.0 -R eginez/clouddeck --draft=false --notes-file' "$FAKE_LOG/gh.log"
check "from-ci real run uploads before publishing" upload_before_publish

R="$(ci_fixture ci-upload-fails)"
export FAKE_GH_FAIL_UPLOAD=1
release_in "$R" 0.2.0 "$T/notes.md" --from-ci --yes; rc=$?
unset FAKE_GH_FAIL_UPLOAD
check "from-ci failed manifest upload is refused" test "$rc" -ne 0
check "from-ci failed manifest upload never publishes" bash -c "! grep -q 'release edit' '$FAKE_LOG/gh.log' 2>/dev/null"

R="$(ci_fixture ci-edit-fails)"
export FAKE_GH_FAIL_EDIT=1
release_in "$R" 0.2.0 "$T/notes.md" --from-ci --yes; rc=$?
unset FAKE_GH_FAIL_EDIT
check "from-ci failed publish is refused" test "$rc" -ne 0
check "from-ci failed publish reports it" grep -q 'publishing v0.2.0 failed' "$T/out.log"

R="$(ci_fixture ci-no-confirm)"
release_in "$R" 0.2.0 "$T/notes.md" --from-ci; rc=$?
check "from-ci without --yes and no answer is refused" test "$rc" -ne 0
check "from-ci without --yes and no answer says not publishing" grep -q 'not publishing' "$T/out.log"
check "from-ci without --yes and no answer never uploads or publishes" no_publish

R="$(ci_fixture ci-key-private)"
KEY_PATH="$T/no-such-key" release_in "$R" 0.2.0 "$T/notes.md" --from-ci --yes; rc=$?
check "from-ci missing signing key is refused" test "$rc" -ne 0
check "missing signing key error does not print the key path" bash -c "! grep -qF '$T/no-such-key' '$T/out.log'"
release_in "$R" 0.2.0 "$T/notes.md" --from-ci --yes
check "from-ci run never prints the signing key path" bash -c "! grep -qF '$T/key' '$T/out.log'"

R="$(ci_fixture ci-tag-not-latest)"
release_in "$R" 0.2.1 "$T/notes.md" --from-ci --yes; rc=$?
check "from-ci refuses a version with no tag" test "$rc" -ne 0
check "from-ci missing tag names the latest tag" grep -q 'must be the latest tag' "$T/out.log"

R="$(ci_fixture ci-tag-behind-main)"
git -C "$R" commit -q --allow-empty -m later && git -C "$R" push -q origin main
release_in "$R" 0.2.0 "$T/notes.md" --from-ci --yes; rc=$?
check "from-ci refuses a tag behind origin/main" test "$rc" -ne 0
check "from-ci tag behind main names the mismatch" grep -q 'not at origin/main' "$T/out.log"

R="$(fixture local-yes)"
release_in "$R" 0.2.0 "$T/notes.md" --yes; rc=$?
check "--yes without --from-ci is refused" test "$rc" -ne 0

echo "$PASS passed, $FAIL failed"
[[ "$FAIL" -eq 0 ]]
