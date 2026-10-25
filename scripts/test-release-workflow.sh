#!/usr/bin/env bash
# Checks .github/workflows/release.yml invariants and scripts/release-version.sh with grep and fixed inputs.
set -uo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
WF="$ROOT/.github/workflows/release.yml"
VERSION_SCRIPT="$ROOT/scripts/release-version.sh"
PASS=0
FAIL=0

ok() { PASS=$((PASS + 1)); echo "ok   - $1"; }
bad() { FAIL=$((FAIL + 1)); echo "FAIL - $1"; }
check() { # desc cmd...
    local d="$1"; shift
    if "$@"; then ok "$d"; else bad "$d"; fi
}

[[ -f "$WF" ]] || { echo "FAIL - $WF missing"; exit 1; }

check "lint-workflows is clean" "$ROOT/scripts/lint-workflows.sh"
check "no pull_request trigger" bash -c "! grep -Eq '^[[:space:]]*pull_request(_target)?:' '$WF'"
check "tag filter present" grep -Fq -e "- 'v*.*.*'" "$WF"
check "dry_run input defaults to true" bash -c "grep -A4 'dry_run:' '$WF' | grep -Fq 'default: true'"

write_jobs="$(awk '
    /^jobs:/ { injobs = 1; job = "(top)"; next }
    !injobs { if ($0 ~ /contents: write/) print "(top)"; next }
    /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { job = $1; sub(/:$/, "", job) }
    /contents: write/ { print job }
' "$WF")"
check "contents: write confined to publish-draft" test "$write_jobs" = "publish-draft"

unpinned="$(grep -E '^[[:space:]-]*uses:' "$WF" | grep -vE '@[0-9a-f]{40}([[:space:]]|$)' || true)"
check "every uses: pinned to a 40-char SHA" test -z "$unpinned"

check "publish job never un-drafts" bash -c "! grep -Eq -- '--draft(=|[[:space:]]+)(false|0)' '$WF' && ! grep -q 'gh release edit' '$WF'"
check "draft release is created with --draft" grep -Eq 'gh release create .*--draft' "$WF"
check "only GITHUB_TOKEN is used as a secret" bash -c "! grep -q 'secrets\\.' '$WF'"

CARGO_VERSION="$(awk -F'"' '/^version = / { print $2; exit }' "$ROOT/crates/clouddeck-app/Cargo.toml")"
check "version from tag matching Cargo.toml" bash -c "[[ \"\$('$VERSION_SCRIPT' v$CARGO_VERSION)\" == '$CARGO_VERSION' ]]"
check "version script without tag prints Cargo version" bash -c "[[ \"\$('$VERSION_SCRIPT')\" == '$CARGO_VERSION' ]]"
check "version script rejects mismatched tag" bash -c "! '$VERSION_SCRIPT' v0.2.0 >/dev/null 2>&1"
check "version script rejects non-semver tag" bash -c "! '$VERSION_SCRIPT' v0.1 >/dev/null 2>&1"
check "version script rejects tag without v" bash -c "! '$VERSION_SCRIPT' 0.1.0 >/dev/null 2>&1"

echo "$PASS passed, $FAIL failed"
[[ "$FAIL" -eq 0 ]]
