#!/usr/bin/env bash
# SDI loop on a DeckLink card (SRC.1, OUT.4): a cable from one connector
# back into another. Program goes out on SDI_OUT and comes back in on
# SDI_IN; the check wants the card listed, the output running without
# refused frames, the picture back like the program and moving, and the
# embedded sound back at the program's level. Then 8 channels with a bus
# on pair 2 (channels 3/4), heard on the input's pair 2, and a sound
# delay. Needs the card, its driver
# (Desktop Video) and the GStreamer decklink plugin: stays out of CI.
#
#   scripts/test-sdi-loop.sh
#   SDI_OUT=1 SDI_IN=2          # device numbers of the looped connectors
#                               # (GStreamer's device-number, from 0)
#   MODES="1080p25 1080i50"     # modes to run (default 1080p25 1080i50 1080psf25 720p50)
#   PAIR=2                      # pair the bus goes on, 2-8 (5-8: 16 channels)
#   PAIR_MODE=1080p25           # mode of that part, the input set to it
#   SOUND_DELAY_MS=200          # the card output's sound delay there
#   MIXER=cuda|cpu              # default cuda
#   STROM_BIN=…/strom           # skip scripts/fetch-strom.sh
#   KEEP_WORK=1                 # keep logs and pictures
#   STROM_GST_DEBUG=decklink:6  # more GST_DEBUG for strom (gst.log)
#   HOLD=600                    # on failure, keep strom and lmix-core up so long
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STROM_PORT=8697
CORE_PORT=8797
STROM="http://127.0.0.1:${STROM_PORT}"
CORE="http://127.0.0.1:${CORE_PORT}/api"
WORK="$(mktemp -d)"
PIDS=()
SDI_OUT="${SDI_OUT:-1}"
SDI_IN="${SDI_IN:-2}"
MODES="${MODES:-1080p25 1080i50 1080psf25 720p50}"
MIXER="${MIXER:-cuda}"
PAIR="${PAIR:-2}"
PAIR_MODE="${PAIR_MODE:-1080p25}"
SOUND_DELAY_MS="${SOUND_DELAY_MS:-200}"
# A card locks to a new signal within a few seconds.
SETTLE=20
# Thumbnail taps refresh about once a second.
TAP_INTERVAL=1.5
# Sound back from the cable may differ from the program by rounding only.
LEVEL_TOLERANCE_DB=1
# A silent pair meters below this.
SILENT_DB=-60
# Bars back from the cable, scaled and converted twice: normalised RMSE
# (ImageMagick, 0-1) of the input's picture against the program's.
MAX_PICTURE_RMSE=0.08

cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    # strom may hang on SIGTERM (todo.md); it must let go of the card.
    for _ in $(seq 1 20); do
        alive() { for pid in "${PIDS[@]}"; do kill -0 "$pid" 2>/dev/null && return 0; done; return 1; }
        alive || break
        sleep 0.5
    done
    for pid in "${PIDS[@]}"; do kill -9 "$pid" 2>/dev/null || true; done
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT
pass() { echo "ok   $*"; }
fail() {
    echo "FAIL: $*"
    for log in strom.log core.log; do
        echo "--- last 30 lines of $log"
        tail -n 30 "$WORK/$log"
    done
    KEEP_WORK=1
    [[ -n "${HOLD:-}" ]] && { echo "holding for $HOLD s"; sleep "$HOLD"; }
    exit 1
}
act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null || fail "action $1"; }
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    fail "$2 did not come up"
}
wait_for() {   # wait_for <seconds> <command…>
    local deadline=$((SECONDS + $1)); shift
    until "$@"; do
        ((SECONDS < deadline)) || return 1
        sleep 0.5
    done
}
state() { curl -sf "$CORE/state"; }
# picture <what> <file>: program or inputs/<n>. The first request wakes
# the tap; the second waits for a fresh frame.
picture() {
    curl -sf "$CORE/pictures/$1" -o /dev/null || true
    sleep "$TAP_INTERVAL"
    curl -sf "$CORE/pictures/$1" -o "$2"
}
peak() { curl -sf "$CORE/audio/levels" | jq "$1"; }
# The caps the card input's source negotiated, from the engine.
input_caps() {
    local flow
    for flow in $(curl -sf "$STROM/api/flows" | jq -r '.flows[].id'); do
        curl -sf "$STROM/api/flows/$flow/pad-caps" \
            | jq -r '.["card:decklinkvideosrc"][]? | select(.direction == "src") | .caps'
    done
}

command -v magick >/dev/null || { echo "needs ImageMagick (magick)" >&2; exit 2; }
gst-inspect-1.0 decklinkvideosink >/dev/null 2>&1 || { echo "needs the GStreamer decklink plugin" >&2; exit 2; }

# A card connector takes one user; an engine left over holds it.
pgrep -f '^[^ ]*/strom --host' >/dev/null && { echo "another strom is running; it may hold the card" >&2; exit 2; }

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
(cd "$ROOT/workspace/lmix-core" && cargo build --release)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"

# Display outputs are not used; the engine stays off the desktop.
# The card reports each frame shown late or dropped (decklinkvideosink
# at DEBUG).
(cd "$WORK" && exec env -u WAYLAND_DISPLAY GST_GL_WINDOW=x11 GST_DEBUG="decklinkvideosink:5${STROM_GST_DEBUG:+,$STROM_GST_DEBUG}" \
    GST_DEBUG_FILE="$WORK/gst.log" GST_DEBUG_NO_COLOR=1 "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" \
    --no-auto-restart --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "$STROM/health" strom
"$CORE_BIN" --strom "$STROM" --listen "127.0.0.1:${CORE_PORT}" --mixer "$MIXER" --slots 4 \
    --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" \
    --layout-templates "$WORK/layout-templates.json" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core

has_card() { curl -sf "$CORE/devices" | jq -e --arg n "$1" 'any(.[]; .kind == "card" and .id == $n)' >/dev/null; }
wait_for "$SETTLE" has_card "$SDI_OUT" && has_card "$SDI_IN" \
    || fail "cards $SDI_OUT and $SDI_IN not listed: $(curl -sf "$CORE/devices")"
pass "card connectors $SDI_OUT and $SDI_IN listed"

act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Bars"}'
act '{"action":"add_input","source":{"kind":"pattern","pattern":"ball"},"name":"Ball"}'
act "{\"action\":\"add_input\",\"source\":{\"kind\":\"card\",\"device\":$SDI_IN},\"name\":\"SDI in\"}"
# Heard on program, the cable would feed the program's sound back to itself.
act '{"action":"set_audio","input":"SDI in","on":false}'
online() { [[ "$(state | jq '[.inputs[] | select(.status == "online")] | length')" == 3 ]]; }
wait_for "$SETTLE" online || fail "inputs not online: $(state | jq -c '[.inputs[] | {name, status}]')"
pass "inputs online"
SDI_IN_ID="$(state | jq -r '.inputs[] | select(.name == "SDI in") | .id')"

refused() { grep -c 'Failed to schedule' "$WORK/strom.log" || true; }
missed() { grep -cE '(Late|Dropped) Frame' "$WORK/gst.log" || true; }
for mode in $MODES; do
    act '{"action":"cut_direct","input":"Bars"}'
    act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"card\",\"device\":$SDI_OUT,\"mode\":\"$mode\"},\"name\":\"SDI out\"}"
    before="$(refused)"
    act '{"action":"start_output","output":"SDI out"}'
    running() { [[ "$(state | jq -r '.outputs[] | select(.name == "SDI out") | .status.state')" == running ]]; }
    wait_for "$SETTLE" running || fail "$mode: output $(state | jq -c '.outputs')"
    # The input notices the new mode and starts over.
    sleep 8
    [[ "$(refused)" == "$before" ]] || fail "$mode: the card refused frames"
    pass "$mode: output running, no frame refused"
    # The first frames of a start may come late; from here on none.
    missed_before="$(missed)"

    # The input found the mode on the cable (it is on auto).
    # PsF goes as interlaced fields; on auto the input takes it so.
    case "$mode" in
        720p*) want="width=(int)1280" ;;
        *i* | *psf*) want="interlace-mode=(string)interleaved" ;;
        *) want="interlace-mode=(string)progressive" ;;
    esac
    caps="$(input_caps)"
    [[ "$caps" == *"$want"* ]] || fail "$mode: input caps $caps, want $want"
    pass "$mode: input took the mode ($want)"

    picture program "$WORK/$mode-program.jpg"
    picture inputs/3 "$WORK/$mode-in.jpg"
    rmse="$(magick compare -metric RMSE "$WORK/$mode-program.jpg" "$WORK/$mode-in.jpg" null: 2>&1 | sed -E 's/.*\((.*)\).*/\1/' || true)"
    awk -v r="$rmse" -v m="$MAX_PICTURE_RMSE" 'BEGIN { exit !(r != "" && r + 0 <= m) }' \
        || fail "$mode: picture back differs from program (RMSE $rmse > $MAX_PICTURE_RMSE)"
    pass "$mode: picture back like program (RMSE $rmse)"

    act '{"action":"cut_direct","input":"Ball"}'
    sleep 2
    picture inputs/3 "$WORK/$mode-a.jpg"
    picture inputs/3 "$WORK/$mode-b.jpg"
    cmp -s "$WORK/$mode-a.jpg" "$WORK/$mode-b.jpg" && fail "$mode: picture back stands still"
    pass "$mode: picture back moves"

    pgm="$(peak .program_peak_db)"
    back="$(peak ".inputs[] | select(.input == \"$SDI_IN_ID\") | .peak_db")"
    awk -v p="$pgm" -v b="$back" -v t="$LEVEL_TOLERANCE_DB" 'BEGIN { d = p - b; if (d < 0) d = -d; exit !(p > -60 && d <= t) }' \
        || fail "$mode: sound back at $back dB, program at $pgm dB"
    pass "$mode: sound back at $back dB (program $pgm dB)"

    missed_now="$(missed)"
    ((missed_now - missed_before == 0)) || fail "$mode: the card showed $((missed_now - missed_before)) frames late or dropped them"
    pass "$mode: no frame late or dropped"

    act '{"action":"remove_output","output":"SDI out"}'
    sleep 2
done

# Bus "Pair" on pair $PAIR (8 or 16 channels), played SOUND_DELAY_MS late; the
# input, set to the mode, hears that pair. Silent while nothing is sent to
# the bus, so program (pair 1) does not leak into it.
act '{"action":"add_bus","name":"Pair"}'
PAIR_BUS="$(state | jq -r '.buses[] | select(.name == "Pair") | .id')"
CHANNELS=$((PAIR <= 4 ? 8 : 16))
MORE_PAIRS="$(jq -cn --arg bus "$PAIR_BUS" --argjson n "$PAIR" '[range(2; $n) | null] + [$bus]')"
act "{\"action\":\"replace_source\",\"input\":\"SDI in\",\"source\":{\"kind\":\"card\",\"device\":$SDI_IN,\"mode\":\"$PAIR_MODE\",\"pair\":$PAIR}}"
act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"card\",\"device\":$SDI_OUT,\"mode\":\"$PAIR_MODE\",\"channels\":\"$CHANNELS\",\"more_pairs\":$MORE_PAIRS,\"sound_delay_ms\":$SOUND_DELAY_MS},\"name\":\"SDI out\"}"
before="$(refused)"
act '{"action":"start_output","output":"SDI out"}'
wait_for "$SETTLE" running || fail "pairs: output $(state | jq -c '.outputs')"
wait_for "$SETTLE" online || fail "pairs: input not online"
sleep 8
[[ "$(refused)" == "$before" ]] || fail "pairs: the card refused frames"
pass "$PAIR_MODE, $CHANNELS channels, $SOUND_DELAY_MS ms sound delay: output running, no frame refused"

# The input runs in the mode and moves.
picture inputs/3 "$WORK/pair-a.jpg"
picture inputs/3 "$WORK/pair-b.jpg"
cmp -s "$WORK/pair-a.jpg" "$WORK/pair-b.jpg" && fail "pairs: picture back stands still"
pass "pairs: picture back moves"

input_peak() { peak ".inputs[] | select(.input == \"$SDI_IN_ID\") | .peak_db"; }
silent() { awk -v b="$(input_peak)" -v s="$SILENT_DB" 'BEGIN { exit !(b == "" || b + 0 < s) }'; }
wait_for 5 silent || fail "pairs: pair $PAIR at $(input_peak) dB with nothing on the bus"
pass "pairs: pair $PAIR silent while the bus is ($(input_peak) dB)"

# How long the sound takes from the send to the input: the delay plus
# the way round (meters refresh every few tens of ms).
sent=$(date +%s%3N)
act '{"action":"set_send","input":"Bars","bus":"Pair","level":80}'
sounding() { ! silent; }
until sounding; do
    (( $(date +%s%3N) - sent < 5000 )) || fail "pairs: pair $PAIR silent with Bars on the bus"
    sleep 0.02
done
pass "pairs: pair $PAIR carries the bus ($(input_peak) dB) after $(( $(date +%s%3N) - sent )) ms"
echo "SDI loop check passed"
