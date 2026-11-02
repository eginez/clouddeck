#!/usr/bin/env bash
# Prints the git tag that pins clouddeck-extension-api at its API_VERSION, failing if a given tag does not match.
# With no tag argument, prints the expected tag.
set -euo pipefail

root_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
api_version="$(awk -F'[ ;]+' '/^pub const API_VERSION: u32 = / { print $(NF-1); exit }' "$root_dir/crates/clouddeck-extension-api/src/lib.rs")"
[[ "$api_version" =~ ^[0-9]+$ ]] || { echo "extension-api-tag: no API_VERSION in clouddeck-extension-api" >&2; exit 1; }

expected="extension-api-v$api_version"
tag="${1:-}"
if [[ -z "$tag" ]]; then
    echo "$expected"
    exit 0
fi

[[ "$tag" == "$expected" ]] || {
    echo "extension-api-tag: tag '$tag' does not match API_VERSION $api_version (expected $expected)" >&2
    exit 1
}
echo "$tag"
