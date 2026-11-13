#!/usr/bin/env bash
# Checks test-quiet.sh and clippy-quiet.sh against a passing and a failing fixture crate.
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

make_fixture() { # dir lib.rs
    mkdir -p "$1/src"
    printf '[package]\nname = "fixture"\nversion = "0.1.0"\nedition = "2021"\n' >"$1/Cargo.toml"
    cp "$2" "$1/src/lib.rs"
    (cd "$1" && cargo generate-lockfile --quiet)
}

cat >"$T/pass.rs" <<'EOF'
#[test]
fn adds() {
    assert_eq!(2 + 2, 4);
}
EOF

cat >"$T/fail.rs" <<'EOF'
pub fn copied() -> i32 {
    let a: i32 = 1;
    a.clone()
}

#[test]
fn copied_is_one() {
    assert_eq!(copied(), 2, "copied should be two");
}
EOF

make_fixture "$T/pass" "$T/pass.rs"
make_fixture "$T/fail" "$T/fail.rs"

out="$T/out"

(cd "$T/pass" && "$SRC/scripts/test-quiet.sh" >"$out" 2>&1)
check "test-quiet exits 0 on passing fixture" [ $? -eq 0 ]
check "test-quiet summary counts the passing test" grep -q 'passed 1, failed 0' "$out"

(cd "$T/fail" && "$SRC/scripts/test-quiet.sh" >"$out" 2>&1)
status=$?
check "test-quiet exits nonzero on failing fixture" [ "$status" -ne 0 ]
check "test-quiet names the failing test" grep -q -- '---- copied_is_one stdout ----' "$out"
check "test-quiet prints the assertion message" grep -q 'copied should be two' "$out"
check "test-quiet summary counts the failure" grep -q 'passed 0, failed 1' "$out"

(cd "$T/pass" && "$SRC/scripts/clippy-quiet.sh" >"$out" 2>&1)
check "clippy-quiet exits 0 on clean fixture" [ $? -eq 0 ]
check "clippy-quiet summary reports no errors" grep -q '0 errors, 0 warnings' "$out"

(cd "$T/fail" && "$SRC/scripts/clippy-quiet.sh" >"$out" 2>&1)
status=$?
check "clippy-quiet exits nonzero on failing fixture" [ "$status" -ne 0 ]
check "clippy-quiet prints file:line of the lint" grep -q 'src/lib.rs:3:' "$out"
check "clippy-quiet prints the lint message" grep -q 'using `clone` on type `i32`' "$out"
check "clippy-quiet summary counts the failing lint" grep -q "1 errors, 0 warnings" "$out"

echo "passed $PASS, failed $FAIL"
[ "$FAIL" -eq 0 ]
