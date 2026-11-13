#!/usr/bin/env bash
# Checks scripts/code-nav.sh on a fixture repo and on crates/clouddeck-core/src/lib.rs.
# Skips with exit 0 when rust-analyzer is not installed.
set -euo pipefail

REPO=$(git rev-parse --show-toplevel)
NAV="$REPO/scripts/code-nav.sh"

if ! rust-analyzer --version >/dev/null 2>&1; then
    echo "test-code-nav: skipped, rust-analyzer not found" >&2
    exit 0
fi

fail() { echo "test-code-nav: FAIL: $*" >&2; exit 1; }

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

FIX="$TMP/fixture"
mkdir -p "$FIX/src"
cd "$FIX"
git init -q
cat > src/nav.rs <<'RS'
pub struct NavFixture {
    value: u32,
}

impl NavFixture {
    pub fn new(value: u32) -> Self {
        Self { value }
    }

    /// Doubles the stored value.
    pub fn answer(&self) -> u32 {
        self.value * 2
    }
}

pub fn make_answer() -> u32 {
    NavFixture::new(21).answer()
}
RS

outline_out=$("$NAV" outline src/nav.rs) || fail "outline exited non-zero"
if ! grep -q 'struct NavFixture' <<<"$outline_out"; then fail "outline lacks struct NavFixture"; fi
if ! grep -q 'fn answer' <<<"$outline_out"; then fail "outline lacks fn answer"; fi

want_line=$(grep -n 'pub fn answer' src/nav.rs | cut -d: -f1)
def_out=$("$NAV" def NavFixture::answer) || fail "def exited non-zero"
if ! grep -q "^src/nav.rs:${want_line} " <<<"$def_out"; then
    fail "def NavFixture::answer: expected src/nav.rs:$want_line, got: $def_out"
fi

show_out=$("$NAV" show NavFixture::answer) || fail "show exited non-zero"
if ! grep -q 'self.value \* 2' <<<"$show_out"; then fail "show lacks the method body"; fi
if ! grep -q 'Doubles the stored value' <<<"$show_out"; then fail "show dropped the doc comment"; fi
if grep -q 'make_answer' <<<"$show_out"; then fail "show printed the item after the method"; fi

refs_out=$("$NAV" refs answer) || fail "refs exited non-zero"
if ! grep -q 'text matches' <<<"$refs_out"; then fail "refs lacks the text-match header"; fi
if ! grep -q 'src/nav.rs:' <<<"$refs_out"; then fail "refs found no mention in src/nav.rs"; fi
if [ -e target/code-nav/index.scip ]; then fail "refs without --precise built the SCIP index"; fi

if "$NAV" bogus >/dev/null 2>&1; then fail "unknown command was accepted"; fi
if "$NAV" def NavFixture::answer --precise >/dev/null 2>&1; then fail "--precise accepted on def"; fi

cd "$REPO"
lib_out=$("$NAV" outline crates/clouddeck-core/src/lib.rs) || fail "outline of clouddeck-core lib.rs exited non-zero"
if ! grep -q '^crates/clouddeck-core/src/lib.rs (' <<<"$lib_out"; then fail "outline header missing for lib.rs"; fi
if [ "$(wc -l <<<"$lib_out")" -lt 2 ]; then fail "outline of lib.rs listed no items"; fi
lib_def=$("$NAV" def query) || fail "def query exited non-zero"
if ! grep -q '^crates/clouddeck-core/src/lib.rs:18 ' <<<"$lib_def"; then fail "def query: expected lib.rs:18, got: $lib_def"; fi
lib_show=$("$NAV" show query) || fail "show query exited non-zero"
if ! grep -q '^==> crates/clouddeck-core/src/lib.rs:18-18 ' <<<"$lib_show"; then fail "show query lacks the lib.rs item"; fi

echo "test-code-nav: ok"
