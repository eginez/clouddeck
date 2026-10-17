#!/usr/bin/env bash
# Screenshot the real clouddeck app (demo mode) under Xvfb + Lavapipe.
#   scripts/render-shot.sh <happy|empty|failure|partial|large> <out.png>
# Env: CLOUDDECK_BIN (default $CARGO_TARGET_DIR|target/debug/clouddeck),
#      RR_DISPLAY (default :99), RR_SCREEN (1600x1000x24), RR_WAIT (seconds, 10).
# Needs the blade-graphics patch (Cargo.toml [patch.crates-io], bead
# clouddeck-rs-sr7i.1) or the window stays black. See brain/reference/real-render.md.
set -euo pipefail
scenario=${1:?usage: render-shot.sh <scenario> <out.png>}
out=${2:?usage: render-shot.sh <scenario> <out.png>}
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
DISPLAY=$disp "$bin" --demo --demo-scenario "$scenario" >/dev/null 2>&1 &
app=$!
sleep "$wait_s"
# The first frame is only presented after an input event; nudge the pointer.
DISPLAY=$disp xdotool mousemove 5 5
sleep 1
DISPLAY=$disp xdotool mousemove 600 400
sleep 2
DISPLAY=$disp import -window root "$out"
