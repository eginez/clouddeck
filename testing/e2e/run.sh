#!/usr/bin/env bash
# Screenshots the real app against the local emulators and compares each one with
# testing/e2e/scenarios/<name>/expected/main.png.
#   testing/e2e/run.sh [--dry-run] [--no-up] [--update [scenario]] [outdir]
# Default outdir /tmp/clouddeck-e2e, not committed. --update records the new screenshot
# as the expected one. Exits non-zero when a scenario or the data check fails.
# Env: CLOUDDECK_BIN (target/debug/clouddeck), E2E_DISPLAY (:96), E2E_SCREEN (1600x1000x24),
#      E2E_CROP_DX (2), E2E_DRY_RUN_DIR (where --dry-run writes the config folders).
set -euo pipefail
shopt -s nullglob

root=$(cd "$(dirname "$0")/../.." && pwd)
e2e="$root/testing/e2e"
emu="$root/testing/emulators"
bin=${CLOUDDECK_BIN:-$root/target/debug/clouddeck}
disp=${E2E_DISPLAY:-:96}
screen=${E2E_SCREEN:-1600x1000x24}
crop_dx=${E2E_CROP_DX:-2}
sw=${screen%%x*}
rest=${screen#*x}
sh=${rest%%x*}

usage() {
    echo "usage: $0 [--dry-run] [--no-up] [--update [scenario]] [outdir]" >&2
    exit 2
}

dry=0
no_up=0
update=0
only=""
outdir=/tmp/clouddeck-e2e
while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) dry=1 ;;
        --no-up) no_up=1 ;;
        --update)
            update=1
            if [ $# -gt 1 ] && [ "${2#-}" = "$2" ]; then
                [ -d "$e2e/scenarios/$2" ] || { echo "no scenario named $2" >&2; exit 2; }
                only="$2"
                shift
            fi
            ;;
        -h | --help) usage ;;
        -*) usage ;;
        *) outdir="$1" ;;
    esac
    shift
done

names=()
for dir in "$e2e"/scenarios/*/; do
    name=$(basename "$dir")
    if [ -z "$only" ] || [ "$only" = "$name" ]; then names+=("$name"); fi
done
[ "${#names[@]}" -gt 0 ] || { echo "no scenarios under $e2e/scenarios" >&2; exit 2; }

tmp=$(mktemp -d)
app=""
xvfb=""
cleanup() {
    [ -n "$app" ] && kill "$app" 2>/dev/null
    [ -n "$xvfb" ] && kill "$xvfb" 2>/dev/null
    rm -rf "$tmp"
    true
}
trap cleanup EXIT

make_config() { # name base
    local src="$e2e/scenarios/$1/config/clouddeck" dst="$2/xdg/clouddeck" file
    mkdir -p "$dst"
    for file in "$src"/*.json; do
        sed "s|@EMULATORS@|$emu|g" "$file" >"$dst/$(basename "$file")"
    done
}

data_check() {
    local creds="$emu/aws/credentials"
    if [ "$dry" -eq 1 ]; then
        echo "would run: CLOUDDECK_EMULATORS=1 AWS_SHARED_CREDENTIALS_FILE=$creds cargo test --manifest-path $root/Cargo.toml -p clouddeck-aws --test emulators -- --ignored"
        return 0
    fi
    if CLOUDDECK_EMULATORS=1 AWS_SHARED_CREDENTIALS_FILE="$creds" \
        cargo test --manifest-path "$root/Cargo.toml" -p clouddeck-aws --test emulators -- --ignored \
        >"$outdir/data-check.log" 2>&1; then
        echo "PASS data check: the AWS emulator lists seeded instances"
    else
        echo "FAIL data check: see $outdir/data-check.log"
        return 1
    fi
}

stop_app() {
    [ -n "$app" ] && kill "$app" 2>/dev/null
    [ -n "$app" ] && wait "$app" 2>/dev/null
    app=""
    true
}

run_scenario() { # name; returns 1 on FAIL
    local name="$1" dir="$e2e/scenarios/$1"
    local SIZE WAIT EXTRA_ARGS TOLERANCE_PCT
    SIZE=1280x760
    WAIT=15
    EXTRA_ARGS=""
    TOLERANCE_PCT=0.5
    # shellcheck source=/dev/null
    . "$dir/scenario.env"
    local extra=() actual="$outdir/$name/actual.png" expected="$dir/expected/main.png"
    local diff="$outdir/$name/diff.png"
    read -r -a extra <<<"$EXTRA_ARGS"

    if [ "$dry" -eq 1 ]; then
        make_config "$name" "${E2E_DRY_RUN_DIR:-$tmp}/$name"
        echo "scenario $name: size $SIZE, wait ${WAIT}s, tolerance ${TOLERANCE_PCT}%"
        echo "  config: ${E2E_DRY_RUN_DIR:-$tmp}/$name/xdg/clouddeck (written)"
        echo "  would run: XDG_CONFIG_HOME=${E2E_DRY_RUN_DIR:-$tmp}/$name/xdg DISPLAY=$disp $bin --size $SIZE $EXTRA_ARGS"
        echo "  would write: $actual"
        if [ -f "$expected" ]; then
            echo "  would compare with: $expected"
        else
            echo "  no expected/main.png yet (NEW)"
        fi
        echo "  approve: $e2e/run.sh --update $name"
        return 0
    fi

    make_config "$name" "$tmp/$name"
    mkdir -p "$outdir/$name"
    rm -f "$actual" "$diff"
    XDG_CONFIG_HOME="$tmp/$name/xdg" DISPLAY="$disp" "$bin" --size "$SIZE" ${extra[@]+"${extra[@]}"} \
        >"$outdir/$name/app.log" 2>&1 &
    app=$!
    sleep "$WAIT"
    if ! kill -0 "$app" 2>/dev/null; then
        app=""
        echo "FAIL $name: the app exited early (see $outdir/$name/app.log)"
        return 1
    fi
    local w=${SIZE%x*} h=${SIZE#*x} x y
    x=$(((sw - w) / 2 + crop_dx))
    y=$(((sh - h) / 2))
    if ! DISPLAY="$disp" xdotool mousemove 5 5 || ! { sleep 1; DISPLAY="$disp" xdotool mousemove 600 400; } ||
        ! { sleep 2; DISPLAY="$disp" import -window root "$tmp/$name/full.png"; } ||
        ! convert "$tmp/$name/full.png" -crop "${w}x${h}+${x}+${y}" +repage "$actual"; then
        stop_app
        echo "FAIL $name: capture failed (xdotool, import or crop)"
        return 1
    fi
    stop_app

    if [ "$update" -eq 1 ]; then
        mkdir -p "$(dirname "$expected")"
        if ! cp "$actual" "$expected"; then
            echo "FAIL $name: cannot write $expected"
            return 1
        fi
        echo "UPDATED $name: $expected"
        return 0
    fi
    if [ ! -f "$expected" ]; then
        echo "NEW  $name: $actual (no expected/main.png yet; approve with $e2e/run.sh --update $name)"
        return 0
    fi

    local ew eh aw ah px rc=0 total pct
    if ! read -r ew eh <<<"$(identify -format '%w %h' "$expected")" || [ -z "$ew" ] || [ -z "$eh" ]; then
        echo "FAIL $name: cannot read $expected"
        return 1
    fi
    read -r aw ah <<<"$(identify -format '%w %h' "$actual")"
    if [ "$ew" != "$aw" ] || [ "$eh" != "$ah" ]; then
        convert "$actual" -resize "${ew}x${eh}!" "$actual"
    fi
    px=$(compare -metric AE -fuzz 2% "$expected" "$actual" "$diff" 2>&1) || rc=$?
    if [ "$rc" -gt 1 ] || ! [[ "$px" =~ ^[0-9]+$ ]]; then
        echo "FAIL $name: compare error: $px"
        return 1
    fi
    total=$((ew * eh))
    pct=$(awk -v p="$px" -v t="$total" 'BEGIN { printf "%.2f", p * 100 / t }')
    if [[ "$pct" =~ ^[0-9]+\.[0-9]+$ ]] && awk -v p="$pct" -v t="$TOLERANCE_PCT" 'BEGIN { exit !(p + 0 <= t + 0) }'; then
        echo "PASS $name: $pct% of pixels differ (tolerance $TOLERANCE_PCT%)"
        return 0
    fi
    echo "FAIL $name: $pct% of pixels differ (tolerance $TOLERANCE_PCT%), see $diff"
    return 1
}

failed=0
if [ "$dry" -eq 0 ]; then
    [ -x "$bin" ] || {
        echo "no app binary at $bin: run cargo build -p clouddeck-app and scripts/stage-bundled-extensions.sh debug" >&2
        exit 1
    }
    if [ "$no_up" -eq 0 ]; then "$emu/run.sh" start all; fi
    mkdir -p "$outdir"
    Xvfb "$disp" -screen 0 "$screen" >/dev/null 2>&1 &
    xvfb=$!
    sleep 2
elif [ "$no_up" -eq 0 ]; then
    echo "would run: $emu/run.sh start all"
fi

for name in "${names[@]}"; do
    run_scenario "$name" || failed=$((failed + 1))
done
data_check || failed=$((failed + 1))

if [ "$failed" -gt 0 ]; then
    echo "$failed failed"
    exit 1
fi
echo "all passed (NEW scenarios need approval with --update)"
