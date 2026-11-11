#!/usr/bin/env bash
# Checks testing/emulators without Docker: syntax, shellcheck, the launcher's dry-run output and the sample files.
set -uo pipefail

SRC="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
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

mkdir -p "$T/bin"
cat >"$T/bin/docker" <<'EOF'
#!/bin/sh
echo "docker called: $*" >>"$FAKE_DOCKER_LOG"
exit 99
EOF
chmod +x "$T/bin/docker"
export FAKE_DOCKER_LOG="$T/docker.log"

SCRIPTS=("$EMU/run.sh" "$EMU/aws/seed.sh" "$EMU/gcp/seed.sh")
for script in "${SCRIPTS[@]}"; do
    check "bash -n $(basename "$(dirname "$script")")/$(basename "$script")" bash -n "$script"
done

if command -v shellcheck >/dev/null 2>&1; then
    check "shellcheck clean" shellcheck "${SCRIPTS[@]}" "$0"
else
    echo "skip - shellcheck not installed"
fi

out="$T/start-all.txt"
PATH="$T/bin:$PATH" bash "$EMU/run.sh" start all --dry-run >"$out" 2>&1
check "start all --dry-run exits 0" test $? -eq 0
check "dry-run never calls docker" test ! -e "$FAKE_DOCKER_LOG"
check "dry-run starts the AWS compose project" grep -q "docker compose -p clouddeck-aws -f $EMU/aws/docker-compose.yml up -d --wait" "$out"
check "dry-run starts the GCP compose project" grep -q "docker compose -p clouddeck-gcp -f $EMU/gcp/docker-compose.yml up -d --wait" "$out"
check "dry-run seeds AWS" grep -q "clouddeck-aws -f $EMU/aws/docker-compose.yml --profile seed run --rm seed" "$out"
check "dry-run seeds GCP" grep -q "clouddeck-gcp -f $EMU/gcp/docker-compose.yml --profile seed run --rm seed" "$out"

check "from another directory" bash -c "cd '$T' && PATH='$T/bin:$PATH' bash '$EMU/run.sh' start aws --dry-run >/dev/null 2>&1"

aws_only="$T/start-aws.txt"
PATH="$T/bin:$PATH" bash "$EMU/run.sh" start aws --dry-run >"$aws_only" 2>&1
check "start aws touches only the AWS project" bash -c "! grep -q clouddeck-gcp '$aws_only'"

check "AWS hint exports both config variables" grep -q "export AWS_CONFIG_FILE='$EMU/aws/config' AWS_SHARED_CREDENTIALS_FILE='$EMU/aws/credentials'" "$aws_only"
check "AWS hint names the in-app Config folder" grep -q "method Use an AWS profile, Config folder: $EMU/aws, pick local-dev, Connect." "$aws_only"
check "AWS env-var fallback is labelled" grep -q "^or, without the app setting:" "$aws_only"
check "GCP hint names the in-app method and Config folder" grep -q "method Use gcloud application-default credentials, Config folder: $EMU/gcp, Connect." "$out"
check "GCP hint has no endpoint overrides" bash -c "! grep -q 'compute_endpoint' '$out'"
check "README has the reset step" grep -q "Reset the app to your real account" "$EMU/README.md"

if python3 -c 'import json,sys; [json.load(open(p)) for p in sys.argv[1:]]' \
    "$EMU/gcp/emulator.json" "$EMU/gcp/application_default_credentials.json" 2>/dev/null; then
    ok "GCP sample JSON files parse"
else
    bad "GCP sample JSON files parse"
fi

for profile in local-dev local-prod local-down; do
    check "AWS config has profile $profile" grep -q "^\[profile $profile\]" "$EMU/aws/config"
done

for file in \
    README.md run.sh \
    aws/docker-compose.yml aws/seed.sh aws/config aws/credentials \
    gcp/docker-compose.yml gcp/seed.sh gcp/emulator.json gcp/application_default_credentials.json; do
    check "file exists: $file" test -f "$EMU/$file"
done

while IFS= read -r ref; do
    ref="${ref%.}"
    check "README reference exists: $ref" test -e "$SRC/$ref"
done < <(grep -o 'testing/emulators/[A-Za-z0-9_./-]*' "$EMU/README.md" | sort -u)

check "gitignore covers seed markers" grep -qx "testing/emulators/\*/.seeded" "$SRC/.gitignore"

echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
