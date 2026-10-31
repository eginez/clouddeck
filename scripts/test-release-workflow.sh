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
check "contents: write confined to publish-draft and sign-and-publish" \
    test "$write_jobs" = "$(printf 'publish-draft\nsign-and-publish')"

unpinned="$(grep -E '^[[:space:]-]*uses:' "$WF" | grep -vE '@[0-9a-f]{40}([[:space:]]|$)' || true)"
check "every uses: pinned to a 40-char SHA" test -z "$unpinned"

# job_of PATTERN: the jobs whose lines contain PATTERN as a fixed string
job_of() { awk -v pat="$1" '/^jobs:/ { injobs = 1; next } injobs && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { job = $1; sub(/:$/, "", job) } index($0, pat) { print job }' "$WF" | sort -u; }
job_block() { awk -v j="$1" '/^  [A-Za-z0-9_-]+:[[:space:]]*$/ { inj = ($1 == j ":") } inj' "$WF"; }
line_of() { grep -nF -- "$1" "$WF" | head -n1 | cut -d: -f1; }
undraft_pattern='--draft(=|[[:space:]]+)(false|0)'

only_sign_undrafts() {
    [[ "$(job_of 'gh release edit')" == sign-and-publish ]] || return 1
    local total in_sign
    total="$(grep -Ec -- "$undraft_pattern" "$WF")"
    in_sign="$(job_block sign-and-publish | grep -Ec -- "$undraft_pattern")"
    [[ "$total" -ge 1 && "$total" == "$in_sign" ]]
}
check "only sign-and-publish un-drafts" only_sign_undrafts
check "draft release is created with --draft" grep -Eq 'gh release create .*--draft' "$WF"

only_secret_is_signing_key() {
    [[ "$(grep -c 'secrets\.' "$WF")" == 1 ]] || return 1
    grep -Fq 'secrets.RELEASE_SIGNING_KEY' "$WF" || return 1
    [[ "$(job_of 'secrets.RELEASE_SIGNING_KEY')" == sign-and-publish ]]
}
check "RELEASE_SIGNING_KEY is the only secret, used once in sign-and-publish" only_secret_is_signing_key

check "sign-and-publish runs in environment release" \
    bash -c "job_block() { awk -v j=\"\$1\" '/^  [A-Za-z0-9_-]+:[[:space:]]*\$/ { inj = (\$1 == j \":\") } inj' '$WF'; }; job_block sign-and-publish | grep -Fxq '    environment: release'"
check "sign-and-publish needs publish-draft" \
    bash -c "awk '/^  sign-and-publish:/ { f = 1; next } f && /^  [A-Za-z0-9_-]+:[[:space:]]*\$/ { f = 0 } f && /^    needs:/' '$WF' | grep -Fq publish-draft"
check "sign-and-publish runs only from a manual dispatch of a tag by the repo owner" \
    bash -c "awk '/^  sign-and-publish:/ { f = 1; next } f && /^  [A-Za-z0-9_-]+:[[:space:]]*\$/ { f = 0 } f && /^    if:/' '$WF' | grep -Fq \"workflow_dispatch\" && awk '/^  sign-and-publish:/ { f = 1; next } f && /^  [A-Za-z0-9_-]+:[[:space:]]*\$/ { f = 0 } f && /^    if:/' '$WF' | grep -Fq 'github.ref_type == '\''tag'\''' && awk '/^  sign-and-publish:/ { f = 1; next } f && /^  [A-Za-z0-9_-]+:[[:space:]]*\$/ { f = 0 } f && /^    if:/' '$WF' | grep -Fq 'github.actor == github.repository_owner'"
check "sign-and-publish has no push trigger path" \
    bash -c "awk '/^  sign-and-publish:/ { f = 1; next } f && /^  [A-Za-z0-9_-]+:[[:space:]]*\$/ { f = 0 } f' '$WF' | grep -Fq \"'push'\" && exit 1 || exit 0"
check "signing key is written under RUNNER_TEMP" grep -Fq 'key="$RUNNER_TEMP/release-signing.key"' "$WF"
check "signing key file is created with umask 077" grep -Fq 'umask 077' "$WF"
check "signing key removal step runs always" \
    bash -c "l=\$(grep -nF 'rm -f \"\$RUNNER_TEMP/release-signing.key\"' '$WF' | head -n1 | cut -d: -f1); [[ -n \"\$l\" ]] && sed -n \"\$((l-1))p\" '$WF' | grep -Fqx '        if: always()'"
check "no line echoes the signing key" bash -c "! grep -Eq 'echo.*RELEASE_SIGNING_KEY' '$WF'"
check "no set -x in the workflow" bash -c "! grep -Eq 'set -[a-z]*x([[:space:]]|\$)' '$WF'"
check "release-manifest is built before the key is written" \
    bash -c "b=\$(grep -nF 'cargo build --locked --release -p clouddeck-core --bin release-manifest' '$WF' | head -n1 | cut -d: -f1); k=\$(grep -nF 'printf \"%s\" \"\$RELEASE_SIGNING_KEY\" > \"\$key\"' '$WF' | head -n1 | cut -d: -f1); [[ -n \"\$b\" && -n \"\$k\" && \"\$b\" -lt \"\$k\" ]]"
check "manifest is verified before the release is un-drafted" \
    bash -c "v=\$(grep -nF 'release-manifest verify --dir dist' '$WF' | head -n1 | cut -d: -f1); e=\$(grep -nF 'gh release edit' '$WF' | head -n1 | cut -d: -f1); [[ -n \"\$v\" && -n \"\$e\" && \"\$v\" -lt \"\$e\" ]]"
check "sign-and-publish reads notes from docs/release-notes" grep -Fq 'docs/release-notes/v$VERSION.md' "$WF"
check "publish sends the release notes file" grep -Fq 'gh release edit "$TAG" --draft=false --notes-file "docs/release-notes/v$VERSION.md"' "$WF"
check "draft assets are compared to the signed files before publishing" \
    bash -c "c=\$(grep -nF 'cmp \"\$f\"' '$WF' | head -n1 | cut -d: -f1); e=\$(grep -nF 'gh release edit' '$WF' | head -n1 | cut -d: -f1); [[ -n \"\$c\" && -n \"\$e\" && \"\$c\" -lt \"\$e\" ]]"
check "sign-and-publish refuses an already-published tag" grep -Fq 'already published' "$WF"

CARGO_VERSION="$(awk -F'"' '/^version = / { print $2; exit }' "$ROOT/crates/clouddeck-app/Cargo.toml")"
check "version from tag matching Cargo.toml" bash -c "[[ \"\$('$VERSION_SCRIPT' v$CARGO_VERSION)\" == '$CARGO_VERSION' ]]"
check "version script without tag prints Cargo version" bash -c "[[ \"\$('$VERSION_SCRIPT')\" == '$CARGO_VERSION' ]]"
check "version script rejects mismatched tag" bash -c "! '$VERSION_SCRIPT' v0.2.0 >/dev/null 2>&1"
check "version script rejects non-semver tag" bash -c "! '$VERSION_SCRIPT' v0.1 >/dev/null 2>&1"
check "version script rejects tag without v" bash -c "! '$VERSION_SCRIPT' 0.1.0 >/dev/null 2>&1"

echo "$PASS passed, $FAIL failed"
[[ "$FAIL" -eq 0 ]]
