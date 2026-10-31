#!/usr/bin/env bash
# Lints .github/workflows with actionlint, installing it with go when missing.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! command -v actionlint >/dev/null; then
    command -v go >/dev/null || { echo "lint-workflows: need actionlint or go to install it" >&2; exit 1; }
    go install github.com/rhysd/actionlint/cmd/actionlint@latest
    gopath="$(go env GOPATH)"
    export PATH="$gopath/bin:$PATH"
fi

cd "$root"
actionlint .github/workflows/*.yml
