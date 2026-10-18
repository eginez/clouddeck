#!/usr/bin/env bash
# Screenshot the real clouddeck app (demo mode) under Xvfb + Lavapipe.
#   scripts/render-shot.sh <happy|empty|failure|partial|large> <out.png> [--state <Name>] [--size WxH]
# --state opens the app in a named UI state (clouddeck --render-state; implies the
# scenario that state needs, so the scenario argument is then only a fallback label
# and is not passed). --size sets the main window content size. See real-render.md.
# Env: CLOUDDECK_BIN (default $CARGO_TARGET_DIR|target/debug/clouddeck),
#      RR_DISPLAY (default :99), RR_SCREEN (1600x1000x24), RR_WAIT (seconds, 10).
# Needs the blade-graphics patch (Cargo.toml [patch.crates-io], bead
# clouddeck-rs-sr7i.1) or the window stays black. See brain/reference/real-render.md.
set -euo pipefail
usage='usage: render-shot.sh <scenario> <out.png> [--state <Name>] [--size WxH]'
scenario=${1:?$usage}
out=${2:?$usage}
shift 2
state=
size=
while [ $# -gt 0 ]; do
  case $1 in
    --state) state=${2:?$usage}; shift 2 ;;
    --size) size=${2:?$usage}; shift 2 ;;
    *) echo "$usage" >&2; exit 2 ;;
  esac
done
args=()
if [ -n "$state" ]; then args+=(--render-state "$state"); else args+=(--demo --demo-scenario "$scenario"); fi
if [ -n "$size" ]; then args+=(--size "$size"); fi
root=$(cd "$(dirname "$0")/.." && pwd)
bin=${CLOUDDECK_BIN:-${CARGO_TARGET_DIR:-$root/target}/debug/clouddeck}
disp=${RR_DISPLAY:-:99}
screen=${RR_SCREEN:-1600x1000x24}
wait_s=${RR_WAIT:-10}

Xvfb "$disp" -screen 0 "$screen" >/dev/null 2>&1 &
xvfb=$!
app=
cleanup() { [ -n "$app" ] && kill "$app" 2>/dev/null || true; kill "$xvfb" 2>/dev/null || true; }
trap cleanup EXIT
sleep 2
DISPLAY=$disp "$bin" "${args[@]}" >/dev/null 2>&1 &
app=$!
sleep "$wait_s"
# The app forces a redraw in --render-state mode; the nudge is a fallback (RR_NUDGE=0 skips it).
if [ "${RR_NUDGE:-1}" = 1 ]; then
DISPLAY=$disp xdotool mousemove 5 5
sleep 1
DISPLAY=$disp xdotool mousemove 600 400
fi
sleep 2
DISPLAY=$disp import -window root "$out"
