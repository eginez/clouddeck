#!/usr/bin/env bash
# Starts, stops and seeds the local AWS (floci) and GCP (floci-gcp) emulators.
# Usage: run.sh start|stop|reset|status [all|aws|gcp] [--dry-run]
# Runs on macOS bash 3.2 and Linux bash; do not use bash 4 features.
set -euo pipefail

emulators_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
    echo "usage: $0 start|stop|reset|status [all|aws|gcp] [--dry-run]" >&2
    exit 2
}

action=""
target="all"
dry_run=0
for arg in "$@"; do
    case "$arg" in
        start | stop | reset | status) action="$arg" ;;
        all | aws | gcp) target="$arg" ;;
        --dry-run) dry_run=1 ;;
        *) echo "unknown argument: $arg" >&2; usage ;;
    esac
done
[ -n "$action" ] || usage

case "$target" in
    all) clouds="aws gcp" ;;
    *) clouds="$target" ;;
esac

run_cmd() {
    if [ "$dry_run" -eq 1 ]; then
        echo "+ $*"
    else
        "$@"
    fi
}

compose() { # cloud args...
    local cloud="$1"
    shift
    run_cmd docker compose -p "clouddeck-$cloud" -f "$emulators_dir/$cloud/docker-compose.yml" "$@"
}

seeded_marker() { # cloud
    echo "$emulators_dir/$1/.seeded"
}

start_cloud() { # cloud
    local cloud="$1"
    local marker
    marker="$(seeded_marker "$cloud")"
    compose "$cloud" up -d --wait
    if [ "$dry_run" -eq 0 ] && [ -f "$marker" ]; then
        echo "$cloud: already seeded (run reset to seed again)"
    else
        if ! compose "$cloud" --profile seed run --rm seed; then
            echo "$cloud: seeding failed partway; run '$0 reset $cloud' to start from empty" >&2
            exit 1
        fi
        run_cmd touch "$marker"
    fi
    print_hint "$cloud"
}

stop_cloud() { # cloud
    compose "$1" stop
}

reset_cloud() { # cloud
    compose "$1" down
    run_cmd rm -f "$(seeded_marker "$1")"
    start_cloud "$1"
}

print_hint() { # cloud
    case "$1" in
        aws)
            echo
            echo "AWS emulator is up at http://localhost:4566 with 12 seeded EC2 instances."
            echo "In CloudDeck: Connect (first run, or the provider picker, Connect again), choose Amazon Web Services,"
            echo "method Use an AWS profile, Config folder: $emulators_dir/aws, pick local-dev, Connect."
            echo "You should see profiles local-dev (us-east-1) and local-prod (eu-west-1) with instances,"
            echo "and local-down failing, because nothing listens on port 4599."
            echo "or, without the app setting: in the shell you start CloudDeck from, run:"
            echo "  export AWS_CONFIG_FILE='$emulators_dir/aws/config' AWS_SHARED_CREDENTIALS_FILE='$emulators_dir/aws/credentials'"
            echo "  clouddeck    (or: cargo run -p clouddeck-app, after scripts/stage-bundled-extensions.sh debug)"
            ;;
        gcp)
            echo
            echo "GCP emulator is up at http://localhost:4588 with projects demo-web and demo-data."
            echo "In CloudDeck: Connect (first run, or the provider picker, Connect again), choose Google Cloud,"
            echo "method Use gcloud application-default credentials, Config folder: $emulators_dir/gcp, Connect."
            echo "You should see projects demo-web and demo-data with a region and zone tree of instances."
            echo "The All providers view shows both the AWS and GCP instances."
            ;;
    esac
}

if [ "$dry_run" -eq 0 ]; then
    case "$action" in
        start | reset)
            command -v docker >/dev/null 2>&1 || {
                echo "docker not found: install Docker Desktop or OrbStack first" >&2
                exit 1
            }
            ;;
    esac
fi

for cloud in $clouds; do
    case "$action" in
        start) start_cloud "$cloud" ;;
        stop) stop_cloud "$cloud" ;;
        reset) reset_cloud "$cloud" ;;
        status) compose "$cloud" ps ;;
    esac
done
