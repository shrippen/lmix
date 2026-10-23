#!/usr/bin/env bash
# lmix against a real engine, on a virtual display (no GPU needed): on the
# Mix page monitors and tiles show moving live pictures, clicks switch, no
# QML warnings. Then the Show page through the page bar: it draws, space is
# GO and Escape panic (also after a click on the header and a closed
# dialog), the page bar shows program small, no QML warnings. Then Audio,
# Outputs and Setup draw without QML warnings. Needs Qt 6 with Kirigami, gst-plugin-qml6
# (gstreamer1.0-qt6), Xvfb, xdotool, ImageMagick, jq.
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
# QML warnings in a log, except Qt's file dialog in the Breeze style
# (warns since qqc2-breeze-style 6.8.1, 22.10.): printed as a note, not
# ours to fix. Kirigami and lmix's own files still fail the check.
QT_KDE_QML='qrc:/qt-project\.org/imports/QtQuick/Dialogs/|qrc:/qt/qml/org/kde/breeze/'
own_qml_warnings() {
    local found
    found="$(grep -E 'qrc:|\.qml:[0-9]+' "$WORK/$1" || true)"
    if grep -qE "$QT_KDE_QML" <<<"$found"; then
        echo "note: $1 has QML warnings from Qt's file dialog in Breeze: $(grep -E "$QT_KDE_QML" <<<"$found" | sort -u | head -2 | tr '\n' ' ')" >&2
    fi
    grep -vE "$QT_KDE_QML" <<<"$found" | grep .
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
CONSOLE_DIR="${CARGO_TARGET_DIR:-$ROOT/workspace/console-qt/target}/release"

Xvfb "$DISPLAY_NO" -screen 0 1440x900x24 >/dev/null 2>&1 &
PIDS+=($!)
(cd "$WORK" && unset WAYLAND_DISPLAY && GST_GL_WINDOW=x11 DISPLAY="$DISPLAY_NO" exec "$STROM_BIN" \
    --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart --data-dir "$WORK/strom-data" \
    >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
# strom's own start (device discovery of this machine may warn) is not
# what this check is about: only lines from here on count.
STROM_START_LINES="$(wc -l <"$WORK/strom.log")"
"$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" --mixer cpu \
    --slots 2 --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" --layout-templates "$WORK/layout-templates.json" --live-dir "$LIVE_DIR" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core

act '{"action":"add_input","source":{"kind":"pattern","pattern":"ball"},"name":"Ball"}'
act '{"action":"add_input","source":{"kind":"pattern","pattern":"snow"},"name":"Snow"}'
act '{"action":"cut_direct","input":1}'
sleep 2

# The window opens at its default size (80 × 50 grid units of 18 px) at 0,0.
unset WAYLAND_DISPLAY
# Own settings: lmix writes style, keys, view and page settings on use,
# never into the operator's ~/.config/lmix/lmix.conf.
export XDG_CONFIG_HOME="$WORK/config" DISPLAY="$DISPLAY_NO" QT_QPA_PLATFORM=xcb QT_FORCE_STDERR_LOGGING=1 \
    LMIX_CORE="http://127.0.0.1:${CORE_PORT}"
"$CONSOLE_DIR/lmix" --page mix >"$WORK/console.log" 2>&1 &
PIDS+=($!)
PROGRAM_AREA="560x300+820+140"
PREVIEW_AREA="560x300+40+140"
TILE_1="200x50+12+540"
TILE_2="200x50+232+540"
# The tile's picture is about 78×44 px: the ball is 2–3 px wide and a
# move changes about 9 px (19.10.–22.10.: 8.9 at fuzz 5 %). A still
# picture changes none.
TILE_MOVED_PX=3
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
    (($(moved a b "$TILE_1") > TILE_MOVED_PX)) && tile_1_moves=1
    awk "BEGIN { exit !($(spread a "$TILE_2") > 0.05) }" && tile_2_shows=1
    [[ -n "$program_moves" && -n "$tile_1_moves" && -n "$tile_2_shows" ]] && break
done
[[ -n "$program_moves" ]] && pass "program monitor moves" || fail "program monitor still"
[[ -n "$tile_1_moves" ]] && pass "tile 1 moves (ball)" || fail "tile 1 still"
[[ -n "$tile_2_shows" ]] && pass "tile 2 shows its picture" || fail "tile 2 blank"

click 336 566
becomes preview 2
[[ "$(number_of preview)" == 2 ]] && pass "click on a tile: preview" || fail "preview is $(number_of preview), not 2"
# Cut acts on what the Console shows: wait until its preview has the picture.
for _ in $(seq 1 60); do
    shot p
    awk "BEGIN { exit !($(spread p "$PREVIEW_AREA") > 0.05) }" && break
    sleep 1
done
click 720 128
becomes program 2
[[ "$(number_of program)" == 2 ]] && pass "Cut: program" || fail "program is $(number_of program) after Cut"
# Two streams, not started: the output bar offers to start both (STR.1).
for port in 9921 9922; do
    curl -sf -X POST "$CORE/actions" -H 'content-type: application/json' \
        -d "{\"action\":\"add_output\",\"spec\":{\"kind\":\"srt\",\"uri\":\"srt://:$port?mode=listener\"},\"name\":\"SRT $port\"}" >/dev/null
done
# The monitor follows the state a little later (live picture).
program_shows=""
for _ in $(seq 1 20); do
    shot c
    awk "BEGIN { exit !($(spread c "$PROGRAM_AREA") > 0.05) }" && program_shows=1 && break
    sleep 0.5
done
[[ -n "$program_shows" ]] && pass "program monitor shows the new input" || fail "program monitor blank after Cut"

# A layout on preview: the monitor shows the mixer's preview (VSP), which
# composes it; the input-only monitor before had nothing to show. Snow in
# the top right quarter of the picture (grey on average, noisy), where
# Ball alone is black and an empty monitor flat beige.
curl -sf -X POST "$CORE/actions" -H 'content-type: application/json' \
    -d '{"action":"add_layout","name":"Corner","background":1,"boxes":[{"rect":{"x":0.5,"y":0.0,"w":0.5,"h":0.5},"input":2}]}' >/dev/null
curl -sf -X POST "$CORE/actions" -H 'content-type: application/json' \
    -d '{"action":"preview_layout","layout":1}' >/dev/null
layout_id="$(state | jq -r '.layouts[0].id')"
for _ in $(seq 1 40); do [[ "$(state | jq -r .preview)" == "$layout_id" ]] && break; sleep 0.5; done
PREVIEW_CORNER="250x130+340+140"
layout_shows=""
for _ in $(seq 1 40); do
    shot l
    # The format ends in a newline: without it `read` returns 1 at the
    # end of input, and set -e ends the script without a word.
    read -r mean sd < <("$IM" "$WORK/l.png" -crop "$PREVIEW_CORNER" -colorspace gray \
        -format '%[fx:mean] %[fx:standard_deviation]\n' info:)
    awk "BEGIN { exit !($mean > 0.25 && $mean < 0.75 && $sd > 0.05) }" && layout_shows=1 && break
    sleep 0.5
done
[[ -n "$layout_shows" ]] && pass "preview monitor shows a layout" || fail "preview monitor blank with a layout on preview"

own_qml_warnings console.log && fail "QML warnings: $(own_qml_warnings console.log | head -3)"
pass "no QML warnings"

# The Show page through the page bar, same window: it draws the show
# view (not a flat colour), and the bar shows program small.
act '{"action":"add_cue_list","name":"Act 1","cues":[{"number":"1","name":"Doors","kind":"wait","duration_ms":120000},{"number":"2","name":"House","kind":"wait","duration_ms":120000},{"number":"3","name":"Lights","kind":"note"}]}'
show_window="$(xdotool search --name '^lmix' 2>/dev/null | head -1 || true)"
[[ -n "$show_window" ]] || fail "no lmix window"
SHOW_TAB="173 868"
click $SHOW_TAB
sleep 3
shot show
awk "BEGIN { exit !($(spread show 1400x700+20+60) > 0.05) }" && pass "Show page draws" || fail "Show page blank"
PGM_SMALL="70x40+1210+830"
small_shows=""
for _ in $(seq 1 20); do
    shot s
    awk "BEGIN { exit !($(spread s "$PGM_SMALL") > 0.02) }" && small_shows=1 && break
    sleep 0.5
done
[[ -n "$small_shows" ]] && pass "page bar shows program small on the Show page" || fail "no program in the page bar"
# No window manager: give the show window the keyboard; keys then go to
# whatever has it (a dialog the show opens takes it over).
standby() { state | jq '.cue_lists[0].standby'; }
cues_running() { state | jq '.running_cues | length'; }
key() { xdotool key "$1"; }
xdotool windowfocus "$show_window"
sleep 1
until_true() { for _ in $(seq 1 20); do eval "$1" && return 0; sleep 0.5; done; return 1; }
key space
until_true '[[ "$(standby)" == 2 && "$(cues_running)" -ge 1 ]]' && pass "show: space is GO" \
    || fail "show: space did not GO (standby $(standby), running $(cues_running))"
# A header button and its dialog (Keys, closed with Escape) must not
# keep the keys: Escape closes the dialog, the cue keeps running.
KEYS_BUTTON="1334 24"
click $KEYS_BUTTON
sleep 2
key Escape
sleep 1
[[ "$(cues_running)" -ge 1 ]] || fail "show: Escape in the Keys dialog reached the show (panic)"
key space
until_true '[[ "$(standby)" == 3 ]]' && pass "show: space is GO after a header button and its dialog" \
    || fail "show: no GO after the Keys dialog (standby $(standby))"
key Escape
until_true '[[ "$(cues_running)" == 0 ]]' && pass "show: Escape is panic" \
    || fail "show: Escape did not stop the cues ($(cues_running) running)"
own_qml_warnings console.log && fail "QML warnings (show): $(own_qml_warnings console.log | head -3)"
pass "no QML warnings (show)"

# The other pages through the page bar: each draws something of its own
# (a light page is flat, so: it differs from the page before), no QML
# warnings.
prev=show
for page in "AUDIO 286" "OUTPUTS 397" "SETUP 510"; do
    read -r name x <<<"$page"
    click "$x" 868
    sleep 2
    shot "page-$name"
    (($(moved "$prev" "page-$name" 1400x700+20+60) > 5000)) && pass "$name page draws" || fail "$name page shows the page before"
    prev="page-$name"
done
own_qml_warnings console.log && fail "QML warnings (pages): $(own_qml_warnings console.log | head -3)"
pass "no QML warnings (Audio, Outputs, Setup)"
criticals="$(tail -n +"$((STROM_START_LINES + 1))" "$WORK/strom.log" | grep -c 'GStreamer-CRITICAL' || true)"
[[ "$criticals" == 0 ]] || fail "$criticals GStreamer CRITICALs in strom.log"
pass "no GStreamer CRITICALs (live flows)"
echo "lmix Mix and Show check passed"
