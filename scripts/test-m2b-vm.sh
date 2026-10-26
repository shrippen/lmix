#!/usr/bin/env bash
# M2b check without GPU: strom (CPU mixer) + lmix-core, driven over the
# lmix API. Proves slot flows, preview/take, source swap and show file.
# Picture quality, real latency and GPU paths: docs/HARDWARE-TODO.md.
# With MIXER=cuda or gl the same checks run on a real GPU (acceptance on
# hardware, H.27 ff.); they stay out of CI.
#
#   scripts/test-m2b-vm.sh            # builds, runs, cleans up
#   CONSOLE_SHOT=out.png scripts/test-m2b-vm.sh
#                                     # also screenshots lmix (Mix page)
#                                     # (lmix-core client) on Xvfb
#   KEEP_WORK=1                       # keep logs and pictures
#   WORK=dir                          # put them there (default: mktemp)
#   LATENCY_BUDGET_MS=150             # p95 capture-to-program budget
#   SETTLE_S=20                       # how long a check waits for its state
#   STROM_GST_DEBUG=compositor:4      # GST_DEBUG for strom (gst.log)
#   STROM_BIN=…/strom                 # skip scripts/fetch-strom.sh
#   MIXER=cpu|cuda|gl                 # mixer (default cpu, software GL)
#                                     #   cuda: NVIDIA; display output on Xvfb
#                                     #   gl: the GPU's GL on this desktop
#                                     #   session (needs WAYLAND_DISPLAY);
#                                     #   its display output opens there and
#                                     #   is not checked; SETTLE_S=30 under
#                                     #   other GPU load (19.10.)
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
SETTLE_S="${SETTLE_S:-20}"
# No request waits for ever: a hung one held CI for 2 h (todo.md). A
# --max-time of its own (act, picture) comes later and wins.
CURL_MAX_S=30
curl() { command curl --max-time "$CURL_MAX_S" "$@"; }
MIXER="${MIXER:-cpu}"
case "$MIXER" in
    cpu | cuda) ;;
    gl) [[ -n "${WAYLAND_DISPLAY:-}" ]] || { echo "MIXER=gl needs a Wayland session (WAYLAND_DISPLAY)" >&2; exit 2; } ;;
    *) echo "MIXER is cpu, cuda or gl, not $MIXER" >&2; exit 2 ;;
esac

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
    for log in strom.log core.log gst.log core-threads.txt strom-threads.txt; do
        [[ -s "$WORK/$log" ]] || continue
        echo "--- last 60 lines of $log" >&2
        tail -n 60 "$WORK/$log" >&2
    done
    # Earlier trouble scrolls out of the tails (thumbnail polls fill them).
    echo "--- last 40 warnings and errors of strom.log" >&2
    grep -E 'WARN|ERROR' "$WORK/strom.log" | grep -v '/thumbnail' | tail -n 40 >&2 || true
    # lmix-core answers a picture request from its last picture while the
    # engine has none (504): a frozen program may be a stuck tap.
    [[ -n "${MIXER_ID:-}" ]] && echo "--- strom program tap: HTTP $(curl -s -o /dev/null -w '%{http_code}' --max-time 5 \
        "$STROM/api/flows/$MIXER_ID/blocks/tap_pgm/thumbnail")" >&2
    echo "--- strom flows" >&2
    curl -sf --max-time 5 "$STROM/api/flows" \
        | jq -c '.flows[] | {name, id, state, running}' >&2 || true
    exit 1
}
pass() { echo "ok   $*"; }
wait_for() {   # wait_for <seconds> <command…>: until the command succeeds
    # By the clock, not by tries: slow tries (pictures under load) made a
    # wait of 20 s take half an hour (todo.md).
    local deadline=$((SECONDS + $1)); shift
    until "$@"; do
        ((SECONDS < deadline)) || return 1
        sleep 0.5
    done
}
# settled <condition>: polls a bash condition (as for eval) until it holds.
settled() { wait_for "$SETTLE_S" eval "$1"; }
# is <state function> <name> <state>: evaluated anew on every try.
is() { [[ "$("$1" "$2")" == "$3" ]]; }

# A hung lmix-core (seen once, todo.md) fails the action after two
# minutes and leaves its threads in the work folder.
act() {
    local out code
    out="$(curl -s --max-time 120 -w '\n%{http_code}' -X POST "$CORE/actions" -H 'content-type: application/json' -d "$1")"
    code="${out##*$'\n'}"
    if [[ "$code" == 000 && -n "${CORE_PID:-}" ]]; then
        # lmix-core may wait on an engine call that hangs: both sides.
        gdb -p "$CORE_PID" -batch -ex 'thread apply all bt' >"$WORK/core-threads.txt" 2>&1 || true
        local strom_pid
        strom_pid="$(pgrep -f -- "--port $STROM_PORT " | head -1)"
        [[ -n "$strom_pid" ]] && { gdb -p "$strom_pid" -batch -ex 'thread apply all bt' >"$WORK/strom-threads.txt" 2>&1 || true; }
        fail "action $1 got no answer in 120 s; threads: $WORK/core-threads.txt, $WORK/strom-threads.txt"
    fi
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
STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
(cd "$ROOT/workspace/lmix-core" && cargo build)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/debug/lmix-core"

# --- start ---
# A screen for display outputs (engine side). On a desktop the engine
# would open its windows there (Wayland first), so it only gets this one.
# The GL mixer needs the GPU's GL, which Xvfb does not have: it stays on
# the desktop session.
ENGINE_DISPLAY=:94
Xvfb "$ENGINE_DISPLAY" -screen 0 1280x720x24 >/dev/null 2>&1 &
PIDS+=($!)
case "$MIXER" in
    cpu) ENGINE_ENV=(-u WAYLAND_DISPLAY GST_GL_WINDOW=x11 DISPLAY="$ENGINE_DISPLAY" LIBGL_ALWAYS_SOFTWARE=1) ;;
    cuda) ENGINE_ENV=(-u WAYLAND_DISPLAY GST_GL_WINDOW=x11 DISPLAY="$ENGINE_DISPLAY") ;;
    gl) ENGINE_ENV=(GST_GL_WINDOW=wayland) ;;
esac
(cd "$WORK" && GST_DEBUG="${STROM_GST_DEBUG:-}" GST_DEBUG_FILE="$WORK/gst.log" GST_DEBUG_NO_COLOR=1 exec env "${ENGINE_ENV[@]}" "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "$STROM/health" strom

start_core() {
    "$CORE_BIN" --strom "$STROM" --listen "127.0.0.1:${CORE_PORT}" --mixer "$MIXER" \
        --slots "$SLOTS" --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" --layout-templates "$WORK/layout-templates.json" >>"$WORK/core.log" 2>&1 &
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
    local path="$1" out="$2" deadline=$((SECONDS + SETTLE_S))
    rm -f "$out"
    until curl -sf --max-time 10 "$CORE/pictures/$path" -o "$out"; do
        ((SECONDS < deadline)) || break
        sleep 1
    done
    # A slot restarting (failover, signal loss) has no picture for a
    # while: one failed try of a wait_for, not the end of the check.
    [[ -s "$out" ]] || { echo "no picture $path (yet)" >&2; return 1; }
}
# Mean brightness 0-100. A black frame is a valid JPEG of normal size,
# so pictures are judged by content, never by bytes.
brightness() { convert "$1" -colorspace gray -format '%[fx:int(mean*100)]' info:; }
# Program changes between two pictures: live frames, not a still.
program_moves() {
    picture program "$WORK/a.jpg" || return 1
    sleep "$TAP_INTERVAL"
    picture program "$WORK/b.jpg" || return 1
    ! cmp -s "$WORK/a.jpg" "$WORK/b.jpg"
}

# bright <picture path> <file>: the picture shows content, not black.
bright() { picture "$1" "$2" || return 1; [[ $(brightness "$2") -gt $MIN_BRIGHTNESS ]]; }
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
mon_db() { curl -sf "$CORE/audio/levels" | jq '.phones_peak_db // -100 | floor'; }
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
act '{"action":"set_phones","volume":0,"mute":false}' >/dev/null
settled '[[ $(mon_db) -lt $SILENT_DB ]]' || fail "monitor volume 0 still heard ($(mon_db) dB)"
[[ $(pgm_db) -gt $AUDIBLE_DB ]] || fail "monitor volume took program down too"
act '{"action":"set_phones","volume":100,"mute":false}' >/dev/null
settled '[[ $(mon_db) -gt $AUDIBLE_DB ]]' || fail "monitor not back at volume 100"
pass "solo: PFL hears a muted input, AFL does not, monitor back on program, own volume"

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

# --- level fade (AUD.14): the master down over 2 s, back at once ---
full=$(steady_db pgm_db)
act '{"action":"fade_volume","target":"master","volume":0,"duration_ms":2000}' >/dev/null
sleep 0.7
mid=$(pgm_db)
(( mid > SILENT_DB )) || fail "master fade cut at once instead of fading ($full -> $mid dB after 0.7 s)"
settled '[[ $(pgm_db) -lt $SILENT_DB ]]' || fail "master fade did not reach silence ($(pgm_db) dB)"
act '{"action":"fade_volume","target":"master","volume":100,"duration_ms":0}' >/dev/null
settled '[[ $(pgm_db) -ge $((full - 2)) ]]' || fail "master not back ($full -> $(pgm_db) dB)"
pass "level fade: master down over 2 s, back at once"

# --- buses (4.7, AUD.2): named, before or after the fader ---
# Each new bus builds the mixer anew; program comes back. Three in a row
# are three builds: program counts as back when it stays up for 3 s, not
# in a gap between two builds (CI, 25.10.).
act '{"action":"add_bus","name":"In-Ear","pre_fader":true}' >/dev/null
act '{"action":"add_bus","name":"Stream"}' >/dev/null
act '{"action":"add_bus","name":"Spare"}' >/dev/null
bus_db() { curl -sf "$CORE/audio/levels" | jq ".buses[$1].peak_db // -100 | floor"; }
program_stays() {
    local _
    for _ in 1 2 3; do
        [[ $(pgm_db) -gt $AUDIBLE_DB ]] || return 1
        sleep 1
    done
}
settled program_stays || fail "program silent after the mixer got its buses ($(pgm_db) dB)"
act '{"action":"set_send","input":"Ball","bus":"In-Ear","level":100}' >/dev/null
act '{"action":"set_send","input":"Ball","bus":"Stream","level":100}' >/dev/null
settled '[[ $(bus_db 0) -gt $AUDIBLE_DB && $(bus_db 1) -gt $AUDIBLE_DB ]]' || fail "buses silent with sends up ($(bus_db 0), $(bus_db 1) dB)"
[[ $(bus_db 2) -lt $SILENT_DB ]] || fail "a bus without sends sounds ($(bus_db 2) dB)"
act '{"action":"set_audio","input":"Ball","on":false}' >/dev/null
settled '[[ $(bus_db 1) -lt $SILENT_DB ]]' || fail "Stream (after the fader) hears a muted input ($(bus_db 1) dB)"
[[ $(bus_db 0) -gt $AUDIBLE_DB ]] || fail "In-Ear (before the fader) lost a muted input"
act '{"action":"set_bus","bus":"In-Ear","volume":100,"mute":true}' >/dev/null
settled '[[ $(bus_db 0) -lt $SILENT_DB ]]' || fail "muted In-Ear still sounds ($(bus_db 0) dB)"
act '{"action":"set_bus","bus":"In-Ear","volume":100,"mute":false}' >/dev/null
act '{"action":"set_audio","input":"Ball","on":true}' >/dev/null
pass "buses: sends heard, a bus without sends silent, before/after the fader, master mute"

# --- headphones (AUD.4): their own mix; a bus listened to on them ---
sound_inputs="$(curl -sf "$CORE/state" | jq -r '.inputs[] | select(.has_audio) | .name')"
phones_sends() {
    local name
    while read -r name; do
        act "$(jq -cn --arg i "$name" --argjson on "$1" '{action:"set_phones_send",input:$i,on:$on,level:100}')" >/dev/null
    done <<<"$sound_inputs"
}
phones_sends false
settled '[[ $(mon_db) -lt $SILENT_DB ]]' || fail "headphones heard without a send ($(mon_db) dB)"
settled '[[ $(pgm_db) -gt $AUDIBLE_DB ]]' || fail "the headphones' sends took program down ($(pgm_db) dB)"
act '{"action":"set_bus_listen","bus":"Stream","on":true}' >/dev/null
settled '[[ $(mon_db) -gt $AUDIBLE_DB ]]' || fail "Stream listened to but not heard ($(mon_db) dB)"
act '{"action":"clear_listen"}' >/dev/null
settled '[[ $(mon_db) -lt $SILENT_DB ]]' || fail "headphones still on Stream after clearing ($(mon_db) dB)"
phones_sends true
settled '[[ $(mon_db) -gt $AUDIBLE_DB ]]' || fail "headphones not back with their sends ($(mon_db) dB)"
pass "headphones: own mix, silent without sends, a bus listened to"

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
is_orange() { picture inputs/Orange "$WORK/orange.jpg" || return 1; [[ $(rgb "$WORK/orange.jpg" r) -gt 200 && $(rgb "$WORK/orange.jpg" b) -lt 50 ]]; }
wait_for "$SETTLE_S" is_orange || fail "color input not orange"
is_logo() { picture inputs/Logo "$WORK/logo.jpg" || return 1; [[ $(rgb "$WORK/logo.jpg" b) -gt 150 && $(rgb "$WORK/logo.jpg" r) -lt 80 ]]; }
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
    picture inputs/Clips "$WORK/c1.jpg" || return 1
    sleep "$TAP_INTERVAL"
    picture inputs/Clips "$WORK/c2.jpg" || return 1
    cmp -s "$WORK/c1.jpg" "$WORK/c2.jpg"
}
picture inputs/Clips "$WORK/c0.jpg" || fail "no picture of Clips"   # switches the tap on before pausing
act '{"action":"transport_input","input":"Clips","transport":"pause"}' >/dev/null
wait_for "$SETTLE_S" clips_still || fail "paused playlist still moves"
# The state says where it stands (API.4).
clips_playback() { curl -sf "$CORE/state" | jq -r '.inputs[] | select(.name=="Clips") | .playback // empty'; }
settled '[[ "$(clips_playback | jq -r .state)" == paused ]]' || fail "state does not show the pause: $(clips_playback)"
[[ "$(clips_playback | jq '.items == 2 and .duration_ms > 0')" == true ]] || fail "playback in the state incomplete: $(clips_playback)"
act '{"action":"transport_input","input":"Clips","transport":"play"}' >/dev/null
clips_move() { ! clips_still; }
wait_for "$SETTLE_S" clips_move || fail "playlist does not play again"
# a is dark, b bright: next must flip the brightness either way.
picture inputs/Clips "$WORK/before.jpg" || fail "no picture of Clips"
act '{"action":"transport_input","input":"Clips","transport":"next"}' >/dev/null
switched() {
    picture inputs/Clips "$WORK/next.jpg" || return 1
    change=$(( $(brightness "$WORK/next.jpg") - $(brightness "$WORK/before.jpg") ))
    [[ ${change#-} -gt $MIN_BRIGHTNESS ]]
}
wait_for "$SETTLE_S" switched || fail "next did not switch clips (brightness change $change)"
settled '[[ "$(clips_playback | jq -r .state)" == playing ]]' || fail "state does not show playing: $(clips_playback)"
pass "transport: pause, play, next; playback in the state"

# --- paused clip: strom repeats its last frame (no signal loss) ---
act '{"action":"cut_direct","input":"Clips"}' >/dev/null
# Straight to program from outside preview: the mixer must follow lmix.
mixer_pgm="$(curl -sf "$STROM/api/flows/$MIXER_ID/blocks/vm/state" | jq '.program_input')"
[[ "$mixer_pgm" == 4 ]] || fail "cut_direct: mixer program is input $mixer_pgm, not Clips (4)"
# Pause in the bright clip b, well before its end: start b afresh with
# "next" from the dark clip a (from b, "next" goes to a first). Paused
# near its end, b would roll over to a before the pause takes hold.
dark_clip() { picture inputs/Clips "$WORK/now.jpg" || return 1; [[ $(brightness "$WORK/now.jpg") -lt $MIN_BRIGHTNESS ]]; }
if ! wait_for 3 dark_clip; then
    act '{"action":"transport_input","input":"Clips","transport":"next"}' >/dev/null
    wait_for "$SETTLE_S" dark_clip || fail "next did not go to the dark clip a"
fi
act '{"action":"transport_input","input":"Clips","transport":"next"}' >/dev/null
wait_for "$SETTLE_S" bright inputs/Clips "$WORK/now.jpg" || fail "bright clip b does not play"
act '{"action":"transport_input","input":"Clips","transport":"pause"}' >/dev/null
wait_for "$SETTLE_S" clips_still || fail "playlist did not pause"
# Held: bright, and still bright two seconds later. Program lags the
# mixer by seconds under load, so a late picture can show the b before
# the "next", then a: such a pair is a try, not the end of the check.
held_bright() { bright program "$WORK/hold.jpg" && sleep 2 && bright program "$WORK/hold.jpg"; }
wait_for "$SETTLE_S" held_bright || fail "paused clip not held on program"
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

# Quick play (SWI.8): the clip, paused off air, to program with its own
# effect (half a second of fade), playing.
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
act '{"action":"transport_input","input":"Clips","transport":"pause"}' >/dev/null
wait_for "$SETTLE_S" clips_still || fail "clip did not pause before quick play"
act '{"action":"quick_play","input":"Clips"}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq -r '.program')" == "$clips_id" ]] || fail "quick play: Clips not on program"
mixer_on_clips() { [[ "$(curl -sf "$STROM/api/flows/$MIXER_ID/blocks/vm/state" | jq '.program_input')" == 4 ]]; }
wait_for "$SETTLE_S" mixer_on_clips || fail "quick play: mixer program is not Clips (4)"
wait_for "$SETTLE_S" clips_move || fail "quick play: clip does not play"
pass "quick play: paused clip off air to program, playing"

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
# All streams at once (STR.1): the stopped one starts, then both stop.
act '{"action":"add_output","spec":{"kind":"srt","uri":"srt://:9912?mode=listener"},"name":"Stream 2"}' >/dev/null
act '{"action":"start_outputs","group":"streams"}' >/dev/null
settled '[[ "$(output_state Stream)" == running && "$(output_state "Stream 2")" == running ]]' || fail "streams not all running"
[[ "$(output_state Rec)" == stopped ]] || fail "start of all streams started the recording"
act '{"action":"stop_outputs","group":"streams"}' >/dev/null
settled '[[ "$(output_state Stream)" == stopped && "$(output_state "Stream 2")" == stopped ]]' || fail "streams not all stopped"
act '{"action":"remove_output","output":"Stream 2"}' >/dev/null
pass "outputs: all streams started and stopped together"
# RTMP (STR.2): ffmpeg takes the stream as a platform would.
timeout 30 ffmpeg -loglevel error -listen 1 -i rtmp://127.0.0.1:19350/live/lmixkey -t 4 -c copy "$WORK/rtmp.flv" &
rtmp_receiver=$!
sleep 1
act '{"action":"add_output","spec":{"kind":"rtmp","url":"rtmp://127.0.0.1:19350/live","key":"lmixkey","video_kbps":2000},"name":"RTMP"}' >/dev/null
act '{"action":"start_output","output":"RTMP"}' >/dev/null
wait "$rtmp_receiver" || true
[[ $(stat -c %s "$WORK/rtmp.flv" 2>/dev/null || echo 0) -gt 50000 ]] || fail "RTMP output delivered nothing"
[[ "$(codecs "$WORK/rtmp.flv")" == "aac,h264" ]] || fail "RTMP stream lacks H.264 or AAC: $(codecs "$WORK/rtmp.flv")"
act '{"action":"stop_output","output":"RTMP"}' >/dev/null
act '{"action":"remove_output","output":"RTMP"}' >/dev/null
pass "outputs: RTMP stream received with H.264 and AAC"
# Encoding (STR.3): H.265 at a constant bitrate lands as HEVC.
act '{"action":"add_output","spec":{"kind":"record","folder":"hevc","container":"mkv","encoding":{"codec":"h265","rate":"cbr","keyframe_ms":1000}},"name":"HEVC"}' >/dev/null
act '{"action":"start_output","output":"HEVC"}' >/dev/null
settled '[[ "$(output_state HEVC)" == running ]]' || fail "HEVC recording not running"
sleep 3
act '{"action":"stop_output","output":"HEVC"}' >/dev/null
wait_for "$SETTLE_S" is output_state HEVC stopped || fail "HEVC recording did not stop"
hevc="$(find "$WORK/strom-data" -path '*hevc/*' -name '*.mkv' | head -1)"
[[ -n "$hevc" ]] || fail "no HEVC recording file"
[[ "$(codecs "$hevc")" == "aac,hevc" ]] || fail "recording is not HEVC: $(codecs "$hevc")"
act '{"action":"remove_output","output":"HEVC"}' >/dev/null
pass "outputs: H.265 recording at a constant bitrate"

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
# The multiview as an output (OUT.2), with program sound.
act '{"action":"add_output","spec":{"kind":"record","folder":"mv","container":"mp4"},"source":{"from":"multiview"},"name":"MV"}' >/dev/null
act '{"action":"start_output","output":"MV"}' >/dev/null
sleep 4
act '{"action":"stop_output","output":"MV"}' >/dev/null
wait_for "$SETTLE_S" is output_state MV stopped || fail "multiview recording did not stop"
mv_file="$(find "$WORK/strom-data" -path '*mv/*' -name '*.mp4' | head -1)"
[[ "$(codecs "$mv_file")" == "aac,h264" ]] || fail "multiview recording: $(codecs "$mv_file")"
act '{"action":"remove_output","output":"MV"}' >/dev/null
# The preview as an output (OUT.2), with program sound: Orange on preview,
# Ball on program, so the recorded picture is orange.
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
act '{"action":"preview_input","input":"Orange"}' >/dev/null
act '{"action":"add_output","spec":{"kind":"record","folder":"pvw","container":"mp4"},"source":{"from":"preview"},"name":"PVW"}' >/dev/null
act '{"action":"start_output","output":"PVW"}' >/dev/null
sleep 4
act '{"action":"stop_output","output":"PVW"}' >/dev/null
wait_for "$SETTLE_S" is output_state PVW stopped || fail "preview recording did not stop"
pvw_file="$(find "$WORK/strom-data" -path '*pvw/*' -name '*.mp4' | head -1)"
[[ "$(codecs "$pvw_file")" == "aac,h264" ]] || fail "preview recording: $(codecs "$pvw_file")"
ffmpeg -loglevel error -y -ss 2 -i "$pvw_file" -frames:v 1 "$WORK/pvw.png"
[[ $(rgb "$WORK/pvw.png" r) -gt 200 && $(rgb "$WORK/pvw.png" b) -lt 60 ]] \
    || fail "preview recording does not show the preview (r $(rgb "$WORK/pvw.png" r), b $(rgb "$WORK/pvw.png" b))"
act '{"action":"remove_output","output":"PVW"}' >/dev/null
pass "MultiCorder: ISO recordings per input (silent input video only, playlist with sound); multiview and preview recorded"

# Display output: program in a window on the engine's screen. Orange on
# program makes the window easy to recognize.
act '{"action":"cut_direct","input":"Orange"}' >/dev/null
act '{"action":"add_output","spec":{"kind":"display","mode":{"mode":"window"}},"name":"Screen"}' >/dev/null
act '{"action":"start_output","output":"Screen"}' >/dev/null
screen_orange() {
    DISPLAY="$ENGINE_DISPLAY" import -window root "$WORK/screen.png"
    [[ $(rgb "$WORK/screen.png" r) -gt 200 && $(rgb "$WORK/screen.png" b) -lt 60 ]]
}
if [[ "$MIXER" == gl ]]; then
    # It opens on the desktop, not on Xvfb: only that it runs.
    [[ "$(output_state Screen)" == running ]] || fail "display output not running"
    act '{"action":"stop_output","output":"Screen"}' >/dev/null
    pass "display output: running (picture not checked on the GL mixer)"
else
    wait_for "$SETTLE_S" screen_orange || true
    [[ "$(output_state Screen)" == running ]] && screen_orange || fail "display output does not show program"
    act '{"action":"stop_output","output":"Screen"}' >/dev/null
    pass "display output: program in a window on the engine screen"
fi

# --- failover (3.4): a source dies mid-transition, a stream breaks ---
input_state() { curl -sf "$CORE/state" | jq -r ".inputs[] | select(.name==\"$1\") | .status"; }
flow_id() { curl -sf "$STROM/api/flows" | jq -r ".flows[] | select(.name==\"$1\") | .id"; }
# The Logo dies for good: with its file gone, the fail back (every 2-10 s)
# cannot restart it until the file is back. A loss of only 2 s raced the
# program picture, which lags the mixer by seconds on a busy runner.
logo_slot="lmix slot $(curl -sf "$CORE/state" | jq '.inputs[] | select(.name=="Logo") | .slot')"
kill_logo() {
    mv "$MEDIA/logo.png" "$MEDIA/logo.gone"
    curl -sf -X POST "$STROM/api/flows/$(flow_id "$logo_slot")/stop" >/dev/null
    wait_for "$SETTLE_S" is input_state Logo offline || fail "dead source not marked offline"
}
revive_logo() {
    mv "$MEDIA/logo.gone" "$MEDIA/logo.png"
    wait_for "$((SETTLE_S + 15))" is input_state Logo online || fail "source did not fail back"
}
logo_pgm() { picture program "$WORK/logo-pgm.jpg" || return 1; [[ $(rgb "$WORK/logo-pgm.jpg" b) -gt 150 && $(rgb "$WORK/logo-pgm.jpg" r) -lt 80 ]]; }
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
act '{"action":"preview_input","input":"Logo"}' >/dev/null
act '{"action":"set_transition","slot":1,"effect":"fade","duration_ms":3000}' >/dev/null
act '{"action":"transition","slot":1}' >/dev/null
sleep 2   # two thirds into the fade
kill_logo
# Hold: program fades on to the dead source's last frame (CPU compositor
# in the VM may show one black frame at the loss, HARDWARE-TODO).
wait_for "$SETTLE_S" logo_pgm || fail "program lost the logo while the source was gone"
[[ "$(input_state Logo)" == offline ]] || fail "dead source back before its file"
running="$(curl -sf "$STROM/api/flows/$MIXER_ID" | jq -r '.flow.running')"
[[ "$running" == true ]] || fail "mixer stopped after source loss"
revive_logo
wait_for "$SETTLE_S" is_logo || fail "logo not back after failback"
pass "failover: source died mid-transition, program held its last frame, input failed back"

# Signal loss per input: hold the last frame (default, above) or black.
# A paused clip keeps sending its frame, so the source has to die.
act '{"action":"set_signal_loss","input":"Logo","mode":"black"}' >/dev/null
act '{"action":"cut_direct","input":"Logo"}' >/dev/null
wait_for "$SETTLE_S" logo_pgm || fail "logo not on program before the loss"
kill_logo
black_pgm() { picture program "$WORK/black.jpg" || return 1; [[ $(brightness "$WORK/black.jpg") -lt 5 ]]; }
wait_for "$SETTLE_S" black_pgm || fail "dead source not black on program ($(brightness "$WORK/black.jpg"))"
act '{"action":"set_signal_loss","input":"Logo","mode":"hold"}' >/dev/null
revive_logo
# Black ends when the source is back.
wait_for "$SETTLE_S" logo_pgm || fail "program black after the source came back"
pass "signal loss: black when set, the source died; back on program with it"

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
overlay_shown() { picture program "$WORK/ovl.jpg" || return 1; [[ $(rgb "$WORK/ovl.jpg" r) -gt 200 ]]; }
overlay_gone() { picture program "$WORK/ovl.jpg" || return 1; [[ $(rgb "$WORK/ovl.jpg" r) -lt 60 ]]; }
# Wait for Ball: a late picture of an orange program would pass.
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
# Eight channels (OVL.1): the top one shows above program as well.
act '{"action":"overlay_in","channel":8,"input":"Orange","duration_ms":0}' >/dev/null
wait_for "$SETTLE_S" overlay_shown || fail "overlay 8 not on program"
act '{"action":"overlay_out","channel":8,"duration_ms":0}' >/dev/null
wait_for "$SETTLE_S" overlay_gone || fail "overlay 8 not taken off"
pass "overlays: channel 8 on and off"
# A still with alpha (SRC.4): a PNG, red on the left half and clear on the
# right, as overlay over Orange lets Orange through on the right.
convert -size 1280x720 xc:none -fill '#ff0000' -draw 'rectangle 0,0 639,719' "$MEDIA/badge.png"
act "{\"action\":\"add_input\",\"source\":{\"kind\":\"image\",\"path\":\"$MEDIA/badge.png\"},\"name\":\"Badge\"}" >/dev/null
act '{"action":"cut_direct","input":"Orange"}' >/dev/null
# Orange first: a late picture of Ball under the badge reads as lost alpha.
wait_for "$SETTLE_S" overlay_shown || fail "orange not on program before the badge"
act '{"action":"overlay_in","channel":1,"input":"Badge","duration_ms":0}' >/dev/null
# Mean of colour channel $3 (r, g) in the left or right half, middle band.
half() {
    local w h x; read -r w h <<<"$(identify -format '%w %h' "$1")"
    [[ "$2" == left ]] && x=$((w / 8)) || x=$((w * 5 / 8))
    convert "$1" -crop "$((w / 4))x$((h / 4))+$x+$((h * 3 / 8))" +repage -format "%[fx:int(mean.$3*255)]" info:
}
badge_over_orange() {
    picture program "$WORK/badge.jpg" || return 1
    [[ $(half "$WORK/badge.jpg" left r) -gt 200 && $(half "$WORK/badge.jpg" left g) -lt 60 \
        && $(half "$WORK/badge.jpg" right r) -gt 200 && $(half "$WORK/badge.jpg" right g) -gt 90 ]]
}
wait_for "$SETTLE_S" badge_over_orange \
    || fail "PNG alpha lost: right half $(half "$WORK/badge.jpg" right r)/$(half "$WORK/badge.jpg" right g) (r/g), want orange; left $(half "$WORK/badge.jpg" left r)/$(half "$WORK/badge.jpg" left g), want red"
act '{"action":"overlay_out_all","duration_ms":0}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq '.overlays[0]')" == null ]] || fail "badge overlay not off"
# Its slot is needed later on.
act '{"action":"remove_input","input":"Badge"}' >/dev/null
pass "still with alpha: a PNG overlay lets program through where it is clear"
# Channel settings (OVL.2) and an overlay prepared on preview (OVL.3):
# Orange waits on channel 2 with preview tally, goes on with the cut in
# the top right quarter (the place of its DSK pad) and off by itself.
act '{"action":"set_overlay","channel":2,"effect":"cut","duration_ms":0,"auto_off_ms":6000,"rect":{"x":0.5,"y":0,"w":0.5,"h":0.5}}' >/dev/null
act '{"action":"cut_direct","input":"Bars"}' >/dev/null
act '{"action":"preview_input","input":"Ball"}' >/dev/null
act '{"action":"overlay_preview","channel":2,"input":"Orange"}' >/dev/null
[[ "$(curl -sf "$CORE/tally" | jq ".[$((orange_number - 1))]")" == 2 ]] || fail "prepared overlay has no preview tally"
act '{"action":"cut"}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq -r '.overlay_preview[1] == null and .overlays[1] != null')" == true ]] \
    || fail "prepared overlay did not go on with the cut"
# Mean red of a region (WxH+X+Y) of a picture.
quarter() { convert "$1" -crop "$2" +repage -format '%[fx:int(mean.r*255)]' info:; }
top_right() { local w h; read -r w h <<<"$(identify -format '%w %h' "$1")"; quarter "$1" "$((w / 4))x$((h / 4))+$((w * 5 / 8))+$((h / 8))"; }
pip_shown() {
    picture program "$WORK/pip.jpg" || return 1
    local w h; read -r w h <<<"$(identify -format '%w %h' "$WORK/pip.jpg")"
    # Top right: Orange; bottom left: the black around Ball.
    [[ $(top_right "$WORK/pip.jpg") -gt 200 ]] \
        && [[ $(quarter "$WORK/pip.jpg" "$((w / 4))x$((h / 4))+0+$((h * 3 / 4))") -lt 60 ]]
}
wait_for "$SETTLE_S" pip_shown || fail "prepared overlay not on program in the top right quarter"
pass "overlays: prepared on preview with preview tally, on with the take, in a box"
pip_gone() { picture program "$WORK/pip.jpg" || return 1; [[ $(top_right "$WORK/pip.jpg") -lt 120 ]]; }
wait_for "$SETTLE_S" pip_gone || fail "overlay 2 did not go off by itself"
[[ "$(curl -sf "$CORE/state" | jq -r '.overlays[1]')" == null ]] || fail "auto off not in state"
act '{"action":"set_overlay","channel":2,"effect":"fade","duration_ms":500}' >/dev/null
pass "overlays: off by itself after its time"
code="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$CORE/actions" -H 'content-type: application/json' \
    -d '{"action":"set_channel_strip","input":"Orange","strip":{}}')"
[[ "$code" != 200 ]] || fail "channel strip accepted for an input without sound"

# --- T-bar (4.3): preview mixed in by hand, taken at the end ---
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
act '{"action":"preview_input","input":"Orange"}' >/dev/null
# Sound that follows video follows the T-bar (SWI.5): Ball fades out by
# half (about -6 dB) and comes back when the bar goes back. Heard on the
# Stream bus, after the fader, where only Ball sends (program mixes other
# inputs).
act '{"action":"set_send","input":"Ball","bus":"Stream","level":100}' >/dev/null
act '{"action":"set_audio_follow","input":"Ball","follow":true}' >/dev/null
full=$(steady_db bus_db 1)
act '{"action":"t_bar","position":0.5}' >/dev/null
settled '[[ $(bus_db 1) -le $((full - 4)) && $(bus_db 1) -gt $SILENT_DB ]]' || fail "Ball's sound did not follow the T-bar ($full -> $(bus_db 1) dB)"
act '{"action":"t_bar","position":0}' >/dev/null
settled '[[ $(bus_db 1) -ge $((full - 2)) ]]' || fail "Ball's sound not back with the T-bar ($full -> $(bus_db 1) dB)"
act '{"action":"set_audio_follow","input":"Ball","follow":false}' >/dev/null
act '{"action":"t_bar","position":0.5}' >/dev/null
half_mixed() { picture program "$WORK/tbar.jpg" || return 1; local r; r=$(rgb "$WORK/tbar.jpg" r); ((r > 80 && r < 200)); }
wait_for "$SETTLE_S" half_mixed || fail "T-bar at half: program red $(rgb "$WORK/tbar.jpg" r), not a mix"
[[ "$(curl -sf "$CORE/state" | jq '.t_bar')" == 0.5 ]] || fail "T-bar position not in state"
act '{"action":"t_bar","position":1}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq -r '[.inputs[] | select(.name=="Orange") | .id][0] == .program')" == true ]] \
    || fail "T-bar at the end did not take preview to program"
wait_for "$SETTLE_S" overlay_shown || fail "program not orange after the T-bar take"
pass "T-bar: preview mixed in by half, sound follows, taken at the end"

# --- stinger (4.2, STG): strom plays a clip with alpha over program and
# cuts underneath at its cut point ---
# Blue square in the middle, the rest transparent; 2 s.
ffmpeg -loglevel error -y -f lavfi -i "color=c=black@0.0:s=1280x720:r=30:d=2,format=yuva420p[bg];color=c=blue:s=400x400:r=30:d=2,format=yuva420p[box];[bg][box]overlay=440:160:format=yuv420" \
    -c:v libvpx-vp9 -pix_fmt yuva420p -auto-alt-ref 0 "$MEDIA/sting.webm"
act "{\"action\":\"add_input\",\"source\":{\"kind\":\"file\",\"path\":\"$MEDIA/sting.webm\"},\"name\":\"Sting\"}" >/dev/null
act '{"action":"set_transition","slot":4,"effect":"stinger","duration_ms":3000,"stinger":{"input":"Sting","cut_ms":1500}}' >/dev/null
# The button's clip is in strom's stinger library, with its cut point.
STINGER_API="$STROM/api/flows/$MIXER_ID/blocks/vm/stinger"
in_library() {
    curl -sf "$STINGER_API" | jq -e --arg f "$MEDIA/sting.webm" \
        '[.clips[] | select(.file == $f and .settings.cut_point_ms == 1500)] | length == 1' >/dev/null
}
wait_for "$SETTLE_S" in_library || fail "stinger clip not in strom's library: $(curl -sf "$STINGER_API" | jq -c '.clips')"
act '{"action":"preview_input","input":"Ball"}' >/dev/null   # program is Orange
sleep 2
sting_at=$(date +%s)
act '{"action":"transition","slot":4}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq -r '[.inputs[] | select(.name=="Ball") | .id][0] == .program')" == true ]] \
    || fail "stinger did not take preview to program"
code="$(curl -s -o /dev/null -w '%{http_code}' -X POST "$CORE/actions" -H 'content-type: application/json' -d '{"action":"cut"}')"
[[ "$code" != 200 ]] || fail "cut accepted while the stinger plays"
middle() { convert "$1" -gravity center -crop 10x10+0+0 +repage -format "%[fx:int(mean.b*255)] %[fx:int(mean.r*255)]" info:; }
corner_red() { convert "$1" -crop 10x10+0+0 +repage -format "%[fx:int(mean.r*255)]" info:; }
stinger_over_orange() {
    picture program "$WORK/sting.jpg" || return 1
    local b r; read -r b r <<<"$(middle "$WORK/sting.jpg")"
    ((b > 180 && r < 80)) && [[ $(corner_red "$WORK/sting.jpg") -gt 200 ]]
}
wait_for "$SETTLE_S" stinger_over_orange || fail "stinger clip not over program (alpha around it)"
stinger_gone() {
    picture program "$WORK/sting.jpg" || return 1
    local b r; read -r b r <<<"$(middle "$WORK/sting.jpg")"
    ((b < 120)) && [[ $(rgb "$WORK/sting.jpg" r) -lt 60 ]]
}
wait_for "$SETTLE_S" stinger_gone || fail "stinger did not end on the new program"
# strom ran the take, not lmix-core: its report names the clip.
took() { curl -sf "$STINGER_API" | jq -e --arg f "$MEDIA/sting.webm" '.last_take.file == $f' >/dev/null; }
wait_for "$SETTLE_S" took || fail "strom reports no stinger take: $(curl -sf "$STINGER_API" | jq -c '.last_take')"
# strom's end of the take frees lmix's lock, long before its limit
# (clip + 6 s): a cut right after goes out.
cut_ok() { [[ "$(curl -s -o /dev/null -w '%{http_code}' -X POST "$CORE/actions" -H 'content-type: application/json' -d '{"action":"cut"}')" == 200 ]]; }
wait_for 2 cut_ok || fail "cut refused after strom finished the stinger"
(($(date +%s) - sting_at < 6)) || fail "cut only accepted $(($(date +%s) - sting_at)) s after the stinger (lock limit, not strom's end)"
wait_for "$SETTLE_S" overlay_shown || fail "cut after the stinger not on program (orange)"
act '{"action":"remove_input","input":"Sting"}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq -r '.transitions[3].effect')" == cut ]] || fail "stinger button kept a removed clip"
library_empty() { [[ "$(curl -sf "$STINGER_API" | jq '.clips | length')" == 0 ]]; }
wait_for "$SETTLE_S" library_empty || fail "removed clip still in strom's stinger library"
pass "stinger: strom plays the clip with alpha over program, cuts underneath, locked until strom ends it"

# --- looks (4.6): only the GL mixer has them; the others say so ---
code="$(curl -s -o "$WORK/look.json" -w '%{http_code}' -X POST "$CORE/actions" -H 'content-type: application/json' \
    -d '{"action":"set_look","input":"Ball","look":{"brightness":0.3}}')"
if [[ "$MIXER" == gl ]]; then
    [[ "$(curl -sf "$CORE/state" | jq '.looks_available')" == true ]] || fail "GL mixer offers no looks"
    [[ "$code" == 200 ]] || fail "look on the GL mixer: $code $(cat "$WORK/look.json")"
    act '{"action":"set_look","input":"Ball","look":{"brightness":0.0}}' >/dev/null
    pass "looks: accepted on the GL mixer (picture check: scripts/test-looks-gl.sh)"
else
    [[ "$(curl -sf "$CORE/state" | jq '.looks_available')" == false ]] || fail "$MIXER mixer claims looks"
    [[ "$code" != 200 ]] && grep -q "GL mixer" "$WORK/look.json" || fail "look on the $MIXER mixer: $code $(cat "$WORK/look.json")"
    pass "looks: refused with a reason on the $MIXER mixer (GL check: scripts/test-looks-gl.sh)"
fi

# --- layouts (4.6): Orange behind, Ball in the top right quarter ---
act '{"action":"add_layout","name":"Interview","background":"Orange","boxes":[{"rect":{"x":0.5,"y":0.0,"w":0.5,"h":0.5},"input":"Ball","border_px":4}]}' >/dev/null
act '{"action":"cut_direct","input":"Bars"}' >/dev/null
act '{"action":"preview_layout","layout":"Interview"}' >/dev/null
act '{"action":"cut"}' >/dev/null
layout_id="$(curl -sf "$CORE/state" | jq -r '.layouts[0].id')"
[[ "$(curl -sf "$CORE/state" | jq -r '.program')" == "$layout_id" ]] || fail "layout not on program"
on_air() { curl -sf "$CORE/tally" | jq ".[$(($(curl -sf "$CORE/state" | jq ".inputs[] | select(.name==\"$1\") | .number") - 1))]"; }
[[ "$(on_air Orange)" == 1 && "$(on_air Ball)" == 1 ]] || fail "inputs of the layout on program have no red tally"
layout_shown() {
    picture program "$WORK/layout.jpg" || return 1
    local w h; read -r w h <<<"$(identify -format '%w %h' "$WORK/layout.jpg")"
    # Bottom left: the orange background; top right: the ball (dark).
    [[ $(quarter "$WORK/layout.jpg" "$((w / 4))x$((h / 4))+0+$((h * 3 / 4))") -gt 200 ]] \
        && [[ $(quarter "$WORK/layout.jpg" "$((w / 4))x$((h / 4))+$((w * 5 / 8))+$((h / 8))") -lt 100 ]]
}
wait_for "$SETTLE_S" layout_shown || fail "layout picture: background and box not where they belong"
# A hidden box (LAY.2): Orange shows through, Ball is off air; back.
act '{"action":"show_box","layout":"Interview","number":1,"shown":"hidden"}' >/dev/null
box_hidden() {
    picture program "$WORK/layout.jpg" || return 1
    local w h; read -r w h <<<"$(identify -format '%w %h' "$WORK/layout.jpg")"
    [[ $(quarter "$WORK/layout.jpg" "$((w / 4))x$((h / 4))+$((w * 5 / 8))+$((h / 8))") -gt 200 ]]
}
wait_for "$SETTLE_S" box_hidden || fail "hidden box still in the picture"
[[ "$(on_air Ball)" == 0 ]] || fail "input of a hidden box still has red tally"
act '{"action":"show_box","layout":"Interview","number":1,"shown":"toggle"}' >/dev/null
wait_for "$SETTLE_S" layout_shown || fail "box not back after showing it again"
# Crop: the right half of Orange fills a full-picture box at its own
# aspect (strom fits it), so the left edge turns black.
act '{"action":"set_layout","layout":1,"background":null,"boxes":[{"rect":{"x":0,"y":0,"w":1,"h":1},"input":"Orange","crop":{"left":0.5}}]}' >/dev/null
cropped() {
    picture program "$WORK/layout.jpg" || return 1
    local w h; read -r w h <<<"$(identify -format '%w %h' "$WORK/layout.jpg")"
    [[ $(quarter "$WORK/layout.jpg" "$((w / 16))x$((h / 4))+0+$((h * 3 / 8))") -lt 60 ]] \
        && [[ $(quarter "$WORK/layout.jpg" "$((w / 8))x$((h / 4))+$((w * 7 / 16))+$((h * 3 / 8))") -gt 200 ]]
}
wait_for "$SETTLE_S" cropped || fail "layout crop not applied"
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
act '{"action":"remove_layout","layout":1}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq '.layouts | length')" == 0 ]] || fail "layout not removed"
pass "layouts: background and box on program, red tally for both inputs, box hidden and back, crop, removed"

# --- automation (AUT.7, AUT.14): a cue list whose cues fire triggers ---
act '{"action":"set_triggers","input":"Orange","triggers":[{"on":"program","delay_ms":500,"actions":[{"action":"overlay_in","channel":2,"input":"Ball","duration_ms":0}]},{"on":"program_off","actions":[{"action":"overlay_out","channel":2,"duration_ms":0}]}]}' >/dev/null
act '{"action":"add_cue_list","name":"Run","cues":[{"name":"Orange","actions":[{"action":"cut_direct","input":"Orange"}]},{"name":"Ball","actions":[{"action":"cut_direct","input":"Ball"}],"follow":"after_wait","post_wait_ms":1500},{"number":"3","name":"Orange again","actions":[{"action":"cut_direct","input":"Orange"}]}]}' >/dev/null
state_is() { [[ "$(curl -sf "$CORE/state" | jq -r "$1")" == true ]]; }
id_of() { curl -sf "$CORE/state" | jq -r ".inputs[] | select(.name==\"$1\") | .id"; }
orange_id="$(id_of Orange)"; ball_id="$(id_of Ball)"
act '{"action":"cue_go","list":"Run"}' >/dev/null
state_is ".program == \"$orange_id\" and .cue_lists[0].standby == 2" || fail "cue 1 did not take Orange"
wait_for 4 state_is ".overlays[1] == \"$ball_id\"" || fail "trigger of Orange did not put Ball on overlay 2"
# GO without a list: the active one. Ball, then Orange again 1.5 s later
# by itself (follow after the post wait).
act '{"action":"cue_go"}' >/dev/null
state_is ".program == \"$ball_id\" and .cue_lists[0].standby == 4" || fail "cue 2 did not take Ball or standby did not pass the chain"
state_is "(.running_cues | length) == 1" || fail "cue 2 not running in its post wait"
wait_for 4 state_is ".overlays[1] == null" || fail "leaving program did not take overlay 2 off"
wait_for 4 state_is ".program == \"$orange_id\"" || fail "the follow did not start cue 3"
wait_for 4 state_is ".overlays[1] == \"$ball_id\"" || fail "trigger did not fire again for cue 3"
act '{"action":"cue_standby","list":"Run","to":"previous_sequence"}' >/dev/null
state_is ".cue_lists[0].standby == 2 and .program == \"$orange_id\"" || fail "standby back did not go to the chain start, or ran something"
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
act '{"action":"remove_cue_list","list":"Run"}' >/dev/null
act '{"action":"set_triggers","input":"Orange","triggers":[]}' >/dev/null
wait_for 4 state_is ".overlays[1] == null and (.cue_lists | length) == 0" || fail "automation not cleaned up"
pass "automation: cues GO, follow after a wait, standby back; triggers on and off program"
# Clip end (AUT.7): Clips (two 10 s clips, looped) puts Orange on overlay 3
# two seconds before its end and takes it off at the end.
act '{"action":"set_triggers","input":"Clips","triggers":[{"on":"clip_end","before_ms":2000,"actions":[{"action":"overlay_in","channel":3,"input":"Orange","duration_ms":0}]},{"on":"clip_end","actions":[{"action":"overlay_out","channel":3,"duration_ms":0}]}]}' >/dev/null
wait_for "$((SETTLE_S + 20))" state_is ".overlays[2] == \"$orange_id\"" || fail "countdown before the clip end did not fire"
wait_for "$SETTLE_S" state_is ".overlays[2] == null" || fail "clip end did not fire"
act '{"action":"set_triggers","input":"Clips","triggers":[]}' >/dev/null
pass "clip end: countdown and end of a playlist fire their triggers"
# Hold at the end (INP.10): the playlist stops on its last frame, the
# end still fires, switching back to the loop does not restart it.
act '{"action":"set_at_end","input":"Clips","at_end":"hold"}' >/dev/null
act '{"action":"set_triggers","input":"Clips","triggers":[{"on":"clip_end","actions":[{"action":"overlay_in","channel":3,"input":"Orange","duration_ms":0}]}]}' >/dev/null
wait_for "$((SETTLE_S + 25))" state_is ".overlays[2] == \"$orange_id\"" || fail "held end did not fire clip_end"
settled '[[ "$(clips_playback | jq -r .state)" == stopped ]]' || fail "held playlist not stopped: $(clips_playback)"
wait_for "$SETTLE_S" clips_still || fail "held playlist still moves"
act '{"action":"set_at_end","input":"Clips","at_end":"loop"}' >/dev/null
sleep 1
clips_still || fail "switching back to the loop restarted the held clip"
act '{"action":"transport_input","input":"Clips","transport":"restart"}' >/dev/null
wait_for "$SETTLE_S" clips_move || fail "held playlist does not play again"
act '{"action":"set_triggers","input":"Clips","triggers":[]}' >/dev/null
act '{"action":"overlay_out","channel":3,"duration_ms":0}' >/dev/null
pass "clip end: hold on the last frame, fires, loops again"

# --- latency (NFA.3): capture-to-program, CPU mixer in a VM ---
# Budget here is generous (software compositing, no GPU); the real target
# of two frames is checked on hardware (HARDWARE-TODO).
LATENCY_BUDGET_MS="${LATENCY_BUDGET_MS:-150}"
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
# About 20 samples: p95 then tolerates one hiccup of the shared VM (with 10
# it was the slowest sample).
curl -sf -X POST "$CORE/latency/measure?seconds=10" >/dev/null || fail "latency measurement not started"
sleep 11
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

# --- lmix as lmix-core client, on a virtual display ---
if [[ -n "${CONSOLE_SHOT:-}" ]]; then
    # lmix (needs Qt 6 with Kirigami and gst-plugin-qml6, BUILD.md),
    # with its own settings, not the operator's.
    (cd "$ROOT/workspace/console-qt" && cargo build --release)
    CONSOLE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/console-qt/target}/release/lmix"
    Xvfb :93 -screen 0 1440x900x24 >/dev/null 2>&1 &
    PIDS+=($!)
    sleep 1
    DISPLAY=:93 WAYLAND_DISPLAY= QT_QPA_PLATFORM=xcb QT_FORCE_STDERR_LOGGING=1 XDG_CONFIG_HOME="$WORK/config" \
        LMIX_CORE="${CORE%/api}" "$CONSOLE_BIN" >"$WORK/console.log" 2>&1 &
    PIDS+=($!)
    sleep "${CONSOLE_WAIT:-25}"   # live flows start one after another
    act '{"action":"set_listen","input":"Bars","listen":"pfl"}' >/dev/null   # show a solo
    act '{"action":"start_output","output":"Stream"}' >/dev/null           # and a running output
    sleep 2
    DISPLAY=:93 import -window root "$CONSOLE_SHOT"
    act '{"action":"clear_listen"}' >/dev/null
    pass "mixer screenshot: $CONSOLE_SHOT"
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
mixer_starts() { grep -c "Starting pipeline: lmix mixer" "$WORK/strom.log" || true; }
mixer_before="$(mixer_starts)"

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
# The mixer too: a new one stops program for a moment (strom gives its numbers
# back signed, which once made its buses unknown).
[[ "$(mixer_starts)" == "$mixer_before" ]] || fail "mixer built anew instead of adopted"
pass "lmix-core killed and restarted: mixer and output adopted, show resumed"

# --- program format (OUT.1): upright 1080x1920, then back ---
before="$(mixer_starts)"
act '{"action":"cut_direct","input":"Ball"}' >/dev/null
act '{"action":"set_master_format","format":{"width":1080,"height":1920,"framerate":"25/1"}}' >/dev/null
# The API's program picture is a 320x180 thumbnail whatever the format:
# the mixer's own properties tell the format, the picture that it runs.
vm_prop() { curl -sf "$STROM/api/flows/$MIXER_ID" | jq -r ".flow.blocks[] | select(.id==\"vm\") | .properties.$1"; }
wait_for "$((SETTLE_S + 10))" is vm_prop pgm_resolution 1080x1920 || fail "mixer not built in 1080x1920"
[[ "$(vm_prop pgm_framerate)" == 25/1 ]] || fail "mixer not at 25 fps"
[[ "$(mixer_starts)" -gt "$before" ]] || fail "mixer not built anew"
wait_for "$((SETTLE_S + 10))" program_moves || fail "program does not move in 1080x1920"
act '{"action":"set_master_format","format":{"width":1920,"height":1080,"framerate":"30/1"}}' >/dev/null
wait_for "$((SETTLE_S + 10))" is vm_prop pgm_resolution 1920x1080 || fail "mixer not back in 1920x1080"
wait_for "$((SETTLE_S + 10))" program_moves || fail "program does not move after going back"
wait_for "$SETTLE_S" is input_state Ball online || fail "Ball not online after the format changes"
pass "program format: upright 1080x1920 and back, mixer rebuilt, sources kept"

# --- multiview layout (OUT.10): program on the left, no meters, back ---
act '{"action":"set_multiview","layout":{"program":"left","meters":"hidden"}}' >/dev/null
wait_for "$((SETTLE_S + 10))" is vm_prop swap_pvw_pgm true || fail "multiview not built with program on the left"
[[ "$(vm_prop show_vu_meters)" == false ]] || fail "multiview still draws meters"
wait_for "$((SETTLE_S + 10))" program_moves || fail "program does not move after the multiview change"
act '{"action":"set_multiview","layout":{}}' >/dev/null
wait_for "$((SETTLE_S + 10))" is vm_prop swap_pvw_pgm false || fail "multiview layout not back"
wait_for "$SETTLE_S" is input_state Ball online || fail "Ball not online after the multiview changes"
pass "multiview layout: program on the left without meters and back, sources kept"

# --- sample rate (AUD.17): 44.1 kHz, program still sounds, back ---
mix_prop() { curl -sf "$STROM/api/flows/$MIXER_ID" | jq -r ".flow.blocks[] | select(.id==\"mix\") | .properties.$1"; }
act '{"action":"set_sample_rate","hz":44100}' >/dev/null
wait_for "$((SETTLE_S + 10))" is mix_prop sample_rate 44100 || fail "audio mixer not built at 44.1 kHz"
settled '[[ $(pgm_db) -gt $AUDIBLE_DB ]]' || fail "program silent at 44.1 kHz ($(pgm_db) dB)"
act '{"action":"set_sample_rate","hz":48000}' >/dev/null
wait_for "$((SETTLE_S + 10))" is mix_prop sample_rate 48000 || fail "audio mixer not back at 48 kHz"
settled '[[ $(pgm_db) -gt $AUDIBLE_DB ]]' || fail "program silent back at 48 kHz ($(pgm_db) dB)"
pass "sample rate: 44.1 kHz with sound and back"

criticals="$(grep -c 'GStreamer-CRITICAL' "$WORK/strom.log" || true)"
[[ "$criticals" == 0 ]] || fail "$criticals GStreamer CRITICALs in strom.log"
pass "no GStreamer CRITICALs"

echo "M2b VM check passed"
