#!/usr/bin/env bash
# Runs the gate's `cargo test --workspace --locked` and prints only failures and a summary.
# Extra arguments go to cargo test. Exits with cargo test's exit code.
set -uo pipefail

out="$(mktemp)"
trap 'rm -f "$out"' EXIT

start=$(date +%s)
cargo test --workspace --locked --no-fail-fast "$@" >"$out" 2>&1
status=$?
elapsed=$(($(date +%s) - start))

awk '
    /^---- .* stdout ----$/ { print; inblock = 1; n = 0; next }
    /^failures:$/ || /^test result:/ || /^---- / { inblock = 0 }
    inblock && n < 40 { print; n++ }
' "$out"

if [ "$status" -ne 0 ]; then
    grep -E -A2 '^error(\[[A-Z0-9]+\])?: ' "$out" | grep -v 'could not compile' | head -n 120
fi

passed=$(grep -E '^test result:' "$out" | awk '{ p += $4 } END { print p + 0 }')
failed=$(grep -E '^test result:' "$out" | awk '{ f += $6 } END { print f + 0 }')
echo "test-quiet: passed $passed, failed $failed, ${elapsed}s, exit $status"
exit "$status"
