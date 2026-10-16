#!/usr/bin/env bash
# Qt Console against a real engine, on a virtual display (no GPU needed):
# monitors and tiles show moving live pictures, clicks switch, no QML
# warnings. Needs Qt 6 with Kirigami, gst-plugin-qml6 (gstreamer1.0-qt6),
# Xvfb, xdotool, ImageMagick, jq.
#
#   scripts/test-console-qt.sh
#   KEEP_WORK=1 scripts/test-console-qt.sh   # keep logs and screenshots
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STROM_PORT=8799
CORE_PORT=8899
CORE="http://127.0.0.1:${CORE_PORT}/api"
DISPLAY_NO=:95
WORK="$(mktemp -d)"
# Unix socket paths must stay short (107 bytes).
LIVE_DIR="${XDG_RUNTIME_DIR:-/tmp}/lmix-qt-test"
PIDS=()

cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    rm -rf "$LIVE_DIR"
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT

pass() { echo "ok   $*"; }
fail() {
    echo "FAIL: $*"
    KEEP_WORK=1
    exit 1
}
act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null; }
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    fail "$2 did not come up"
}
state() { curl -sf "$CORE/state"; }
number_of() { state | jq -r --arg field "$1" '.[$field] as $id | .inputs[] | select(.id == $id) | .number'; }
# ImageMagick 7 (magick) or 6 (convert).
IM="$(command -v magick || command -v convert)"
shot() { DISPLAY="$DISPLAY_NO" import -window root "$WORK/$1.png"; }
# Standard deviation of a region of a screenshot (0 = flat colour).
spread() { "$IM" "$WORK/$1.png" -crop "$2" -colorspace gray -format '%[fx:standard_deviation]' info:; }
# Pixels that differ between two screenshots in a region.
moved() {
    "$IM" "$WORK/$1.png" -crop "$3" +repage "$WORK/m1.png"
    "$IM" "$WORK/$2.png" -crop "$3" +repage "$WORK/m2.png"
    compare -metric AE -fuzz 5% "$WORK/m1.png" "$WORK/m2.png" null: 2>&1 | awk '{ printf "%d", $1 }' || true
}
click() { DISPLAY="$DISPLAY_NO" xdotool mousemove "$1" "$2" click "${3:-1}"; }
# Waits up to 60 s until the input in `preview`/`program` has number $2
# (a busy VM handles clicks late).
becomes() {
    for _ in $(seq 1 120); do [[ "$(number_of "$1")" == "$2" ]] && return 0; sleep 0.5; done
}

STROM_BIN="$("$ROOT/scripts/fetch-strom.sh")"
(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
(cd "$ROOT/workspace/console-qt" && cargo build --release >/dev/null 2>&1) || fail "console-qt does not build"
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
CONSOLE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/console-qt/target}/release/lmix-console"

Xvfb "$DISPLAY_NO" -screen 0 1440x900x24 >/dev/null 2>&1 &
PIDS+=($!)
(cd "$WORK" && unset WAYLAND_DISPLAY && GST_GL_WINDOW=x11 DISPLAY="$DISPLAY_NO" exec "$STROM_BIN" \
    --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart --data-dir "$WORK/strom-data" \
    >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
"$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" --mixer cpu \
    --slots 2 --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" --live-dir "$LIVE_DIR" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core

act '{"action":"add_input","source":{"kind":"pattern","pattern":"ball"},"name":"Ball"}'
act '{"action":"add_input","source":{"kind":"pattern","pattern":"snow"},"name":"Snow"}'
act '{"action":"cut_direct","input":1}'
sleep 2

# The window opens at its default size (80 × 50 grid units of 18 px) at 0,0.
unset WAYLAND_DISPLAY
# Own settings: the Console writes style, keys and view settings on use,
# never into the operator's ~/.config/lmix/console.conf.
XDG_CONFIG_HOME="$WORK/config" DISPLAY="$DISPLAY_NO" QT_QPA_PLATFORM=xcb QT_FORCE_STDERR_LOGGING=1 \
    LMIX_CORE="http://127.0.0.1:${CORE_PORT}" "$CONSOLE_BIN" >"$WORK/console.log" 2>&1 &
PIDS+=($!)
PROGRAM_AREA="600x340+810+180"
PREVIEW_AREA="600x340+20+180"
TILE_1="200x110+12+660"
TILE_2="200x110+232+660"
# The live flows start a few seconds after the Console, and on a VM
# software GL shares four cores with the CPU mixer: pictures can stall
# for seconds. Each check needs to hold once within LIVE_WAIT seconds.
sleep 5
program_moves="" tile_1_moves="" tile_2_shows=""
deadline=$((SECONDS + ${LIVE_WAIT:-60}))
while ((SECONDS < deadline)); do
    shot a
    sleep 1
    shot b
    (($(moved a b "$PROGRAM_AREA") > 50)) && program_moves=1
    (($(moved a b "$TILE_1") > 10)) && tile_1_moves=1
    awk "BEGIN { exit !($(spread a "$TILE_2") > 0.05) }" && tile_2_shows=1
    [[ -n "$program_moves" && -n "$tile_1_moves" && -n "$tile_2_shows" ]] && break
done
[[ -n "$program_moves" ]] && pass "program monitor moves" || fail "program monitor still"
[[ -n "$tile_1_moves" ]] && pass "tile 1 moves (ball)" || fail "tile 1 still"
[[ -n "$tile_2_shows" ]] && pass "tile 2 shows its picture" || fail "tile 2 blank"

click 330 710
becomes preview 2
[[ "$(number_of preview)" == 2 ]] && pass "click on a tile: preview" || fail "preview is $(number_of preview), not 2"
# Cut acts on what the Console shows: wait until its preview has the picture.
for _ in $(seq 1 60); do
    shot p
    awk "BEGIN { exit !($(spread p "$PREVIEW_AREA") > 0.05) }" && break
    sleep 1
done
click 720 220
becomes program 2
[[ "$(number_of program)" == 2 ]] && pass "Cut: program" || fail "program is $(number_of program) after Cut"
shot c
awk "BEGIN { exit !($(spread c "$PROGRAM_AREA") > 0.05) }" && pass "program monitor shows the new input" \
    || fail "program monitor blank after Cut"

if grep -E 'qrc:|\.qml:[0-9]+' "$WORK/console.log" >/dev/null; then
    fail "QML warnings: $(grep -E 'qrc:|\.qml:[0-9]+' "$WORK/console.log" | head -3)"
fi
pass "no QML warnings"
criticals="$(grep -c 'GStreamer-CRITICAL' "$WORK/strom.log" || true)"
[[ "$criticals" == 0 ]] || fail "$criticals GStreamer CRITICALs in strom.log"
pass "no GStreamer CRITICALs (live flows)"
echo "Qt Console check passed"
