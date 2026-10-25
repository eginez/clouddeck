#!/usr/bin/env bash
# Cuts a CloudDeck release: test, bump, tag, build both platforms, sign the manifest, publish.
# Usage: scripts/release.sh <version> <notes-file> [--dry-run]
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${RELEASE_REPO_ROOT:-$(cd -- "$SCRIPT_DIR/.." && pwd)}"
RELEASE_CARGO="${RELEASE_CARGO:-cargo}"
RELEASE_GH="${RELEASE_GH:-gh}"
RELEASE_SSH="${RELEASE_SSH:-ssh}"
RELEASE_SCP="${RELEASE_SCP:-scp}"
RELEASE_GIT="${RELEASE_GIT:-git}"
RELEASE_PACKAGE_MACOS="${RELEASE_PACKAGE_MACOS:-scripts/package-macos.sh}"
RELEASE_KEY="${RELEASE_KEY:-$HOME/.config/clouddeck/release-signing.key}"
FLEET_HOST="fleet-vm.example.invalid"
# shellcheck disable=SC2088 # tilde expands on the fleet host, not here
REMOTE_RELEASE_DIR='~/src/release'
SEMVER_RE='^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'

die() { echo "release: $*" >&2; exit 1; }

is_semver() { [[ "$1" =~ $SEMVER_RE ]]; }

version_gt() { [[ "$1" != "$2" && "$(printf '%s\n' "$1" "$2" | sort -V | tail -n1)" == "$1" ]]; }

latest_tag_version() { # repo dir; prints the highest vX.Y.Z tag without the v, or nothing
    git -C "$1" tag -l 'v[0-9]*.[0-9]*.[0-9]*' | sed 's/^v//' | grep -E "$SEMVER_RE" | sort -V | tail -n1 || true
}

set_cargo_version() { # Cargo.toml version
    local tmp
    tmp="$(mktemp)"
    awk -v v="$2" '!done && /^version = / { print "version = \"" v "\""; done = 1; next } { print }' "$1" > "$tmp"
    cat "$tmp" > "$1"
    rm -f "$tmp"
    grep -qx "version = \"$2\"" "$1" || die "could not set version $2 in $1"
}

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | cut -d' ' -f1
    fi
}

summary() {
    local f
    for f in "$@"; do
        if [[ -s "$f" ]]; then
            printf '  %s  %s bytes  sha256 %s\n' "$(basename "$f")" "$(wc -c < "$f" | tr -d ' ')" "$(sha256_of "$f")"
        else
            printf '  %s  (not built)\n' "$(basename "$f")"
        fi
    done
}

require_file() {
    local f
    for f in "$@"; do
        [[ -s "$f" ]] || die "missing release asset: $f"
    done
}

preflight() {
    local version="$1" branch latest
    if [[ "${RELEASE_SKIP_PLATFORM_CHECK:-0}" != 1 ]]; then
        [[ "$(uname -s)" == Darwin && "$(uname -m)" == arm64 ]] || die "releases are cut on macOS arm64"
    fi
    is_semver "$version" || die "version must be X.Y.Z semver, got '$version'"
    branch="$(git -C "$ROOT" symbolic-ref --short -q HEAD || true)"
    [[ "$branch" == main ]] || die "must release from main, currently on '${branch:-a detached HEAD}'"
    [[ -z "$(git -C "$ROOT" status --porcelain)" ]] || die "working tree is not clean"
    git -C "$ROOT" fetch --quiet --tags origin || die "git fetch origin failed"
    [[ "$(git -C "$ROOT" rev-parse main)" == "$(git -C "$ROOT" rev-parse origin/main)" ]] \
        || die "local main differs from origin/main"
    latest="$(latest_tag_version "$ROOT")"
    if [[ -n "$latest" ]] && ! version_gt "$version" "$latest"; then
        die "version $version is not greater than latest tag v$latest"
    fi
    [[ -f "$RELEASE_KEY" ]] || die "signing key not found: $RELEASE_KEY"
    "$RELEASE_GH" auth status >/dev/null 2>&1 || die "gh auth status failed"
    if [[ -z "${RELEASE_PACKAGE_LINUX:-}" ]]; then
        "$RELEASE_SSH" "$FLEET_HOST" true >/dev/null 2>&1 || die "ssh $FLEET_HOST failed"
    fi
}

prepare_release() { # work dir, version
    set_cargo_version "$1/crates/clouddeck-app/Cargo.toml" "$2"
    set_cargo_version "$1/crates/clouddeck-core/Cargo.toml" "$2"
    if ! { (cd "$1" && "$RELEASE_CARGO" build --workspace) && (cd "$1" && "$RELEASE_CARGO" test --workspace); }; then
        git -C "$1" checkout -q -- crates/clouddeck-app/Cargo.toml crates/clouddeck-core/Cargo.toml Cargo.lock
        die "cargo build or test --workspace failed; version bump reverted, nothing committed"
    fi
    git -C "$1" add -u
    git -C "$1" commit -q -m "Release $2"
    git -C "$1" tag "v$2"
}

package_macos() { # work dir
    (cd "$1" && "$RELEASE_PACKAGE_MACOS") || die "macOS packaging failed"
}

remote_build_command() { # version
    local url
    url="$(git -C "$ROOT" remote get-url origin)"
    printf '%s' "set -e; [ -d $REMOTE_RELEASE_DIR/.git ] || git clone -q '$url' $REMOTE_RELEASE_DIR; cd $REMOTE_RELEASE_DIR && git fetch -q --tags origin && git checkout -q v$1 && CARGO_TARGET_DIR=\$PWD/target CARGO_PROFILE_RELEASE_DEBUG=0 nice -n 10 scripts/package-linux.sh"
}

package_linux() { # work dir, version, linux tarball
    if [[ -n "${RELEASE_PACKAGE_LINUX:-}" ]]; then
        (cd "$1" && "$RELEASE_PACKAGE_LINUX") || die "Linux packaging failed"
    else
        "$RELEASE_SSH" "$FLEET_HOST" "$(remote_build_command "$2")" || die "Linux packaging on $FLEET_HOST failed"
        "$RELEASE_SCP" "$FLEET_HOST:$REMOTE_RELEASE_DIR/dist/$(basename "$3")" "$1/dist/" || die "copying the Linux tarball failed"
    fi
    require_file "$3"
}

build_manifest() { # work dir, version, notes, mac tarball, linux tarball, dist dir
    (cd "$1" && "$RELEASE_CARGO" run --quiet --release -p clouddeck-core --bin release-manifest -- build \
        --version "$2" --notes-file "$3" \
        --asset "macos-aarch64=$4" --asset "linux-x86_64=$5" \
        --key "$RELEASE_KEY" --out "$6") || die "manifest build failed"
}

main() {
    local dry_run=0 positional=() arg
    for arg in "$@"; do
        case "$arg" in
            --dry-run) dry_run=1 ;;
            *) positional+=("$arg") ;;
        esac
    done
    [[ ${#positional[@]} -eq 2 ]] || die "usage: scripts/release.sh <version> <notes-file> [--dry-run]"
    local version="${positional[0]}" notes="${positional[1]}"
    [[ -f "$notes" ]] || die "notes file not found: $notes"
    notes="$(cd "$(dirname "$notes")" && pwd)/$(basename "$notes")"

    preflight "$version"

    local work="$ROOT"
    if (( dry_run )); then
        DRY_RUN_DIR="$(mktemp -d)"
        trap 'rm -rf "$DRY_RUN_DIR"' EXIT
        git clone -q "$ROOT" "$DRY_RUN_DIR/release"
        work="$DRY_RUN_DIR/release"
        git -C "$work" checkout -q -b "dry-run-release-$version"
    fi

    echo "==> bumping, testing, committing v$version in $work"
    prepare_release "$work" "$version"

    local dist="$work/dist"
    local mac_tar="$dist/CloudDeck-$version-macos-aarch64.tar.gz"
    local mac_dmg="$dist/CloudDeck-$version-macos-aarch64.dmg"
    local linux_tar="$dist/CloudDeck-$version-linux-x86_64.tar.gz"
    local manifest="$dist/manifest.json"
    local signature="$dist/manifest.json.sig"
    local assets=("$mac_tar" "$mac_dmg" "$linux_tar" "$manifest" "$signature")
    mkdir -p "$dist"

    echo "==> packaging macOS"
    package_macos "$work"
    require_file "$mac_tar" "$mac_dmg"

    if (( ! dry_run )); then
        echo "==> pushing main and v$version"
        "$RELEASE_GIT" -C "$work" push origin main "v$version" || die "push failed"
    fi

    if (( ! dry_run )) || [[ -n "${RELEASE_PACKAGE_LINUX:-}" ]]; then
        echo "==> packaging Linux"
        package_linux "$work" "$version" "$linux_tar"
        echo "==> signing manifest"
        build_manifest "$work" "$version" "$notes" "$mac_tar" "$linux_tar" "$dist"
        require_file "$manifest" "$signature"
    else
        echo "dry-run: Linux packaging and manifest skipped; fleet-vm cannot fetch the throwaway commit"
    fi

    echo "==> assets"
    summary "${assets[@]}"

    if (( dry_run )); then
        echo "dry-run: would run gh release create v$version with the assets above; nothing pushed or published"
        return 0
    fi

    echo "==> publishing"
    local url
    url="$("$RELEASE_GH" release create "v$version" "${assets[@]}" --title "CloudDeck $version" --notes-file "$notes")" \
        || die "gh release create failed"
    echo "Released $url"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
