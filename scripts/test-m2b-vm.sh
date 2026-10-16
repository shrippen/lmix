#!/usr/bin/env bash
# M2b check without GPU: strom (CPU mixer) + lmix-core, driven over the
# lmix API. Proves slot flows, preview/take, source swap and show file.
# Picture quality, real latency and GPU paths: docs/HARDWARE-TODO.md.
#
#   scripts/test-m2b-vm.sh            # builds, runs, cleans up
#   CONSOLE_SHOT=out.png scripts/test-m2b-vm.sh
#                                     # also screenshots the Qt Console
#                                     # (lmix-core client) on Xvfb
#   KEEP_WORK=1                       # keep logs and pictures
#   WORK=dir                          # put them there (default: mktemp)
#   LATENCY_BUDGET_MS=150             # p95 capture-to-program budget
#   SETTLE_S=10                       # how long a check waits for its state
#   STROM_GST_DEBUG=compositor:4      # GST_DEBUG for strom (gst.log)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STROM_PORT=8199
CORE_PORT=8299
STROM="http://127.0.0.1:${STROM_PORT}"
CORE="http://127.0.0.1:${CORE_PORT}/api"
WORK="${WORK:-$(mktemp -d)}"
mkdir -p "$WORK"
# Mixer slots for the test; small to fit a VM.
SLOTS=7   # six inputs plus a stinger clip
# Mean brightness (0-100) bars or snow exceed; black is 0.
MIN_BRIGHTNESS=30
# Audio levels (dBFS): a test tone is above, silence below.
AUDIBLE_DB=-40
SILENT_DB=-60
# Thumbnail taps refresh about once a second.
TAP_INTERVAL=1.5
# Checks poll for the expected state up to this long. The CPU mixer in a
# shared CI container can lag several seconds behind real time; a slow
# engine passes, a wrong state still fails.
SETTLE_S="${SETTLE_S:-10}"

PIDS=()
cleanup() {
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    [[ -n "${KEEP_WORK:-}" ]] && { echo "work kept: $WORK" >&2; return; }
    rm -rf "$WORK"
}
trap cleanup EXIT

fail() {
    echo "FAIL: $*" >&2
    # What the engine and lmix-core were doing (CI shows no files).
    local log
    for log in strom.log core.log gst.log; do
        [[ -s "$WORK/$log" ]] || continue
        echo "--- last 60 lines of $log" >&2
        tail -n 60 "$WORK/$log" >&2
    done
    # Earlier trouble scrolls out of the tails (thumbnail polls fill them).
    echo "--- last 40 warnings and errors of strom.log" >&2
    grep -E 'WARN|ERROR' "$WORK/strom.log" | grep -v '/thumbnail' | tail -n 40 >&2 || true
    echo "--- strom flows" >&2
    curl -sf --max-time 5 "$STROM/api/flows" \
        | jq -c '.flows[] | {name, id, state, running}' >&2 || true
    exit 1
}
pass() { echo "ok   $*"; }
wait_for() {   # wait_for <seconds> <command…>: until the command succeeds
    local seconds="$1"; shift
    for _ in $(seq 1 "$((seconds * 2))"); do "$@" && return 0; sleep 0.5; done
    return 1
}
# settled <condition>: polls a bash condition (as for eval) until it holds.
settled() { wait_for "$SETTLE_S" eval "$1"; }
# is <state function> <name> <state>: evaluated anew on every try.
is() { [[ "$("$1" "$2")" == "$3" ]]; }

act() {
    local out code
    out="$(curl -s -w '\n%{http_code}' -X POST "$CORE/actions" -H 'content-type: application/json' -d "$1")"
    code="${out##*$'\n'}"
    [[ "$code" == 200 ]] || fail "action $1 -> $code ${out%$'\n'*}"
    printf '%s\n' "${out%$'\n'*}"
}

wait_up() {
    local url="$1" what="$2"
    for _ in $(seq 1 120); do
        curl -sf "$url" >/dev/null 2>&1 && return 0
        sleep 1
    done
    fail "$what did not come up"
}

# --- build ---
# Prebuilt strom from CI when available (scripts/fetch-strom.sh).
STROM_BIN="$("$ROOT/scripts/fetch-strom.sh")"
(cd "$ROOT/workspace/lmix-core" && cargo build)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/debug/lmix-core"

# --- start ---
# A screen for display outputs (engine side). On a desktop the engine
# would open its windows there (Wayland first), so it only gets this one.
ENGINE_DISPLAY=:94
Xvfb "$ENGINE_DISPLAY" -screen 0 1280x720x24 >/dev/null 2>&1 &
PIDS+=($!)
(cd "$WORK" && unset WAYLAND_DISPLAY && GST_GL_WINDOW=x11 GST_DEBUG="${STROM_GST_DEBUG:-}" GST_DEBUG_FILE="$WORK/gst.log" GST_DEBUG_NO_COLOR=1 DISPLAY="$ENGINE_DISPLAY" LIBGL_ALWAYS_SOFTWARE=1 exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "$STROM/health" strom

start_core() {
    "$CORE_BIN" --strom "$STROM" --listen "127.0.0.1:${CORE_PORT}" --mixer cpu \
        --slots "$SLOTS" --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" >>"$WORK/core.log" 2>&1 &
    CORE_PID=$!
    PIDS+=("$CORE_PID")
    wait_up "$CORE/state" lmix-core
}
start_core
sleep 3
pass "strom and lmix-core up"

# --- inputs ---
act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Bars"}' >/dev/null
act '{"action":"add_input","source":{"kind":"pattern","pattern":"ball"},"name":"Ball"}' >/dev/null
online() { [[ "$(curl -sf "$CORE/state" | jq '[.inputs[] | select(.status=="online")] | length')" == "$1" ]]; }
wait_for "$SETTLE_S" online 2 || fail "inputs not online: $(curl -sf "$CORE/state")"
pass "two inputs online"

# --- preview and take ---
act '{"action":"preview_input","input":"Bars"}' >/dev/null
act '{"action":"cut"}' >/dev/null
act '{"action":"preview_input","input":"Ball"}' >/dev/null
act '{"action":"transition","slot":1}' >/dev/null
settled '[[ "$(curl -sf "$CORE/tally")" == "[2,1]" ]]' || fail "tally $(curl -sf "$CORE/tally")"
pass "preview, cut, fade; tally"

MIXER_ID="$(curl -sf "$STROM/api/flows" | jq -r '.flows[] | select(.name | startswith("lmix mixer")) | .id')"
# Pictures through lmix-core (503 until the tap has a frame; the first
# request switches it on).
picture() {
    local path="$1" out="$2"
    rm -f "$out"
    for _ in $(seq 1 20); do
        curl -sf "$CORE/pictures/$path" -o "$out" && break
        sleep 1
    done
    [[ -s "$out" ]] || fail "no picture $path"
}
# Mean brightness 0-100. A black frame is a valid JPEG of normal size,
# so pictures are judged by content, never by bytes.
brightness() { convert "$1" -colorspace gray -format '%[fx:int(mean*100)]' info:; }
# Program changes between two pictures: live frames, not a still.
program_moves() {
    picture program "$WORK/a.jpg"
    sleep "$TAP_INTERVAL"
    picture program "$WORK/b.jpg"
    ! cmp -s "$WORK/a.jpg" "$WORK/b.jpg"
}

# bright <picture path> <file>: the picture shows content, not black.
bright() { picture "$1" "$2"; [[ $(brightness "$2") -gt $MIN_BRIGHTNESS ]]; }
wait_for "$SETTLE_S" program_moves || fail "program frozen or black with Ball on air"
pass "program moves (Ball over inter)"
wait_for "$SETTLE_S" bright inputs/preview "$WORK/pvw.jpg" || fail "preview picture dark"
act '{"action":"cut_direct","input":"Bars"}' >/dev/null
wait_for "$SETTLE_S" bright program "$WORK/pgm.jpg" || fail "program dark with Bars on air"
pass "pictures show content (preview, program)"

# --- swap the program source while on air ---
act '{"action":"replace_source","input":"Bars","source":{"kind":"pattern","pattern":"snow"}}' >/dev/null
sleep 2   # the mixer must stay up across the swap, not come back later
running="$(curl -sf "$STROM/api/flows/$MIXER_ID" | jq -r '.flow.running')"
[[ "$running" == true ]] || fail "mixer stopped after source swap"
moving_bright() { program_moves && [[ $(brightness "$WORK/b.jpg") -gt $MIN_BRIGHTNESS ]]; }
wait_for "$SETTLE_S" moving_bright || fail "program frozen or dark after source swap"
pass "program source swapped on air: mixer kept running, new picture moves"

# --- sound over the slots (B23): mute, audio follows video, volume ---
# Patterns carry a test tone (about -21 dBFS). Program is Bars (now snow),
# Ball is in preview. Levels refresh every 100 ms.
pgm_db() { curl -sf "$CORE/audio/levels" | jq '.program_peak_db // -100 | floor'; }
input_db() { curl -sf "$CORE/audio/levels" | jq ".inputs[$1].peak_db | floor"; }
# A level is read once it has settled: two readings a second apart agree
# within 3 dB (peaks of a steady tone; a fade or a lagging engine moves).
steady_db() {
    local a b
    for _ in $(seq 1 "$SETTLE_S"); do
        a=$("$@"); sleep 1; b=$("$@")
        (( a - b <= 3 && b - a <= 3 )) && { echo "$b"; return; }
    done
    echo "$b"
}
settled '[[ $(input_db 0) -gt $AUDIBLE_DB && $(input_db 1) -gt $AUDIBLE_DB ]]' || fail "no sound from the slots"
settled '[[ $(pgm_db) -gt $AUDIBLE_DB ]]' || fail "no program sound"
act '{"action":"set_audio","input":"Bars","on":false}' >/dev/null
act '{"action":"set_audio_follow","input":"Ball","follow":true}' >/dev/null
settled '[[ $(pgm_db) -lt $SILENT_DB ]]' || fail "program not silent: Bars off, Ball follows video off air ($(pgm_db) dB)"
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
settled '[[ $(pgm_db) -gt $AUDIBLE_DB ]]' || fail "Ball not heard on air ($(pgm_db) dB)"
full=$(steady_db pgm_db)
act '{"action":"set_volume","input":"Ball","volume":50}' >/dev/null
settled '[[ $(pgm_db) -lt $((full - 6)) ]]' || fail "volume 50 did not lower program ($full -> $(pgm_db) dB)"
pass "sound: slots heard, mute, audio follows video, volume"

# --- solo listening on the monitor bus: PFL before, AFL after the fader ---
mon_db() { curl -sf "$CORE/audio/levels" | jq '.monitor_peak_db // -100 | floor'; }
settled '[[ $(mon_db) -gt $AUDIBLE_DB ]]' || fail "monitor does not follow program without solo"
act '{"action":"set_audio","input":"Ball","on":false}' >/dev/null   # program now silent
act '{"action":"set_listen","input":"Bars","listen":"pfl"}' >/dev/null
settled '[[ $(pgm_db) -lt $SILENT_DB ]]' || fail "program not silent for the solo check"
settled '[[ $(mon_db) -gt $AUDIBLE_DB ]]' || fail "PFL of a muted input not heard ($(mon_db) dB)"
act '{"action":"set_listen","input":"Bars","listen":"afl"}' >/dev/null
settled '[[ $(mon_db) -lt $SILENT_DB ]]' || fail "AFL of a muted input heard ($(mon_db) dB)"
act '{"action":"clear_listen"}' >/dev/null
act '{"action":"set_audio","input":"Ball","on":true}' >/dev/null
settled '[[ $(mon_db) -gt $AUDIBLE_DB ]]' || fail "monitor not back on program after clearing solos"
pass "solo: PFL hears a muted input, AFL does not, monitor back on program"

# --- channel strip (4.7): trim lowers program, delay set while playing ---
act '{"action":"set_volume","input":"Ball","volume":100}' >/dev/null
settled '[[ $(pgm_db) -gt $AUDIBLE_DB ]]' || fail "Ball not heard at volume 100 ($(pgm_db) dB)"
full=$(steady_db pgm_db)
act '{"action":"set_channel_strip","input":"Ball","strip":{"gain_db":-20,"delay_ms":200}}' >/dev/null
settled '[[ $(pgm_db) -lt $((full - 15)) ]]' || fail "strip gain -20 dB did not lower program ($full -> $(pgm_db) dB)"
ball_slot="$(curl -sf "$CORE/state" | jq '.inputs[] | select(.name=="Ball") | .slot')"
offset="$(curl -sf "$STROM/api/flows/$MIXER_ID/elements/adelay$ball_slot/properties" | jq '.properties["ts-offset"]')"
[[ "$offset" == 200000000 ]] || fail "strip delay not in the engine (ts-offset $offset)"
[[ $(pgm_db) -gt $SILENT_DB ]] || fail "program silent after the delay change"
act '{"action":"set_channel_strip","input":"Ball","strip":{}}' >/dev/null
settled '[[ $(pgm_db) -ge $((full - 2)) ]]' || fail "strip reset did not restore program ($full -> $(pgm_db) dB)"
act '{"action":"set_volume","input":"Ball","volume":50}' >/dev/null   # checked after the crash below
pass "channel strip: gain lowers program, delay changed on air, reset"

# --- aux buses (4.7): bus 1 before the fader, bus 2 after it ---
aux_db() { curl -sf "$CORE/audio/levels" | jq ".aux_peak_db[$1] // -100 | floor"; }
act '{"action":"set_aux_send","input":"Ball","bus":1,"level":100}' >/dev/null
act '{"action":"set_aux_send","input":"Ball","bus":2,"level":100}' >/dev/null
settled '[[ $(aux_db 0) -gt $AUDIBLE_DB && $(aux_db 1) -gt $AUDIBLE_DB ]]' || fail "aux buses silent with sends up ($(aux_db 0), $(aux_db 1) dB)"
act '{"action":"set_audio","input":"Ball","on":false}' >/dev/null
settled '[[ $(aux_db 1) -lt $SILENT_DB ]]' || fail "aux 2 (after the fader) hears a muted input ($(aux_db 1) dB)"
[[ $(aux_db 0) -gt $AUDIBLE_DB ]] || fail "aux 1 (before the fader) lost a muted input"
act '{"action":"set_aux_bus","bus":1,"volume":100,"mute":true}' >/dev/null
settled '[[ $(aux_db 0) -lt $SILENT_DB ]]' || fail "muted aux 1 still sounds ($(aux_db 0) dB)"
act '{"action":"set_aux_bus","bus":1,"volume":100,"mute":false}' >/dev/null
act '{"action":"set_audio","input":"Ball","on":true}' >/dev/null
pass "aux buses: sends heard, before/after the fader, master mute"

# --- more sources (2b.3): color, still image, playlist with transport ---
MEDIA="$WORK/media"
mkdir -p "$MEDIA"
convert -size 1280x720 xc:'#2060c0' "$MEDIA/logo.png"
# Two clips with sound: a dark (ball), b bright (bars).
for clip in "a ball 330" "b smpte 660"; do
    set -- $clip
    gst-launch-1.0 -q videotestsrc num-buffers=300 pattern="$2" ! video/x-raw,width=640,height=360,framerate=30/1 \
        ! x264enc tune=zerolatency ! h264parse ! mp4mux name=m ! filesink location="$MEDIA/$1.mp4" \
        audiotestsrc num-buffers=470 freq="$3" volume=0.2 ! audioconvert ! avenc_aac ! aacparse ! m.
done
rgb() { convert "$1" -format "%[fx:int(mean.$2*255)]" info:; }

act '{"action":"add_input","source":{"kind":"color","color":"#ff8000"},"name":"Orange"}' >/dev/null
act "{\"action\":\"add_input\",\"source\":{\"kind\":\"image\",\"path\":\"$MEDIA/logo.png\"},\"name\":\"Logo\"}" >/dev/null
act "{\"action\":\"add_input\",\"source\":{\"kind\":\"playlist\",\"paths\":[\"$MEDIA/a.mp4\",\"$MEDIA/b.mp4\"]},\"name\":\"Clips\"}" >/dev/null
code="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$CORE/actions" -H 'content-type: application/json' \
    -d '{"action":"add_input","source":{"kind":"color","color":"orange"}}')"
[[ "$code" == 422 ]] || fail "malformed color accepted ($code)"
is_orange() { picture inputs/Orange "$WORK/orange.jpg"; [[ $(rgb "$WORK/orange.jpg" r) -gt 200 && $(rgb "$WORK/orange.jpg" b) -lt 50 ]]; }
wait_for "$SETTLE_S" is_orange || fail "color input not orange"
is_logo() { picture inputs/Logo "$WORK/logo.jpg"; [[ $(rgb "$WORK/logo.jpg" b) -gt 150 && $(rgb "$WORK/logo.jpg" r) -lt 80 ]]; }
wait_for "$SETTLE_S" is_logo || fail "image input not the logo"
clips_id="$(curl -sf "$CORE/state" | jq -r '.inputs[] | select(.name=="Clips") | .id')"
clips_db() { curl -sf "$CORE/audio/levels" | jq --arg id "$clips_id" '.inputs[] | select(.input==$id) | .peak_db // -100 | floor'; }
settled '[[ $(clips_db) -gt $AUDIBLE_DB ]]' || fail "playlist silent ($(clips_db) dB)"
pass "sources: color, still image, playlist with sound; bad color refused"

act '{"action":"add_input","source":{"kind":"whip","endpoint":"guest"},"name":"Guest"}' >/dev/null
whip_url="$(curl -sf "$CORE/state" | jq -r '.inputs[] | select(.name=="Guest") | .ingest.whip_url')"
[[ "$whip_url" == "$STROM/whip/guest" ]] || fail "WHIP link: $whip_url"
[[ "$(curl -s -o /dev/null -w '%{http_code}' -X OPTIONS "$whip_url")" == 200 ]] || fail "WHIP endpoint not served"
pass "WHIP input: ingest link in state, endpoint served"

clips_still() {
    picture inputs/Clips "$WORK/c1.jpg"
    sleep "$TAP_INTERVAL"
    picture inputs/Clips "$WORK/c2.jpg"
    cmp -s "$WORK/c1.jpg" "$WORK/c2.jpg"
}
picture inputs/Clips "$WORK/c0.jpg"   # switches the tap on before pausing
act '{"action":"transport_input","input":"Clips","transport":"pause"}' >/dev/null
wait_for "$SETTLE_S" clips_still || fail "paused playlist still moves"
act '{"action":"transport_input","input":"Clips","transport":"play"}' >/dev/null
clips_move() { ! clips_still; }
wait_for "$SETTLE_S" clips_move || fail "playlist does not play again"
# a is dark, b bright: next must flip the brightness either way.
picture inputs/Clips "$WORK/before.jpg"
act '{"action":"transport_input","input":"Clips","transport":"next"}' >/dev/null
switched() {
    picture inputs/Clips "$WORK/next.jpg"
    change=$(( $(brightness "$WORK/next.jpg") - $(brightness "$WORK/before.jpg") ))
    [[ ${change#-} -gt $MIN_BRIGHTNESS ]]
}
wait_for "$SETTLE_S" switched || fail "next did not switch clips (brightness change $change)"
pass "transport: pause, play, next"

# --- paused clip: strom repeats its last frame (no signal loss) ---
act '{"action":"cut_direct","input":"Clips"}' >/dev/null
# Straight to program from outside preview: the mixer must follow lmix.
mixer_pgm="$(curl -sf "$STROM/api/flows/$MIXER_ID/blocks/vm/state" | jq '.program_input')"
[[ "$mixer_pgm" == 4 ]] || fail "cut_direct: mixer program is input $mixer_pgm, not Clips (4)"
# Pause in the bright clip b, well before its end: start b afresh with
# "next" from the dark clip a (from b, "next" goes to a first). Paused
# near its end, b would roll over to a before the pause takes hold.
dark_clip() { picture inputs/Clips "$WORK/now.jpg"; [[ $(brightness "$WORK/now.jpg") -lt $MIN_BRIGHTNESS ]]; }
if ! wait_for 3 dark_clip; then
    act '{"action":"transport_input","input":"Clips","transport":"next"}' >/dev/null
    wait_for "$SETTLE_S" dark_clip || fail "next did not go to the dark clip a"
fi
act '{"action":"transport_input","input":"Clips","transport":"next"}' >/dev/null
wait_for "$SETTLE_S" bright inputs/Clips "$WORK/now.jpg" || fail "bright clip b does not play"
act '{"action":"transport_input","input":"Clips","transport":"pause"}' >/dev/null
wait_for "$SETTLE_S" clips_still || fail "playlist did not pause"
wait_for "$SETTLE_S" bright program "$WORK/hold.jpg" || fail "paused clip not on program"
# Held: still there two seconds later, not just passing through.
sleep 2
bright program "$WORK/hold.jpg" || fail "paused clip not held on program"
act '{"action":"transport_input","input":"Clips","transport":"play"}' >/dev/null
pass "paused clip held on program"

# Automatic playback (INP.11): paused once off air, plays again on air.
act '{"action":"set_auto_play","input":"Clips","auto_play":{"play":true,"restart":false,"pause":true}}' >/dev/null
wait_for "$SETTLE_S" clips_move || fail "playlist does not play before the automatic pause"
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
wait_for "$SETTLE_S" clips_still || fail "clip off air not paused automatically"
act '{"action":"cut_direct","input":"Clips"}' >/dev/null
wait_for "$SETTLE_S" clips_move || fail "clip on air does not play automatically"
act '{"action":"set_auto_play","input":"Clips","auto_play":{"play":false,"restart":false,"pause":false}}' >/dev/null
pass "automatic playback: paused off air, plays on air"

# --- outputs (M3): recording and SRT, each in its own flow ---
# Codecs of a media file, sorted and comma separated (needs a finalized file).
codecs() { ffprobe -v error -show_entries stream=codec_name -of csv=p=0 "$1" 2>/dev/null | sort | paste -sd, -; }
# MP4: unreadable unless the stop drained the recorder with EOS.
act '{"action":"add_output","spec":{"kind":"record","folder":"rec","container":"mp4"},"name":"Rec"}' >/dev/null
act '{"action":"add_output","spec":{"kind":"srt","uri":"srt://:9911?mode=listener"},"name":"Stream"}' >/dev/null
act '{"action":"start_output","output":"Rec"}' >/dev/null
act '{"action":"start_output","output":"Stream"}' >/dev/null
output_state() { curl -sf "$CORE/state" | jq -r ".outputs[] | select(.name==\"$1\") | .status.state"; }
settled '[[ "$(output_state Rec)" == running && "$(output_state Stream)" == running ]]' || fail "outputs not running"
timeout "$((SETTLE_S + 5))" gst-launch-1.0 -q srtsrc uri="srt://127.0.0.1:9911" ! filesink location="$WORK/srt.ts" &
receiver=$!
# Link health while the receiver is connected (B32).
stream_link() { curl -sf "$CORE/outputs/stats" | jq -e '.[] | select(.connected and .kbps > 0)' >/dev/null; }
wait_for "$SETTLE_S" stream_link || fail "no link health for the SRT output: $(curl -sf "$CORE/outputs/stats")"
wait "$receiver" || true
[[ $(stat -c %s "$WORK/srt.ts" 2>/dev/null || echo 0) -gt 50000 ]] || fail "SRT output delivered nothing"
act '{"action":"stop_output","output":"Rec"}' >/dev/null
wait_for "$SETTLE_S" is output_state Rec stopped || fail "recording did not stop"
recording="$(find "$WORK/strom-data" -path '*rec/*' -name '*.mp4' | head -1)"
[[ -n "$recording" ]] || fail "no recording file"
[[ "$(codecs "$recording")" == "aac,h264" ]] || fail "recording lacks H.264 or AAC: $(codecs "$recording")"
pass "outputs: recording (H.264 + AAC, finalized MP4), SRT stream received with link health, start/stop"

# MultiCorder: ISO recordings of single inputs, with and without sound.
input_id() { curl -sf "$CORE/state" | jq -r ".inputs[] | select(.name==\"$1\") | .id"; }
for iso in Orange Clips; do
    act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"record\",\"folder\":\"iso-$iso\",\"container\":\"mp4\"},\"source\":{\"from\":\"input\",\"input\":\"$(input_id $iso)\"},\"name\":\"ISO $iso\"}" >/dev/null
    act "{\"action\":\"start_output\",\"output\":\"ISO $iso\"}" >/dev/null
done
sleep 4
for iso in Orange Clips; do act "{\"action\":\"stop_output\",\"output\":\"ISO $iso\"}" >/dev/null; done
for iso in Orange Clips; do
    wait_for "$SETTLE_S" is output_state "ISO $iso" stopped || fail "ISO $iso did not stop"
done
iso_file() { find "$WORK/strom-data" -path "*iso-$1/*" -name '*.mp4' | head -1; }
[[ "$(codecs "$(iso_file Orange)")" == "h264" ]] || fail "ISO of a silent input: $(codecs "$(iso_file Orange)")"
[[ "$(codecs "$(iso_file Clips)")" == "aac,h264" ]] || fail "ISO of the playlist: $(codecs "$(iso_file Clips)")"
pass "MultiCorder: ISO recordings per input (silent input video only, playlist with sound)"

# Display output: program in a window on the engine's screen. Orange on
# program makes the window easy to recognize.
act '{"action":"cut_direct","input":"Orange"}' >/dev/null
act '{"action":"add_output","spec":{"kind":"display","mode":{"mode":"window"}},"name":"Screen"}' >/dev/null
act '{"action":"start_output","output":"Screen"}' >/dev/null
screen_orange() {
    DISPLAY="$ENGINE_DISPLAY" import -window root "$WORK/screen.png"
    [[ $(rgb "$WORK/screen.png" r) -gt 200 && $(rgb "$WORK/screen.png" b) -lt 60 ]]
}
wait_for "$SETTLE_S" screen_orange || true
[[ "$(output_state Screen)" == running ]] && screen_orange || fail "display output does not show program"
act '{"action":"stop_output","output":"Screen"}' >/dev/null
pass "display output: program in a window on the engine screen"

# --- failover (3.4): a source dies mid-transition, a stream breaks ---
input_state() { curl -sf "$CORE/state" | jq -r ".inputs[] | select(.name==\"$1\") | .status"; }
flow_id() { curl -sf "$STROM/api/flows" | jq -r ".flows[] | select(.name==\"$1\") | .id"; }
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
act '{"action":"preview_input","input":"Orange"}' >/dev/null
act '{"action":"set_transition","slot":1,"effect":"fade","duration_ms":3000}' >/dev/null
act '{"action":"transition","slot":1}' >/dev/null
sleep 1
orange_slot="lmix slot $(curl -sf "$CORE/state" | jq '.inputs[] | select(.name=="Orange") | .slot')"
curl -sf -X POST "$STROM/api/flows/$(flow_id "$orange_slot")/stop" >/dev/null
# Hold: while the input is offline, program keeps fading to its last
# frame (CPU compositor in the VM may show one black frame at the loss,
# HARDWARE-TODO). The source fails back after ~2 s on a slow runner:
# offline and the picture are checked in one loop, the state first, so a
# picture taken while offline counts.
saw_offline=""
held_orange() {
    local state
    state="$(input_state Orange)"
    [[ "$state" == offline ]] && saw_offline=1
    picture program "$WORK/held.jpg"
    [[ "$state" == offline && $(rgb "$WORK/held.jpg" r) -gt 150 ]]
}
if ! wait_for "$SETTLE_S" held_orange; then
    [[ -n "$saw_offline" ]] || fail "dead source not marked offline"
    fail "program lost the orange while the source was gone"
fi
running="$(curl -sf "$STROM/api/flows/$MIXER_ID" | jq -r '.flow.running')"
[[ "$running" == true ]] || fail "mixer stopped after source loss"
wait_for "$((SETTLE_S + 15))" is input_state Orange online || fail "source did not fail back"
orange_back() { picture inputs/Orange "$WORK/back.jpg"; [[ $(rgb "$WORK/back.jpg" r) -gt 200 ]]; }
wait_for "$SETTLE_S" orange_back || fail "orange not back after failback"
pass "failover: source died mid-transition, program held its last frame, input failed back"

# Signal loss per input: hold the last frame (default, above) or black.
# A paused clip keeps sending its frame, so the source has to die.
act '{"action":"set_signal_loss","input":"Orange","mode":"black"}' >/dev/null
act '{"action":"cut_direct","input":"Orange"}' >/dev/null
orange_pgm() { picture program "$WORK/orange-pgm.jpg"; [[ $(rgb "$WORK/orange-pgm.jpg" r) -gt 200 ]]; }
wait_for "$SETTLE_S" orange_pgm || fail "orange not on program before the loss"
curl -sf -X POST "$STROM/api/flows/$(flow_id "$orange_slot")/stop" >/dev/null
black_pgm() { picture program "$WORK/black.jpg"; [[ $(brightness "$WORK/black.jpg") -lt 5 ]]; }
wait_for "$SETTLE_S" black_pgm || fail "dead source not black on program ($(brightness "$WORK/black.jpg"))"
act '{"action":"set_signal_loss","input":"Orange","mode":"hold"}' >/dev/null
wait_for "$((SETTLE_S + 15))" is input_state Orange online || fail "source did not fail back after the black check"
wait_for "$SETTLE_S" orange_back || fail "orange not back after the black check"
pass "signal loss: black when set, the source died"

act '{"action":"start_output","output":"Stream"}' >/dev/null
sleep 2   # running for a while, then the link breaks
stream_flow="$(curl -sf "$STROM/api/flows" | jq -r '[.flows[] | select(.name | startswith("lmix output")) | select(.running)][0].id')"
curl -sf -X POST "$STROM/api/flows/$stream_flow/stop" >/dev/null
wait_for "$SETTLE_S" is output_state Stream failed || fail "broken stream not marked failed"
wait_for "$((SETTLE_S + 15))" is output_state Stream running || fail "stream did not restart"
act '{"action":"stop_output","output":"Stream"}' >/dev/null
pass "failover: stream broke, was marked failed and restarted itself"

# --- overlays (4.1): an input above program, tally, take off ---
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
overlay_shown() { picture program "$WORK/ovl.jpg"; [[ $(rgb "$WORK/ovl.jpg" r) -gt 200 ]]; }
overlay_gone() { picture program "$WORK/ovl.jpg"; [[ $(rgb "$WORK/ovl.jpg" r) -lt 60 ]]; }
# Orange was on program before: wait for Ball, or a late picture passes.
wait_for "$SETTLE_S" overlay_gone || fail "ball not on program before the overlay"
act '{"action":"overlay_in","channel":1,"input":"Orange","duration_ms":0}' >/dev/null
wait_for "$SETTLE_S" overlay_shown || fail "overlay 1 not on program"
orange_number="$(curl -sf "$CORE/state" | jq '.inputs[] | select(.name=="Orange") | .number')"
[[ "$(curl -sf "$CORE/tally" | jq ".[$((orange_number - 1))]")" == 1 ]] || fail "overlay input has no program tally"
[[ "$(curl -sf "$CORE/state" | jq -r '.overlays[0] != null')" == true ]] || fail "overlay not in state"
act '{"action":"overlay_out","channel":1,"duration_ms":500}' >/dev/null
wait_for "$SETTLE_S" overlay_gone || fail "overlay 1 not taken off"
pass "overlays: input above program, red tally, faded off"
# All off and the last input again (OVL.8).
act '{"action":"overlay_last","channel":1,"duration_ms":0}' >/dev/null
wait_for "$SETTLE_S" overlay_shown || fail "last input of overlay 1 not back"
# Back on while the fade off still runs: the last command wins, the end
# of the fade must not take it off again.
act '{"action":"overlay_out","channel":1,"duration_ms":500}' >/dev/null
act '{"action":"overlay_last","channel":1,"duration_ms":0}' >/dev/null
sleep 1.5
overlay_shown || fail "overlay taken off by the end of an older fade"
act '{"action":"overlay_out_all","duration_ms":0}' >/dev/null
wait_for "$SETTLE_S" overlay_gone || fail "overlays not all off"
pass "overlays: last input again, all off"
code="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$CORE/actions" -H 'content-type: application/json' \
    -d '{"action":"set_channel_strip","input":"Orange","strip":{}}')"
[[ "$code" != 200 ]] || fail "channel strip accepted for an input without sound"

# --- T-bar (4.3): preview mixed in by hand, taken at the end ---
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
act '{"action":"preview_input","input":"Orange"}' >/dev/null
act '{"action":"t_bar","position":0.5}' >/dev/null
half_mixed() { picture program "$WORK/tbar.jpg"; local r; r=$(rgb "$WORK/tbar.jpg" r); ((r > 80 && r < 200)); }
wait_for "$SETTLE_S" half_mixed || fail "T-bar at half: program red $(rgb "$WORK/tbar.jpg" r), not a mix"
[[ "$(curl -sf "$CORE/state" | jq '.t_bar')" == 0.5 ]] || fail "T-bar position not in state"
act '{"action":"t_bar","position":1}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq -r '[.inputs[] | select(.name=="Orange") | .id][0] == .program')" == true ]] \
    || fail "T-bar at the end did not take preview to program"
wait_for "$SETTLE_S" overlay_shown || fail "program not orange after the T-bar take"
pass "T-bar: preview mixed in by half, taken at the end"

# --- stinger (4.2): a clip with alpha over program, cut underneath ---
# Blue square in the middle, the rest transparent; 2 s.
ffmpeg -loglevel error -y -f lavfi -i "color=c=black@0.0:s=1280x720:r=30:d=2,format=yuva420p[bg];color=c=blue:s=400x400:r=30:d=2,format=yuva420p[box];[bg][box]overlay=440:160:format=yuv420" \
    -c:v libvpx-vp9 -pix_fmt yuva420p -auto-alt-ref 0 "$MEDIA/sting.webm"
act "{\"action\":\"add_input\",\"source\":{\"kind\":\"file\",\"path\":\"$MEDIA/sting.webm\"},\"name\":\"Sting\"}" >/dev/null
act '{"action":"set_transition","slot":4,"effect":"stinger","duration_ms":3000,"stinger":{"input":"Sting","cut_ms":1500}}' >/dev/null
act '{"action":"preview_input","input":"Ball"}' >/dev/null   # program is Orange
sleep 2
act '{"action":"transition","slot":4}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq -r '[.inputs[] | select(.name=="Ball") | .id][0] == .program')" == true ]] \
    || fail "stinger did not take preview to program"
code="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$CORE/actions" -H 'content-type: application/json' -d '{"action":"cut"}')"
[[ "$code" != 200 ]] || fail "cut accepted while the stinger plays"
middle() { convert "$1" -gravity center -crop 10x10+0+0 +repage -format "%[fx:int(mean.b*255)] %[fx:int(mean.r*255)]" info:; }
corner_red() { convert "$1" -crop 10x10+0+0 +repage -format "%[fx:int(mean.r*255)]" info:; }
stinger_over_orange() {
    picture program "$WORK/sting.jpg"
    local b r; read -r b r <<<"$(middle "$WORK/sting.jpg")"
    ((b > 180 && r < 80)) && [[ $(corner_red "$WORK/sting.jpg") -gt 200 ]]
}
wait_for "$SETTLE_S" stinger_over_orange || fail "stinger clip not over program (alpha around it)"
stinger_gone() {
    picture program "$WORK/sting.jpg"
    local b r; read -r b r <<<"$(middle "$WORK/sting.jpg")"
    ((b < 120)) && [[ $(rgb "$WORK/sting.jpg" r) -lt 60 ]]
}
wait_for "$SETTLE_S" stinger_gone || fail "stinger did not end on the new program"
act '{"action":"remove_input","input":"Sting"}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq -r '.transitions[3].effect')" == cut ]] || fail "stinger button kept a removed clip"
pass "stinger: clip with alpha over program, cut underneath, locked while it plays"

# --- looks (4.6): the CPU mixer has none; the action says so ---
[[ "$(curl -sf "$CORE/state" | jq '.looks_available')" == false ]] || fail "CPU mixer claims looks"
code="$(curl -s -o "$WORK/look.json" -w '%{http_code}' -X POST "$CORE/actions" -H 'content-type: application/json' \
    -d '{"action":"set_look","input":"Ball","look":{"brightness":0.3}}')"
[[ "$code" != 200 ]] && grep -q "GL mixer" "$WORK/look.json" || fail "look on the CPU mixer: $code $(cat "$WORK/look.json")"
pass "looks: refused with a reason on the CPU mixer (GL check: scripts/test-looks-gl.sh)"

# --- layouts (4.6): Orange behind, Ball in the top right quarter ---
act '{"action":"add_layout","name":"Interview","background":"Orange","boxes":[{"rect":{"x":0.5,"y":0.0,"w":0.5,"h":0.5},"input":"Ball","border_px":4}]}' >/dev/null
act '{"action":"cut_direct","input":"Bars"}' >/dev/null
act '{"action":"preview_layout","layout":"Interview"}' >/dev/null
act '{"action":"cut"}' >/dev/null
layout_id="$(curl -sf "$CORE/state" | jq -r '.layouts[0].id')"
[[ "$(curl -sf "$CORE/state" | jq -r '.program')" == "$layout_id" ]] || fail "layout not on program"
on_air() { curl -sf "$CORE/tally" | jq ".[$(($(curl -sf "$CORE/state" | jq ".inputs[] | select(.name==\"$1\") | .number") - 1))]"; }
[[ "$(on_air Orange)" == 1 && "$(on_air Ball)" == 1 ]] || fail "inputs of the layout on program have no red tally"
quarter() { convert "$1" -crop "$2" +repage -format '%[fx:int(mean.r*255)]' info:; }
layout_shown() {
    picture program "$WORK/layout.jpg"
    local w h; read -r w h <<<"$(identify -format '%w %h' "$WORK/layout.jpg")"
    # Bottom left: the orange background; top right: the ball (dark).
    [[ $(quarter "$WORK/layout.jpg" "$((w / 4))x$((h / 4))+0+$((h * 3 / 4))") -gt 200 ]] \
        && [[ $(quarter "$WORK/layout.jpg" "$((w / 4))x$((h / 4))+$((w * 5 / 8))+$((h / 8))") -lt 100 ]]
}
wait_for "$SETTLE_S" layout_shown || fail "layout picture: background and box not where they belong"
# Crop: the right half of Orange fills a full-picture box at its own
# aspect (strom fits it), so the left edge turns black.
act '{"action":"set_layout","layout":1,"background":null,"boxes":[{"rect":{"x":0,"y":0,"w":1,"h":1},"input":"Orange","crop":{"left":0.5}}]}' >/dev/null
cropped() {
    picture program "$WORK/layout.jpg"
    local w h; read -r w h <<<"$(identify -format '%w %h' "$WORK/layout.jpg")"
    [[ $(quarter "$WORK/layout.jpg" "$((w / 16))x$((h / 4))+0+$((h * 3 / 8))") -lt 60 ]] \
        && [[ $(quarter "$WORK/layout.jpg" "$((w / 8))x$((h / 4))+$((w * 7 / 16))+$((h * 3 / 8))") -gt 200 ]]
}
wait_for "$SETTLE_S" cropped || fail "layout crop not applied"
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
act '{"action":"remove_layout","layout":1}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq '.layouts | length')" == 0 ]] || fail "layout not removed"
pass "layouts: background and box on program, red tally for both inputs, crop, removed"

# --- automation (AUT.7, AUT.14): a cue list whose cues fire triggers ---
act '{"action":"set_triggers","input":"Orange","triggers":[{"on":"program","delay_ms":500,"actions":[{"action":"overlay_in","channel":2,"input":"Ball","duration_ms":0}]},{"on":"program_off","actions":[{"action":"overlay_out","channel":2,"duration_ms":0}]}]}' >/dev/null
act '{"action":"add_cue_list","name":"Run","cues":[{"label":"Orange","actions":[{"action":"cut_direct","input":"Orange"}]},{"label":"Ball","actions":[{"action":"cut_direct","input":"Ball"}]}]}' >/dev/null
state_is() { [[ "$(curl -sf "$CORE/state" | jq -r "$1")" == true ]]; }
id_of() { curl -sf "$CORE/state" | jq -r ".inputs[] | select(.name==\"$1\") | .id"; }
orange_id="$(id_of Orange)"; ball_id="$(id_of Ball)"
act '{"action":"cue_go","list":"Run"}' >/dev/null
state_is ".program == \"$orange_id\" and .cue_lists[0].next == 2" || fail "cue 1 did not take Orange"
wait_for 4 state_is ".overlays[1] == \"$ball_id\"" || fail "trigger of Orange did not put Ball on overlay 2"
act '{"action":"cue_go","list":"Run"}' >/dev/null
state_is ".program == \"$ball_id\"" || fail "cue 2 did not take Ball"
wait_for 4 state_is ".overlays[1] == null" || fail "leaving program did not take overlay 2 off"
act '{"action":"cue_back","list":"Run"}' >/dev/null
state_is ".program == \"$orange_id\" and .cue_lists[0].next == 2" || fail "back did not run cue 1"
wait_for 4 state_is ".overlays[1] == \"$ball_id\"" || fail "trigger did not fire again after back"
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
act '{"action":"remove_cue_list","list":"Run"}' >/dev/null
act '{"action":"set_triggers","input":"Orange","triggers":[]}' >/dev/null
wait_for 4 state_is ".overlays[1] == null and (.cue_lists | length) == 0" || fail "automation not cleaned up"
pass "automation: cues GO and back, triggers on and off program"
# Clip end (AUT.7): Clips (two 10 s clips, looped) puts Orange on overlay 3
# two seconds before its end and takes it off at the end.
act '{"action":"set_triggers","input":"Clips","triggers":[{"on":"clip_end","before_ms":2000,"actions":[{"action":"overlay_in","channel":3,"input":"Orange","duration_ms":0}]},{"on":"clip_end","actions":[{"action":"overlay_out","channel":3,"duration_ms":0}]}]}' >/dev/null
wait_for "$((SETTLE_S + 20))" state_is ".overlays[2] == \"$orange_id\"" || fail "countdown before the clip end did not fire"
wait_for "$SETTLE_S" state_is ".overlays[2] == null" || fail "clip end did not fire"
act '{"action":"set_triggers","input":"Clips","triggers":[]}' >/dev/null
pass "clip end: countdown and end of a playlist fire their triggers"

# --- latency (NFA.3): capture-to-program, CPU mixer in a VM ---
# Budget here is generous (software compositing, no GPU); the real target
# of two frames is checked on hardware (HARDWARE-TODO).
LATENCY_BUDGET_MS="${LATENCY_BUDGET_MS:-150}"
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
curl -sf -X POST "$CORE/latency/measure?seconds=5" >/dev/null || fail "latency measurement not started"
sleep 6
latency="$(curl -sf "$CORE/latency")"
samples="$(jq -r '.samples' <<<"$latency")"
median="$(jq -r '.median_ms' <<<"$latency")"
p95="$(jq -r '.p95_ms' <<<"$latency")"
[[ "$samples" -ge 3 ]] || fail "too few latency samples: $latency"
[[ "$p95" -le "$LATENCY_BUDGET_MS" ]] || fail "latency p95 ${p95} ms over ${LATENCY_BUDGET_MS} ms: $latency"
pass "latency: median ${median} ms, p95 ${p95} ms from ${samples} samples (budget ${LATENCY_BUDGET_MS} ms)"

# --- input color, device list ---
act '{"action":"set_input_color","input":"Ball","color":"cyan"}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq -r '.inputs[1].color')" == cyan ]] || fail "color not set"
curl -sf "$CORE/devices" | jq -e 'type == "array"' >/dev/null || fail "device list"
pass "color tag; device list ($(curl -sf "$CORE/devices" | jq length) found)"

# --- Console as lmix-core client, on a virtual display ---
if [[ -n "${CONSOLE_SHOT:-}" ]]; then
    # Qt Console (needs Qt 6 with Kirigami and gst-plugin-qml6, BUILD.md).
    (cd "$ROOT/workspace/console-qt" && cargo build --release)
    CONSOLE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/console-qt/target}/release/lmix-console"
    Xvfb :93 -screen 0 1440x900x24 >/dev/null 2>&1 &
    PIDS+=($!)
    sleep 1
    DISPLAY=:93 WAYLAND_DISPLAY= QT_QPA_PLATFORM=xcb QT_FORCE_STDERR_LOGGING=1 \
        LMIX_CORE="${CORE%/api}" "$CONSOLE_BIN" >"$WORK/console.log" 2>&1 &
    PIDS+=($!)
    sleep "${CONSOLE_WAIT:-25}"   # live flows start one after another
    act '{"action":"set_listen","input":"Bars","listen":"pfl"}' >/dev/null   # show a solo
    act '{"action":"start_output","output":"Stream"}' >/dev/null           # and a running output
    sleep 2
    DISPLAY=:93 import -window root "$CONSOLE_SHOT"
    act '{"action":"clear_listen"}' >/dev/null
    pass "Console screenshot: $CONSOLE_SHOT"
fi

# --- show file ---
act "{\"action\":\"save_show\",\"path\":\"$WORK/test.lmix\"}" >/dev/null
[[ "$(curl -sf "$CORE/shows/recent" | jq -r '.[0]')" == "$WORK/test.lmix" ]] || fail "recent shows"
act '{"action":"new_show"}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq '.inputs | length')" == 0 ]] || fail "new show not empty"
act "{\"action\":\"open_show\",\"path\":\"$WORK/test.lmix\"}" >/dev/null
settled '[[ "$(curl -sf "$CORE/state" | jq ".inputs | length")" == 6 ]]' || fail "show not restored: $(curl -sf "$CORE/state")"
pass "show saved, cleared, reopened"

# An output running across the crash below must be adopted, not restarted.
act '{"action":"start_output","output":"Stream"}' >/dev/null
output_starts() { grep -c "Starting pipeline: lmix output" "$WORK/strom.log" || true; }
starts_before="$(output_starts)"

# --- crash of lmix-core: engine keeps running, show comes back ---
kill -9 "$CORE_PID"
sleep 1
running="$(curl -sf "$STROM/api/flows/$MIXER_ID" | jq -r '.flow.running')"
[[ "$running" == true ]] || fail "mixer died with lmix-core"
start_core
settled '[[ "$(curl -sf "$CORE/state" | jq ".inputs | length")" == 6 ]]' || fail "autosave not resumed"
[[ "$(curl -sf "$CORE/state" | jq '.inputs[1].audio.volume')" == 50 ]] || fail "audio settings not resumed"
wait_for "$SETTLE_S" is output_state Stream running || fail "running output not adopted after lmix-core restart"
[[ "$(curl -sf "$STROM/api/flows" | jq '[.flows[] | select(.name | startswith("lmix output"))] | length')" == 1 ]] \
    || fail "output flow duplicated"
[[ "$(output_starts)" == "$starts_before" ]] || fail "output restarted instead of adopted"
pass "lmix-core killed and restarted: mixer kept running, show resumed, output adopted"

criticals="$(grep -c 'GStreamer-CRITICAL' "$WORK/strom.log" || true)"
[[ "$criticals" == 0 ]] || fail "$criticals GStreamer CRITICALs in strom.log"
pass "no GStreamer CRITICALs"

echo "M2b VM check passed"
