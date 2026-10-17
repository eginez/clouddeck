#!/usr/bin/env bash
# Render every named UI state at its key sizes with the real app under Xvfb + Lavapipe.
#   scripts/render-all.sh [outdir]      (default artifacts/render)
# Writes <outdir>/<State>-<W>x<H>.png (cropped to the window) and <outdir>/index.json:
#   [{state, size, shot, reference, note}]. A state that fails is noted 'failed: <reason>'.
# Builds nothing: expects target/debug/clouddeck or CLOUDDECK_BIN.
# Env: CLOUDDECK_BIN, RR_DISPLAY (:97), RR_SCREEN (1600x1000x24), RR_WAIT (10).
set -uo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-artifacts/render}
bin=${CLOUDDECK_BIN:-${CARGO_TARGET_DIR:-$root/target}/debug/clouddeck}
disp=${RR_DISPLAY:-:97}
screen=${RR_SCREEN:-1600x1000x24}
wait_s=${RR_WAIT:-10}
sw=${screen%%x*}; rest=${screen#*x}; sh=${rest%%x*}
mkdir -p "$out"

table='MainIdle 1280x760 04-main-window-no-selection.png
MainIdle 960x760 40-responsive-medium-960.png
MainIdle 700x760 41-responsive-narrow-700.png
MainIdle 520x760 44-responsive-compact-520.png
MainIdle 1280x480 45-responsive-short-480.png
MainRowSelected 1280x760 01-main-window-light.png
MainRowSelected 700x760 42-responsive-narrow-inspector-sheet.png
MainEmpty 1280x760 75-state-empty-fleet.png
MainFailure 1280x760 78-state-error.png
MainPartial 1280x760 70-state-several-regions-failed.png
MainLarge 1280x760 03-main-window-demo.png
SearchFocusedEmpty 1280x760 27-search-empty-recent.png
SearchTypingSt 1280x760 23-query-autocomplete-values.png
SearchError 1280x760 25-query-error-unknown-field.png
OverlayProfile 1280x760 30-picker-profile.png
OverlayRegion 1280x760 31-picker-region-multi.png
OverlayPalette 1280x760 33-command-palette.png
OverlayFailure 1280x760 32-failure-popover.png
OverlayRowMenu 1280x760 36-context-menu.png
Welcome 1280x760 60-welcome-window.png
SearchFocusedEmpty 520x760 -
SearchTypingSt 520x760 -
OverlayProfile 520x760 -
OverlayRegion 520x760 -
OverlayPalette 520x760 -
OverlayFailure 520x760 -
OverlayRowMenu 520x760 -'

Xvfb "$disp" -screen 0 "$screen" >/dev/null 2>&1 &
xvfb=$!
app=
cleanup() { [ -n "$app" ] && kill "$app" 2>/dev/null; kill "$xvfb" 2>/dev/null; wait 2>/dev/null; true; }
trap cleanup EXIT
sleep 2

entries=()
while read -r state size ref; do
  w=${size%x*}; h=${size#*x}
  shot="$out/$state-$size.png"
  refjson=null; [ "$ref" != "-" ] && refjson="\"design/images/$ref\""
  note=""
  rm -f "$shot"
  DISPLAY=$disp "$bin" --demo --render-state "$state" --size "$size" >/dev/null 2>&1 &
  app=$!
  sleep "$wait_s"
  if ! kill -0 "$app" 2>/dev/null; then
    note="failed: app exited early"
  else
    DISPLAY=$disp xdotool mousemove 5 5; sleep 1
    DISPLAY=$disp xdotool mousemove 600 400; sleep 2
    full="$out/.full.png"
    if ! DISPLAY=$disp import -window root "$full" 2>/dev/null; then
      note="failed: capture error"
    else
      x=$(( (sw - w) / 2 )); y=$(( (sh - h) / 2 ))
      convert "$full" -crop "${w}x${h}+${x}+${y}" +repage "$shot" 2>/dev/null || note="failed: crop error"
    fi
    rm -f "$full"
  fi
  kill "$app" 2>/dev/null; wait "$app" 2>/dev/null; app=
  if [ -n "$note" ]; then shot_json=null; else shot_json="\"$shot\""; fi
  entries+=("{\"state\":\"$state\",\"size\":\"$size\",\"shot\":$shot_json,\"reference\":$refjson,\"note\":\"$note\"}")
  echo "$state $size ${note:-ok}"
done <<< "$table"

{ echo "["; (IFS=,; printf '%s\n' "${entries[*]}" | sed 's/},{/},\n{/g'); echo "]"; } > "$out/index.json"
