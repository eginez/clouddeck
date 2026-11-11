#!/usr/bin/env bash
# Checks testing/e2e without Docker or the app: syntax, shellcheck, the dry-run output and the scenario files.
set -uo pipefail

SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
E2E="$SRC/testing/e2e"
EMU="$SRC/testing/emulators"
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
json_ok() { python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "$1"; }
no_placeholder() { ! grep -q "@EMULATORS@" "$1"/*.json; }

check "bash -n run.sh" bash -n "$E2E/run.sh"
if command -v shellcheck >/dev/null 2>&1; then
    check "shellcheck clean" shellcheck "$E2E/run.sh" "$0"
else
    echo "skip - shellcheck not installed"
fi

out="$T/dry-run.txt"
E2E_DRY_RUN_DIR="$T/cfg" bash "$E2E/run.sh" --dry-run "$T/out" >"$out" 2>&1
check "--dry-run exits 0" test $? -eq 0
check "--dry-run prints no PASS or FAIL results" bash -c "! grep -q '^PASS\|^FAIL' '$out'"
check "--dry-run plans the emulator start" grep -qF "would run: $EMU/run.sh start all" "$out"

scenarios=()
for dir in "$E2E"/scenarios/*/; do scenarios+=("$(basename "$dir")"); done
check "at least one scenario folder" test "${#scenarios[@]}" -gt 0

for name in "${scenarios[@]}"; do
    check "dry-run lists scenario $name" grep -q "^scenario $name:" "$out"
    check "dry-run prints the update command for $name" grep -q "run.sh --update $name\$" "$out"
    check "scenario $name has scenario.env" test -f "$E2E/scenarios/$name/scenario.env"
    check "scenario $name has expected/.gitkeep" test -f "$E2E/scenarios/$name/expected/.gitkeep"

    for json in "$E2E/scenarios/$name"/config/clouddeck/*.json; do
        check "committed $(basename "$json") parses for $name" json_ok "$json"
    done

    gen="$T/cfg/$name/xdg/clouddeck"
    check "dry-run writes the config folder for $name" test -f "$gen/settings.json"
    for json in "$gen"/*.json; do
        check "generated $(basename "$json") parses for $name" json_ok "$json"
    done
    check "generated $name config has no placeholder left" no_placeholder "$gen"
    check "generated $name settings point at the AWS Config folder" grep -qF "\"$EMU/aws\"" "$gen/settings.json"
    check "generated $name settings point at the GCP Config folder" grep -qF "\"$EMU/gcp\"" "$gen/settings.json"
done

check "README exists" test -f "$E2E/README.md"

echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
