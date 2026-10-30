#!/usr/bin/env bash
# lmix against a real engine, on a virtual display (no GPU needed): on the
# Mix page monitors and tiles show moving live pictures, clicks switch, no
# QML warnings, the overlay grid and the narrow mixer are in reach. Then the Show page through the page bar: it draws, space is
# GO and Escape panic (also after a click on the header and a closed
# dialog), the page bar shows program small, the inspector renames a cue,
# no QML warnings. Then Audio,
# Outputs and Setup draw without QML warnings, Audio with a bus. Needs Qt 6 with Kirigami, gst-plugin-qml6
# (gstreamer1.0-qt6), Xvfb, xdotool, ImageMagick, jq.
#
#   scripts/test-console-qt.sh
#   KEEP_WORK=1 scripts/test-console-qt.sh   # keep logs and screenshots
#   DISPLAY_NO=:96 STROM_PORT=8798 CORE_PORT=8898 scripts/test-console-qt.sh
#
# Clicks find their control by its accessible name (scripts/a11y.py,
# needs at-spi2-core and PyGObject) where a row may gain a button, else
# by pixel. A failure keeps a screenshot and the named controls.
set -euo pipefail

# In a D-Bus session of its own: lmix on an accessibility bus of its
# own, apart from the desktop's.
if [[ -z "${LMIX_TEST_SESSION:-}" ]]; then
    LMIX_TEST_SESSION=1 exec dbus-run-session --config-file="$(dirname "$0")/test-session.conf" -- "$0" "$@"
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Display and ports can be moved, so two runs on one machine stay apart.
STROM_PORT="${STROM_PORT:-8799}"
CORE_PORT="${CORE_PORT:-8899}"
CORE="http://127.0.0.1:${CORE_PORT}/api"
DISPLAY_NO="${DISPLAY_NO:-:95}"
WORK="$(mktemp -d)"
# Unix socket paths must stay short (107 bytes).
LIVE_DIR="${XDG_RUNTIME_DIR:-/tmp}/lmix-qt-test${DISPLAY_NO#:}"
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
    # What the screen showed when the check gave up.
    shot failed 2>/dev/null || true
    DISPLAY="$DISPLAY_NO" python3 "$ROOT/scripts/a11y.py" dump >"$WORK/failed-controls.txt" 2>&1 || true
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
# Clicks the showing control whose accessible name matches $1 (a regex:
# give the English and the German name), `-3` first for the right
# button; further arguments go to `a11y.py find` (--role, --within, --nth).
click_on() {
    local button=1 at
    [[ "$1" == -3 ]] && button=3 && shift
    at="$(DISPLAY="$DISPLAY_NO" python3 "$ROOT/scripts/a11y.py" find "$@")" || fail "no control named /$1/ on the screen"
    click "${at% *}" "${at#* }" "$button"
}
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
/usr/libexec/at-spi-bus-launcher --launch-immediately >/dev/null 2>&1 &
PIDS+=($!)
sleep 0.5
/usr/libexec/at-spi2-registryd >/dev/null 2>&1 &
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
    --slots 3 --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" --layout-templates "$WORK/layout-templates.json" --live-dir "$LIVE_DIR" >"$WORK/core.log" 2>&1 &
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
    QT_LINUX_ACCESSIBILITY_ALWAYS_ON=1 LMIX_CORE="http://127.0.0.1:${CORE_PORT}"
"$CONSOLE_DIR/lmix" --page mix >"$WORK/console.log" 2>&1 &
PIDS+=($!)
# Regions inside the pictures, from where the monitors and tiles are on
# the screen (their boxes less the label rows at the edges).
box_of() {
    DISPLAY="$DISPLAY_NO" python3 "$ROOT/scripts/a11y.py" find "$1" --box "$2" --wait 60
}
PROGRAM_AREA="$(box_of "(Program|Programm): .*" 0.15)" || fail "no program monitor on the screen"
PREVIEW_AREA="$(box_of "(Preview|Vorschau)(: .*)?" 0.15)" || fail "no preview monitor on the screen"
TILE_1="$(box_of "Input 1 .*" 0.25)" || fail "no tile of input 1 on the screen"
TILE_2="$(box_of "Input 2 .*" 0.25)" || fail "no tile of input 2 on the screen"
# The tile's picture (medium, compact) is about 186×104 px: the ball is
# a few px wide and a move changes some px. A still picture changes none.
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

click_on "Input 2 .*"
becomes preview 2
[[ "$(number_of preview)" == 2 ]] && pass "click on a tile: preview" || fail "preview is $(number_of preview), not 2"
# Cut acts on what the Console shows: wait until its preview has the picture.
for _ in $(seq 1 60); do
    shot p
    awk "BEGIN { exit !($(spread p "$PREVIEW_AREA") > 0.05) }" && break
    sleep 1
done
click_on "Cut" --role button
becomes program 2
[[ "$(number_of program)" == 2 ]] && pass "Cut: program" || fail "program is $(number_of program) after Cut"
# The overlay grid beside the monitors is reachable at 1440 x 900 without
# scrolling (#137): channel 1 shows preview, "All off" takes it off.
act '{"action":"preview_input","input":1}'
becomes preview 1
overlay_1() { state | jq -r '.overlays[0] // "none"'; }
click_on "Overlay 1: .*" --role button
for _ in $(seq 1 40); do [[ "$(overlay_1)" != none ]] && break; sleep 0.5; done
[[ "$(overlay_1)" != none ]] && pass "overlay 1 from the grid: on" || fail "overlay 1 not on after a click in the grid (hidden or moved?)"
# Away first: the tooltip of channel 1 opens above it, over "All off".
DISPLAY="$DISPLAY_NO" xdotool mousemove 1000 300
sleep 1
click_on "All overlays off|Alle Overlays aus" --role button
for _ in $(seq 1 40); do [[ "$(overlay_1)" == none ]] && break; sleep 0.5; done
[[ "$(overlay_1)" == none ]] && pass "overlays: All off" || fail "overlay 1 still on after All off"
# The audio column at the right edge (inputs on top, the ways out below):
# mute input 1 and the master, solo input 1 and end it, take input 1 off
# the headphones, add a bus.
audio_1() { state | jq '.inputs[0].audio.on'; }
master_mute() { state | jq '.master.mute'; }
listen_1() { state | jq -r '.inputs[0].listen'; }
phones_1() { state | jq '.inputs[0].phones.on'; }
buses() { state | jq '.buses | length'; }
click 1231 518
for _ in $(seq 1 40); do [[ "$(audio_1)" == false ]] && break; sleep 0.5; done
[[ "$(audio_1)" == false ]] && pass "audio column: input 1 muted" || fail "input 1 still on after M in the audio column"
sleep 1 # not a double click on the same M
click 1231 518
for _ in $(seq 1 40); do [[ "$(audio_1)" == true ]] && break; sleep 0.5; done
click 1231 827
for _ in $(seq 1 40); do [[ "$(master_mute)" == true ]] && break; sleep 0.5; done
[[ "$(audio_1)" == true && "$(master_mute)" == true ]] && pass "audio column: input 1 on, master muted" \
    || fail "audio column: input 1 on $(audio_1), master mute $(master_mute)"
click 1231 827
for _ in $(seq 1 40); do [[ "$(master_mute)" == false ]] && break; sleep 0.5; done
click 1256 518
for _ in $(seq 1 40); do [[ "$(listen_1)" != off ]] && break; sleep 0.5; done
[[ "$(listen_1)" != off ]] && pass "audio column: S solos input 1" || fail "input 1 listen $(listen_1) after S"
sleep 1 # "Solo off" shows once the Console has the new state
click_on "Solo off|Solo aus" --role button
for _ in $(seq 1 40); do [[ "$(listen_1)" == off ]] && break; sleep 0.5; done
[[ "$(listen_1)" == off ]] && pass "audio column: Solo off ends it" || fail "input 1 listen $(listen_1) after Solo off"
click 1231 555
for _ in $(seq 1 40); do [[ "$(phones_1)" == false ]] && break; sleep 0.5; done
[[ "$(phones_1)" == false ]] && pass "audio column: PH takes input 1 off the headphones" \
    || fail "input 1 on the headphones $(phones_1) after PH"
sleep 1
click 1231 555
for _ in $(seq 1 40); do [[ "$(phones_1)" == true ]] && break; sleep 0.5; done
[[ "$(phones_1)" == true ]] || fail "input 1 not back on the headphones after a second PH"
click 1400 630
for _ in $(seq 1 40); do [[ "$(buses)" == 1 ]] && break; sleep 0.5; done
[[ "$(buses)" == 1 ]] && pass "audio column: + Bus adds a bus" || fail "$(buses) buses after + Bus"
# Its name opens its menu with the name to edit: Enter renames it to the
# whole name typed (not to the short form the strip shows).
sleep 1
click 1348 751
sleep 1
DISPLAY="$DISPLAY_NO" xdotool key ctrl+a type "Stream Mix"
DISPLAY="$DISPLAY_NO" xdotool key Return
bus_name() { state | jq -r '.buses[0].name'; }
for _ in $(seq 1 40); do [[ "$(bus_name)" == "Stream Mix" ]] && break; sleep 0.5; done
sleep 1
[[ "$(bus_name)" == "Stream Mix" ]] && pass "audio column: Enter in the name field renames the bus" \
    || fail "bus named '$(bus_name)' after typing Stream Mix and Enter"
# Esc drops what was typed; afterwards no field has the keys.
click 1348 751
sleep 1
DISPLAY="$DISPLAY_NO" xdotool key ctrl+a type "Weg"
DISPLAY="$DISPLAY_NO" xdotool key Escape
sleep 1
[[ "$(bus_name)" == "Stream Mix" ]] && pass "audio column: Esc in the bus menu keeps the name" \
    || fail "bus named '$(bus_name)' after Esc in the bus menu"
# Opened from the name and closed, the menu leaves the show its keys (a
# field in a closed menu would keep them). Esc closes it: on Xvfb a press
# outside does not close a popover reliably (the View popover on main too).
act '{"action":"add_shortcut","shortcut":{"label":"Input 1 off","key":"2","actions":[{"action":"set_audio","input":1,"on":false}]}}'
click 1348 751
sleep 1
DISPLAY="$DISPLAY_NO" xdotool key Escape
sleep 1
DISPLAY="$DISPLAY_NO" xdotool key 2
for _ in $(seq 1 40); do [[ "$(audio_1)" == false ]] && break; sleep 0.5; done
[[ "$(audio_1)" == false && "$(bus_name)" == "Stream Mix" ]] && pass "audio column: after the bus menu the show has the keys" \
    || fail "after the bus menu: input 1 on $(audio_1), bus named '$(bus_name)'"
act '{"action":"remove_shortcut","target":"Input 1 off"}'
act '{"action":"set_audio","input":1,"on":true}'
# Renamed elsewhere while its menu is open: closing it (its ⋯ again)
# keeps the new name.
click 1360 793
sleep 1
act "{\"action\":\"rename_bus\",\"bus\":\"$(state | jq -r '.buses[0].id')\",\"name\":\"Stream B\"}"
for _ in $(seq 1 40); do [[ "$(bus_name)" == "Stream B" ]] && break; sleep 0.5; done
sleep 1 # the Console has the new state
click 1360 793
sleep 1
[[ "$(bus_name)" == "Stream B" ]] && pass "audio column: closing the bus menu keeps a name given elsewhere" \
    || fail "bus named '$(bus_name)' after a rename elsewhere and closing its menu"
act "{\"action\":\"rename_bus\",\"bus\":\"$(state | jq -r '.buses[0].id')\",\"name\":\"Stream Mix\"}"
# The bus's M, its S and "Solo off", its tap in the ⋯ menu.
bus_mute() { state | jq '.buses[0].mute'; }
bus_listen() { state | jq '.buses[0].listen'; }
bus_pre() { state | jq '.buses[0].pre_fader'; }
click 1335 793
for _ in $(seq 1 40); do [[ "$(bus_mute)" == true ]] && break; sleep 0.5; done
[[ "$(bus_mute)" == true ]] && pass "audio column: M mutes the bus" || fail "bus mute $(bus_mute) after its M"
sleep 1
click 1335 793
for _ in $(seq 1 40); do [[ "$(bus_mute)" == false ]] && break; sleep 0.5; done
click 1347 827
for _ in $(seq 1 40); do [[ "$(bus_listen)" == true ]] && break; sleep 0.5; done
[[ "$(bus_listen)" == true ]] && pass "audio column: S listens to the bus" || fail "bus listen $(bus_listen) after its S"
sleep 1
click_on "Solo off|Solo aus" --role button
for _ in $(seq 1 40); do [[ "$(bus_listen)" == false ]] && break; sleep 0.5; done
[[ "$(bus_listen)" == false ]] && pass "audio column: Solo off ends listening to the bus" \
    || fail "bus listen $(bus_listen) after Solo off"
click 1360 793
sleep 1
# By name: the menu's layout moves (P02).
click_on "Before the faders|Vor den Fadern" --role "check box"
for _ in $(seq 1 40); do [[ "$(bus_pre)" == true ]] && break; sleep 0.5; done
[[ "$(bus_pre)" == true ]] && pass "audio column: the bus menu takes the bus before the faders" \
    || fail "bus pre_fader $(bus_pre) after the switch in its menu"
DISPLAY="$DISPLAY_NO" xdotool key Escape
sleep 1
# Its send in input 1's menu: a drag goes on over the states it brings
# (one step only, if the rows were built anew with every state).
click 1256 555
sleep 1
send_1() { state | jq --arg bus "$(state | jq -r '.buses[0].id')" '.inputs[0].sends[$bus] // 0'; }
# From the slider's left end (its handle at 0) to past its middle; found
# by name, as the menu's layout moves (P02).
box="$(DISPLAY="$DISPLAY_NO" python3 "$ROOT/scripts/a11y.py" find "Send to Stream Mix|Send an Stream Mix" --role slider --box 0)" \
    || fail "no send slider in input 1's menu"
IFS='x+' read -r sw sh sx sy <<<"$box"
sy=$((sy + sh / 2))
DISPLAY="$DISPLAY_NO" xdotool mousemove $((sx + 4)) "$sy"
sleep 0.3
DISPLAY="$DISPLAY_NO" xdotool mousedown 1
for x in $(seq $((sx + 8)) 5 $((sx + sw * 3 / 4))); do sleep 0.05; DISPLAY="$DISPLAY_NO" xdotool mousemove "$x" "$sy"; done
sleep 0.1
DISPLAY="$DISPLAY_NO" xdotool mouseup 1
for _ in $(seq 1 20); do (( $(send_1) >= 50 )) && break; sleep 0.5; done
(( $(send_1) >= 50 )) && pass "audio column: a send drag in the menu goes on" \
    || fail "send of input 1 at $(send_1) after a drag across the menu's slider"
DISPLAY="$DISPLAY_NO" xdotool key Escape
# An output gets the bus's sound from the bus menu (switched on), and its
# source's own back (off).
act '{"action":"add_output","spec":{"kind":"srt","uri":"srt://:9929?mode=listener"},"name":"Bus out"}'
out_bus() { state | jq -r '.outputs[0].bus // "none"'; }
sleep 1
click 1360 793
sleep 1
click_on "Bus out" --role "check box"
for _ in $(seq 1 40); do [[ "$(out_bus)" != none ]] && break; sleep 0.5; done
[[ "$(out_bus)" == "$(state | jq -r '.buses[0].id')" ]] && pass "audio column: the bus menu gives an output its sound" \
    || fail "output bus $(out_bus) after ticking it in the bus menu"
sleep 1
click_on "Bus out" --role "check box"
for _ in $(seq 1 40); do [[ "$(out_bus)" == none ]] && break; sleep 0.5; done
[[ "$(out_bus)" == none ]] || fail "output bus $(out_bus) after unticking it"
DISPLAY="$DISPLAY_NO" xdotool key Escape
act '{"action":"remove_output","output":"Bus out"}'
# The Audio page adds its own further on.
act "{\"action\":\"remove_bus\",\"bus\":\"$(state | jq -r '.buses[0].id')\"}"
for _ in $(seq 1 40); do [[ "$(buses)" == 0 ]] && break; sleep 0.5; done
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
# The top right quarter of the preview's picture, where the layout has input 2.
read -r pw ph px py < <(sed 's/[x+]/ /g' <<<"$PREVIEW_AREA")
PREVIEW_CORNER="$((pw / 2 - 20))x$((ph / 2 - 20))+$((px + pw / 2 + 10))+$((py + 10))"
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

# View as a popover: its switch sets the multiview's program side. Before
# an input is removed: its "Bring back" widens the tools to two rows.
multiview_program() { state | jq -r '.multiview.program // "right"'; }
click_on "View|Ansicht" --role button
sleep 1
click_on "Program on the left|Programm links"
for _ in $(seq 1 40); do [[ "$(multiview_program)" == left ]] && break; sleep 0.5; done
[[ "$(multiview_program)" == left ]] && pass "View popover: program on the left" \
    || fail "View popover: multiview program $(multiview_program) after its switch"
DISPLAY="$DISPLAY_NO" xdotool key Escape
act '{"action":"set_multiview","layout":{"program":"right","meters":"shown"}}'

# "+ Input": the dialog opens on the test pattern; Ball, a name and the
# footer's "Add input" give an input of that pattern and name.
click_on "\\+ Input" --role button
sleep 2
click 836 262
click 940 320
DISPLAY="$DISPLAY_NO" xdotool type --delay 30 "Dialog Ball"
click 920 746
added() { state | jq -r '.inputs[] | select(.name == "Dialog Ball") | .source.pattern'; }
for _ in $(seq 1 40); do [[ -n "$(added)" ]] && break; sleep 0.5; done
[[ "$(added)" == ball ]] && pass "add input: a pattern from the dialog" \
    || fail "add input: got '$(added)' for the pattern of \"Dialog Ball\""
act "{\"action\":\"remove_input\",\"input\":$(state | jq '.inputs[] | select(.name == "Dialog Ball") | .number')}"
for _ in $(seq 1 40); do [[ -z "$(added)" ]] && break; sleep 0.5; done
[[ -z "$(added)" ]] || fail "add input: \"Dialog Ball\" not removed again"

# File keys: Ctrl+S saves a show that has a path (the menu shows the key).
act "{\"action\":\"save_show\",\"path\":\"$WORK/keys.lmix\"}"
for _ in $(seq 1 20); do [[ -f "$WORK/keys.lmix" ]] && break; sleep 0.5; done
# The window learns the path with the next state: before it, Ctrl+S asks for one.
for _ in $(seq 1 20); do [[ "$(state | jq -r .path)" == "$WORK/keys.lmix" ]] && break; sleep 0.5; done
sleep 1
rm -f "$WORK/keys.lmix"
DISPLAY="$DISPLAY_NO" xdotool windowfocus "$(DISPLAY="$DISPLAY_NO" xdotool search --name '^lmix' 2>/dev/null | head -1)"
DISPLAY="$DISPLAY_NO" xdotool key ctrl+s
for _ in $(seq 1 20); do [[ -f "$WORK/keys.lmix" ]] && break; sleep 0.5; done
[[ -f "$WORK/keys.lmix" ]] && pass "Ctrl+S saves the show" || fail "Ctrl+S: no file saved"

# The input's menu asks before it removes (Enter keeps the input, the red
# button removes); Ctrl+Z while the toast shows brings the input back. A
# spare third input, so the tiles of the checks below stay.
confirm_remove() {
    sleep 1.5
    click_on "Remove input|Input entfernen" --role button
}
act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Spare"}'
spare() { state | jq -r '.inputs[] | select(.name == "Spare") | .number'; }
for _ in $(seq 1 40); do [[ -n "$(spare)" ]] && break; sleep 0.5; done
sleep 2
# The menu's quick row: Preview closes the menu and puts Spare into preview.
spare_id() { state | jq -r '.inputs[] | select(.name == "Spare") | .id'; }
click_on -3 "Input 3 .*"
sleep 1.5
click_on "Preview|Vorschau" --role button
for _ in $(seq 1 20); do [[ "$(state | jq -r .preview)" == "$(spare_id)" ]] && break; sleep 0.5; done
[[ "$(state | jq -r .preview)" == "$(spare_id)" ]] && pass "input menu: Preview in the quick row" \
    || fail "input menu: preview is $(state | jq -r .preview) after Preview in the quick row"
sleep 1
click_on -3 "Input 3 .*"
sleep 1.5
click_on "Remove|Entfernen"
sleep 1.5
DISPLAY="$DISPLAY_NO" xdotool key Return
sleep 2
[[ -n "$(spare)" ]] && pass "remove dialog: Enter keeps the input" || fail "remove dialog: Enter removed Spare"
click_on -3 "Input 3 .*"
sleep 1.5
click_on "Remove|Entfernen"
confirm_remove
for _ in $(seq 1 40); do [[ -z "$(spare)" ]] && break; sleep 0.5; done
[[ -z "$(spare)" ]] && pass "input menu: Remove" || fail "input menu: Spare still there after Remove"
# The toast waits for the Console's state without Spare, a moment after
# the core's.
sleep 1
DISPLAY="$DISPLAY_NO" xdotool key ctrl+z
for _ in $(seq 1 40); do [[ -n "$(spare)" ]] && break; sleep 0.5; done
[[ -n "$(spare)" ]] && pass "input menu: Ctrl+Z brings it back" || fail "input menu: Spare not back after Ctrl+Z"
act "{\"action\":\"remove_input\",\"input\":$(spare)}"
for _ in $(seq 1 40); do [[ -z "$(spare)" ]] && break; sleep 0.5; done
# A refused removal (locked show) is forgotten: when the input goes
# another way later, no toast offers Undo for it. Before the show's Ctrl+Z
# below: no toast is open here (Ctrl+Z above took the last one).
act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Spare"}'
for _ in $(seq 1 40); do [[ -n "$(spare)" ]] && break; sleep 0.5; done
# Lock and unlock with the bar's button: the Console's own unlock clears
# the refusal's line, which would move the tiles below.
locked() { state | jq -r '.locked // false'; }
sleep 1
click_on "Lock|Sperren" --role button
for _ in $(seq 1 20); do [[ "$(locked)" == true ]] && break; sleep 0.5; done
[[ "$(locked)" == true ]] || fail "Lock button: show not locked"
# Twice: the same refusal again sets no new message, its answer counts.
for _ in 1 2; do
    click_on -3 "Input 3 .*"
    sleep 1.5
    click_on "Remove|Entfernen"
    confirm_remove
    sleep 1.5
done
click_on "Locked|Gesperrt" --role button
for _ in $(seq 1 20); do [[ "$(locked)" == false ]] && break; sleep 0.5; done
[[ "$(locked)" == false ]] || fail "Lock button: show still locked"
act "{\"action\":\"remove_input\",\"input\":$(spare)}"
for _ in $(seq 1 40); do [[ -z "$(spare)" ]] && break; sleep 0.5; done
sleep 1
DISPLAY="$DISPLAY_NO" xdotool key ctrl+z
sleep 2
[[ -z "$(spare)" ]] && pass "input menu: a refused removal is forgotten" \
    || fail "input menu: Ctrl+Z brought back Spare, removed elsewhere after a refused removal"
# A show's shortcut on Ctrl+Z wins over the toast's Undo.
act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Spare"}'
for _ in $(seq 1 40); do [[ -n "$(spare)" ]] && break; sleep 0.5; done
act '{"action":"add_shortcut","shortcut":{"label":"Program left","key":"Ctrl+Z","actions":[{"action":"set_multiview","layout":{"program":"left","meters":"shown"}}]}}'
sleep 1.5
click_on -3 "Input 3 .*"
sleep 1.5
click_on "Remove|Entfernen"
confirm_remove
for _ in $(seq 1 40); do [[ -z "$(spare)" ]] && break; sleep 0.5; done
sleep 1
DISPLAY="$DISPLAY_NO" xdotool key ctrl+z
for _ in $(seq 1 20); do [[ "$(multiview_program)" == left ]] && break; sleep 0.5; done
[[ "$(multiview_program)" == left && -z "$(spare)" ]] \
    && pass "input menu: a show's Ctrl+Z wins over Undo" \
    || fail "input menu: Ctrl+Z ran the toast's Undo, not the show's shortcut"
act '{"action":"remove_shortcut","target":"Program left"}'
act '{"action":"set_multiview","layout":{"program":"right","meters":"shown"}}'

# The grip under the monitors sets their height and keeps it in the
# settings; a double click goes back to the standard height (15 grid units).
view_setting() { sed -n "s/^$1=//p" "$WORK"/config/lmix/lmix.conf 2>/dev/null; }
setting_becomes() {
    for _ in $(seq 1 20); do [[ "$(view_setting "$1")" == "$2" ]] && return 0; sleep 0.5; done
    return 1
}
drag_y() {
    # Over the grip first: right after a popup closed, a press without a move
    # before it found no item under the pointer (once in a run).
    DISPLAY="$DISPLAY_NO" xdotool mousemove "$1" "$(($2 + 3))"
    sleep 0.3
    DISPLAY="$DISPLAY_NO" xdotool mousemove "$1" "$2"
    sleep 0.3
    DISPLAY="$DISPLAY_NO" xdotool mousedown 1
    # In steps: a jump in one event is no drag for Qt.
    local y="$2"
    while (( y != $3 )); do
        (( y += ($3 > y ? 1 : -1) * ($3 - y > 8 || y - $3 > 8 ? 8 : 1) ))
        sleep 0.03
        DISPLAY="$DISPLAY_NO" xdotool mousemove "$1" "$y"
    done
    sleep 0.1
    DISPLAY="$DISPLAY_NO" xdotool mouseup 1
}
# At 1440 x 900 the monitors are 270 px high, a little lower than the
# transitions beside them: a drag of 100 px up stops at their height.
grip="$(DISPLAY="$DISPLAY_NO" python3 "$ROOT/scripts/a11y.py" find "Height of the monitors|Höhe der Monitore")" \
    || fail "no grip under the monitors"
grip_x="${grip% *}" grip_y="${grip#* }"
drag_y "$grip_x" "$grip_y" "$((grip_y - 100))"
for _ in $(seq 1 20); do [[ -n "$(view_setting monitorHeight)" ]] && break; sleep 0.5; done
height="$(view_setting monitorHeight)"
awk "BEGIN { exit !($height > 170 && $height < 270) }" && pass "monitor grip: monitors stay as high as the transitions ($height)" \
    || fail "monitor grip: height '$height' after a drag of 100 px up"
# A double click goes back to the standard height (15 grid units), or
# the most the halves beside the transitions give (a little less).
grip="$(DISPLAY="$DISPLAY_NO" python3 "$ROOT/scripts/a11y.py" find "Height of the monitors|Höhe der Monitore")"
DISPLAY="$DISPLAY_NO" xdotool mousemove ${grip% *} ${grip#* } click --repeat 2 --delay 80 1
for _ in $(seq 1 20); do [[ "$(view_setting monitorHeight)" != "$height" ]] && break; sleep 0.5; done
awk "BEGIN { exit !($(view_setting monitorHeight) > 250 && $(view_setting monitorHeight) <= 270) }" \
    && pass "monitor grip: a double click resets it" \
    || fail "monitor grip: height $(view_setting monitorHeight) after a double click"

# The grip left of the audio column: a drag to the left widens it, a
# double click goes back to the standard width (12 grid units). A wide
# column leaves less room than the input actions take in one row: the
# last ones go to the "⋯" menu, and the column stays in the window with
# its grip at the pointer.
drag_x() {
    DISPLAY="$DISPLAY_NO" xdotool mousemove "$(($1 + 3))" "$2"
    sleep 0.3
    DISPLAY="$DISPLAY_NO" xdotool mousemove "$1" "$2"
    sleep 0.3
    DISPLAY="$DISPLAY_NO" xdotool mousedown 1
    local x="$1"
    while (( x != $3 )); do
        (( x += ($3 > x ? 1 : -1) * ($3 - x > 8 || x - $3 > 8 ? 8 : 1) ))
        sleep 0.03
        DISPLAY="$DISPLAY_NO" xdotool mousemove "$x" "$2"
    done
    sleep 0.1
    DISPLAY="$DISPLAY_NO" xdotool mouseup 1
}
drag_x 1205 300 765
setting_becomes mixerWidth 656 && pass "audio grip: a drag widens the column" \
    || fail "audio grip: width $(view_setting mixerWidth) after a drag of 440 px left"
sleep 1
! DISPLAY="$DISPLAY_NO" python3 "$ROOT/scripts/a11y.py" find "Data…|Daten…" --role button --wait 1 >/dev/null 2>&1 \
    || fail "input actions: Data… still in the row beside a wide audio column"
click_on "More input actions|Weitere Input-Aktionen" --role button
sleep 1
DISPLAY="$DISPLAY_NO" python3 "$ROOT/scripts/a11y.py" find "Data…|Daten…" --role "menu item" >/dev/null 2>&1 \
    && pass "input actions: what has no room is in the ⋯ menu" || fail "input actions: Data… neither in the row nor in the ⋯ menu"
DISPLAY="$DISPLAY_NO" xdotool key Escape
sleep 1
DISPLAY="$DISPLAY_NO" xdotool mousemove 765 300 click --repeat 2 --delay 80 1
setting_becomes mixerWidth 216 && pass "audio grip: a double click resets it" \
    || fail "audio grip: width $(view_setting mixerWidth) after a double click"
# The grip over the output paths: a drag up makes them higher and keeps
# the height, a double click goes back to following their rows (0).
drag_y 1324 602 542
for _ in $(seq 1 20); do (( $(view_setting mixerPathsHeight || echo 0) > 0 )) 2>/dev/null && break; sleep 0.5; done
(( $(view_setting mixerPathsHeight) > 0 )) 2>/dev/null && pass "paths grip: a drag sets their height" \
    || fail "paths grip: height '$(view_setting mixerPathsHeight)' after a drag of 60 px up"
DISPLAY="$DISPLAY_NO" xdotool mousemove 1324 542 click --repeat 2 --delay 80 1
setting_becomes mixerPathsHeight 0 && pass "paths grip: a double click resets it" \
    || fail "paths grip: height $(view_setting mixerPathsHeight) after a double click"

# Detail: the tiles get the overlay buttons and the audio row, and the
# monitors give them room, so a row of tiles is not cut off.
click_on "View|Ansicht" --role button
sleep 1
click_on "Detail"
setting_becomes detail true || fail "Detail not stored after a click"
DISPLAY="$DISPLAY_NO" xdotool key Escape
sleep 2
click_on "Overlay 1" --role "check box" --within "Input 1 .*"
for _ in $(seq 1 40); do [[ "$(overlay_1)" != none ]] && break; sleep 0.5; done
[[ "$(overlay_1)" == "$(state | jq -r '.inputs[0].id')" ]] && pass "detail: overlay 1 from the tile" \
    || fail "detail: overlay 1 is $(overlay_1) after a click on the tile's button"
click_on "Mute|Stumm" --role "check box" --within "Input 1 .*"
for _ in $(seq 1 40); do [[ "$(audio_1)" == false ]] && break; sleep 0.5; done
[[ "$(audio_1)" == false ]] && pass "detail: mute from the tile" || fail "detail: input 1 not muted from the tile (cut off?)"
act '{"action":"set_audio","input":1,"on":true}'
act '{"action":"overlay_out","channel":1}'

own_qml_warnings console.log && fail "QML warnings: $(own_qml_warnings console.log | head -3)"
pass "no QML warnings"

# The Show page through the page bar, same window: it draws the show
# view (not a flat colour), and the bar shows program small.
act '{"action":"add_cue_list","name":"Act 1","cues":[{"number":"1","name":"Doors","kind":"wait","duration_ms":120000},{"number":"2","name":"House","kind":"wait","duration_ms":120000},{"number":"3","name":"Lights","kind":"note"}]}'
show_window="$(DISPLAY="$DISPLAY_NO" xdotool search --name '^lmix' 2>/dev/null | head -1 || true)"
[[ -n "$show_window" ]] || fail "no lmix window"
# Pages by key (Ctrl+n): the tabs' places depend on their labels.
DISPLAY="$DISPLAY_NO" xdotool windowfocus "$show_window"
sleep 1
xdotool key ctrl+2
sleep 3
shot show
awk "BEGIN { exit !($(spread show 1400x700+20+60) > 0.05) }" && pass "Show page draws" || fail "Show page blank"
PGM_SMALL="$(box_of "Program|Programm" 0.1)" || fail "no program in the page bar"
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
DISPLAY="$DISPLAY_NO" xdotool windowfocus "$show_window"
sleep 1
until_true() { for _ in $(seq 1 20); do eval "$1" && return 0; sleep 0.5; done; return 1; }
key space
until_true '[[ "$(standby)" == 2 && "$(cues_running)" -ge 1 ]]' && pass "show: space is GO" \
    || fail "show: space did not GO (standby $(standby), running $(cues_running))"
# A header button and its dialog (Keys, closed with Escape) must not
# keep the keys: Escape closes the dialog, the cue keeps running.
click_on "Keys|Tasten" --role button
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
# The side bar: tab Lists, a click on the second list makes it the active
# one, and space is GO there (the bar keeps no keys).
act '{"action":"add_cue_list","name":"Encore","cues":[{"number":"11","name":"Bow","kind":"wait","duration_ms":30000}]}'
active_list() { state | jq '[.cue_lists[] | .active] | index(true) + 1'; }
click_on "Lists|Listen" --role "page tab"
sleep 1
# The second list's row, under the tab.
lists_tab="$(DISPLAY="$DISPLAY_NO" python3 "$ROOT/scripts/a11y.py" find "Lists|Listen" --role "page tab")"
click "${lists_tab% *}" "$((${lists_tab#* } + 85))"
until_true '[[ "$(active_list)" == 2 ]]' && pass "show side bar: a click makes a list active" \
    || fail "show side bar: list $(active_list) active, not 2"
key space
until_true '[[ "$(state | jq '.cue_lists[1].standby')" == 2 ]]' && pass "show side bar: space is GO after a click there" \
    || fail "show side bar: no GO after a click in the side bar"
key Escape
until_true '[[ "$(cues_running)" == 0 ]]' || fail "show side bar: Escape did not stop the cues"
act '{"action":"set_active_cue_list","list":1}'
# The inspector under the list (edit mode) shows the standby cue, 3
# Lights: a new name goes to lmix-core on Enter, and space is GO again.
inspector_name() { click_on "Cue name|Name des Cues" --role text; }
until_true '[[ "$(state | jq '.cue_lists[0].standby')" == 3 ]]' || fail "inspector: standby not on cue 3"
until_true '[[ "$(active_list)" == 1 ]]' || fail "inspector: list 1 not active"
sleep 1 # the page shows list 1 again (it was set from outside)
inspector_name
xdotool key ctrl+a
xdotool type --delay 20 "Lamps"
key Return
until_true '[[ "$(state | jq -r '.cue_lists[0].cues[2].name')" == Lamps ]]' && pass "inspector: a name typed there goes to the cue" \
    || fail "inspector: cue 3 is named $(state | jq -r '.cue_lists[0].cues[2].name'), not Lamps"
# Two quick edits: a name typed without Enter, then a follow on the
# Triggers tab. Both stay (each sends only its own field).
# The inspector's rows by its tabs (the page's height places them).
inspector_tab() { click_on "$1" --role "page tab"; }
below_tabs() {
    local at
    at="$(DISPLAY="$DISPLAY_NO" python3 "$ROOT/scripts/a11y.py" find "Basics|Grundlagen" --role "page tab")" \
        || fail "no inspector on the screen"
    click "$1" "$((${at#* } + 44))"
}
INSPECTOR_AFTER_WAIT=305
inspector_name
xdotool key ctrl+a
xdotool type --delay 20 "Lamps 2"
inspector_tab "Triggers|Auslöser"
below_tabs $INSPECTOR_AFTER_WAIT
until_true '[[ "$(state | jq -r '.cue_lists[0].cues[2].follow')" == after_wait ]]' \
    && [[ "$(state | jq -r '.cue_lists[0].cues[2].name')" == "Lamps 2" ]] \
    && pass "inspector: two quick edits both stay" \
    || fail "inspector: cue 3 $(state | jq -c '.cue_lists[0].cues[2] | {name, follow}')"
# The choice follows the cue shown after a click: on cue 2 (waits for
# GO) it shows that, and a click on After the post wait goes to cue 2.
act '{"action":"cue_standby","to":{"index":2}}'
until_true '[[ "$(standby)" == 2 ]]' || fail "inspector: standby not on cue 2"
sleep 1 # the inspector shows cue 2
below_tabs $INSPECTOR_AFTER_WAIT
until_true '[[ "$(state | jq -r '.cue_lists[0].cues[1].follow')" == after_wait ]]' && pass "inspector: a choice follows the cue shown" \
    || fail "inspector: a click on After the post wait left cue 2 at $(state | jq -r '.cue_lists[0].cues[1].follow')"
act '{"action":"update_cue","list":1,"cue":"2","changes":{"follow":"none"}}'
act '{"action":"cue_standby","to":{"index":3}}'
until_true '[[ "$(standby)" == 3 ]]' || fail "inspector: standby not back on cue 3"
sleep 1 # the inspector shows cue 3 again
inspector_tab "Basics|Grundlagen"
# Standby moves on from outside while a field is being typed in: what
# is typed still goes to the cue it began with.
inspector_name
xdotool key ctrl+a
act '{"action":"cue_standby","to":{"index":1}}'
until_true '[[ "$(standby)" == 1 ]]' || fail "inspector: standby did not move from outside"
xdotool type --delay 20 "Lamp"
key Return
until_true '[[ "$(state | jq -r '.cue_lists[0].cues[2].name')" == Lamp ]]' \
    && [[ "$(state | jq -r '.cue_lists[0].cues[0].name')" == Doors ]] \
    && pass "inspector: keeps its cue while standby moves" \
    || fail "inspector: cue 3 $(state | jq -r '.cue_lists[0].cues[2].name'), cue 1 $(state | jq -r '.cue_lists[0].cues[0].name')"
# Escape in a field is panic and leaves the field without saving, then
# space is GO.
act '{"action":"cue_standby","to":{"index":1}}'
until_true '[[ "$(standby)" == 1 ]]' || fail "inspector: standby not on 1"
key space
until_true '[[ "$(cues_running)" -ge 1 ]]' || fail "inspector: GO started nothing"
act '{"action":"cue_standby","to":{"index":3}}'
until_true '[[ "$(standby)" == 3 ]]' || fail "inspector: standby not back on 3"
name_before="$(state | jq -r '.cue_lists[0].cues[2].name')"
inspector_name
xdotool key End
xdotool type --delay 20 " typo"
key Escape
until_true '[[ "$(cues_running)" == 0 ]]' && pass "inspector: Escape in a field is panic" \
    || fail "inspector: Escape in a field left $(cues_running) cues running"
sleep 1 # a save would have come back by now
[[ "$(state | jq -r '.cue_lists[0].cues[2].name')" == "$name_before" ]] && pass "inspector: Escape drops what was typed" \
    || fail "inspector: Escape saved the cue as $(state | jq -r '.cue_lists[0].cues[2].name')"
key space
until_true '[[ "$(standby)" == 4 ]]' && pass "inspector: space is GO after Enter or Escape in a field" \
    || fail "inspector: no GO after editing (standby $(standby))"
key Escape
# A level by + and typed: each goes to the cue, and after Enter in the
# box space is GO again. A fade cue at -10 dB, the Levels tab.
act '{"action":"add_cue_list","name":"Look","cues":[{"number":"20","kind":"fade","duration_ms":3000,"fade":{"target":{"zone":"Main"},"gain_db":-10}}]}'
act '{"action":"set_active_cue_list","list":3}'
until_true '[[ "$(active_list)" == 3 ]]' || fail "inspector: list 3 not active"
sleep 1 # the page shows list 3
INSPECTOR_LEVEL_UP=233
INSPECTOR_LEVEL_TEXT=188
inspector_tab "Levels|Pegel"
below_tabs $INSPECTOR_LEVEL_UP
until_true '[[ "$(state | jq '.cue_lists[2].cues[0].fade.gain_db')" == -9.5 ]]' && pass "inspector: + of a level goes to the cue" \
    || fail "inspector: fade level $(state | jq '.cue_lists[2].cues[0].fade.gain_db'), not -9.5"
below_tabs $INSPECTOR_LEVEL_TEXT
xdotool key ctrl+a
xdotool type --delay 20 -- "-6"
key Return
until_true '[[ "$(state | jq '.cue_lists[2].cues[0].fade.gain_db==-6')" == true ]]' && pass "inspector: a typed level goes to the cue" \
    || fail "inspector: fade level $(state | jq '.cue_lists[2].cues[0].fade.gain_db'), not -6"
# Escape in the box (the panic) drops what was typed there too.
below_tabs $INSPECTOR_LEVEL_TEXT
xdotool key ctrl+a
xdotool type --delay 20 -- "-3"
key Escape
sleep 1 # a save would have come back by now
[[ "$(state | jq '.cue_lists[2].cues[0].fade.gain_db==-6')" == true ]] && pass "inspector: Escape in a level drops what was typed" \
    || fail "inspector: Escape saved the level as $(state | jq '.cue_lists[2].cues[0].fade.gain_db')"
# The box shows a level changed afterwards: Enter there keeps it.
act '{"action":"update_cue","list":3,"cue":"20","changes":{"fade":{"gain_db":-10}}}'
sleep 1 # the page shows the new state
below_tabs $INSPECTOR_LEVEL_TEXT
key Return
sleep 1 # a save would have come back by now
[[ "$(state | jq '.cue_lists[2].cues[0].fade.gain_db==-10')" == true ]] && pass "inspector: the box follows the level after Escape" \
    || fail "inspector: after Escape the box saved $(state | jq '.cue_lists[2].cues[0].fade.gain_db') over -10"
key space
until_true '[[ "$(state | jq '.cue_lists[2].standby')" == 2 ]]' && pass "inspector: space is GO after a level" \
    || fail "inspector: no GO after a level (standby $(state | jq '.cue_lists[2].standby'))"
key Escape
inspector_tab "Basics|Grundlagen"
act '{"action":"remove_cue_list","list":3}'
own_qml_warnings console.log && fail "QML warnings (show): $(own_qml_warnings console.log | head -3)"
pass "no QML warnings (show)"

# The other pages through the page bar: each draws something of its own
# (a light page is flat, so: it differs from the page before), no QML
# warnings. A bus draws its rows in the bus panel, the send matrix and
# the channel strip.
act '{"action":"add_bus","name":"Stream"}'
until_true '[[ "$(state | jq ".buses | length")" == 1 ]]' || fail "add_bus: no bus in the state"
prev=show
for page in "AUDIO 3" "OUTPUTS 4" "SETUP 5"; do
    read -r name n <<<"$page"
    DISPLAY="$DISPLAY_NO" xdotool windowfocus "$show_window"
    xdotool key "ctrl+$n"
    sleep 2
    shot "page-$name"
    (($(moved "$prev" "page-$name" 1400x700+20+60) > 5000)) && pass "$name page draws" || fail "$name page shows the page before"
    prev="page-$name"
    if [[ "$name" == AUDIO ]]; then
        # A fader drag of several steps: each step brings a new state,
        # and the strip must survive it (a rebuilt strip ended the drag
        # after the first step, at about 93).
        FADER_TOP="54 214"
        xdotool mousemove $FADER_TOP mousedown 1
        for y in 250 300 350 400 450 480; do xdotool mousemove 54 "$y"; sleep 0.15; done
        xdotool mouseup 1
        sleep 1
        volume="$(state | jq '.inputs[0].audio.volume')"
        ((volume < 70)) && pass "Audio: a fader drag goes on over state updates ($volume)" \
            || fail "Audio: fader drag ended early (volume $volume)"
    fi
done
act '{"action":"remove_bus","bus":"Stream"}'
until_true '[[ "$(state | jq ".buses | length")" == 0 ]]' || fail "remove_bus: the bus stays"
# The page bar follows every page change: a click on a tab, then Ctrl+n,
# then the tab again (a click once cut the bar's binding: it kept the
# clicked tab marked and the next click on it did nothing).
FIRST_TAB="25 879"
click $FIRST_TAB
sleep 2
shot tab-mix
DISPLAY="$DISPLAY_NO" xdotool windowfocus "$show_window"
xdotool key ctrl+3
sleep 2
shot tab-audio
click $FIRST_TAB
sleep 2
shot tab-mix-again
# By the page title (in the header, after the file menu): the pages'
# pictures move on their own.
PAGE_TITLE="200x40+110+4"
(($(moved tab-audio tab-mix-again "$PAGE_TITLE") > 50)) && (($(moved tab-mix tab-mix-again "$PAGE_TITLE") < 50)) \
    && pass "page bar: a tab after Ctrl+n switches again" || fail "page bar: the tab did not switch back after Ctrl+n"

# Panic all: cues stop, program fades to black, a running stream goes on.
act '{"action":"start_output","output":1}'
act '{"action":"cue_reset"}'
DISPLAY="$DISPLAY_NO" xdotool windowfocus "$show_window"
xdotool key ctrl+2
sleep 1
xdotool key space
until_true '[[ "$(cues_running)" -ge 1 ]]' || fail "no cue running before Panic all"
click_on "Panic all|Panik: alles" --role button
until_true '[[ "$(cues_running)" == 0 && "$(state | jq .fade_to_black)" == true ]]' \
    && pass "Panic all: cues stopped, program black" || fail "Panic all: running $(cues_running), black $(state | jq .fade_to_black)"
click_on "Panic all|Panik: alles" --role button
sleep 2
[[ "$(state | jq .fade_to_black)" == true ]] && pass "Panic all twice: program stays black" || fail "a second Panic all faded program up"
[[ "$(state | jq -r '.outputs[0].status.state')" == running ]] && pass "Panic all: the stream goes on" || fail "Panic all stopped the stream"

own_qml_warnings console.log && fail "QML warnings (pages): $(own_qml_warnings console.log | head -3)"
pass "no QML warnings (Audio, Outputs, Setup)"
criticals="$(tail -n +"$((STROM_START_LINES + 1))" "$WORK/strom.log" | grep -c 'GStreamer-CRITICAL' || true)"
[[ "$criticals" == 0 ]] || fail "$criticals GStreamer CRITICALs in strom.log"
pass "no GStreamer CRITICALs (live flows)"
echo "lmix Mix and Show check passed"
