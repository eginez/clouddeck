#!/usr/bin/env bash
# Runs the gate's clippy command and prints only the diagnostics and a summary.
# Exits with clippy's exit code.
set -uo pipefail

out="$(mktemp)"
trap 'rm -f "$out"' EXIT

diag='^[^ ].*:[0-9]+:[0-9]+: (error|warning)(\[[A-Z0-9]+\])?:'

start=$(date +%s)
cargo clippy --workspace --all-targets --locked --message-format=short -- -D warnings >"$out" 2>&1
status=$?
elapsed=$(($(date +%s) - start))

grep -E "$diag" "$out" | head -n 200
if [ "$status" -ne 0 ] && ! grep -qE "$diag" "$out"; then
    grep -E '^error' "$out" | grep -v 'could not compile' | head -n 50
fi

errors=$(grep -cE '^[^ ].*:[0-9]+:[0-9]+: error(\[[A-Z0-9]+\])?:' "$out")
warnings=$(grep -cE '^[^ ].*:[0-9]+:[0-9]+: warning(\[[A-Z0-9]+\])?:' "$out")
echo "clippy-quiet: $errors errors, $warnings warnings, ${elapsed}s, exit $status"
exit "$status"
