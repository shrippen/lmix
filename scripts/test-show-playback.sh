#!/usr/bin/env bash
# Sound and picture cues on strom (show mode SND, VIS; S-E.3 B, S-E.13 B):
# a zone on this machine's default output and a stage in a window; a
# sound cue that ends with its file and lets the next cue follow, a stop
# with fade, fades on a zone and a stage, a picture cue, a panic. Runs
# strom's Show Playback block for real. The VM has no sound card (the
# default output is whatever autoaudiosink finds, a fakesink without
# one) and shows the stage on Xvfb. What is heard and seen is checked by
# strom's own tests (show_playback_test.rs, show_playback_stage_test.rs);
# here it is the way from cue to engine and back.
#
#   scripts/test-show-playback.sh
#   KEEP_WORK=1 scripts/test-show-playback.sh   # keep logs
#   # silent on a machine with speakers (autoaudiosink falls back to a fakesink):
#   GST_PLUGIN_FEATURE_RANK=pulsesink:NONE,pipewiresink:NONE,alsasink:NONE,openalsink:NONE scripts/test-show-playback.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STROM_PORT=8299
CORE_PORT=8298
STROM="http://127.0.0.1:${STROM_PORT}"
CORE="http://127.0.0.1:${CORE_PORT}"
WORK="$(mktemp -d)"
PIDS=()
cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT
fail() {
    echo "FAIL: $*"
    echo "--- last strom warnings" >&2
    grep -E 'WARN|ERROR' "$WORK/strom.log" | tail -n 20 >&2 || true
    KEEP_WORK=1
    exit 1
}
pass() { echo "ok   $*"; }
act() {
    local out
    out="$(curl -s -X POST -H 'content-type: application/json' -d "$1" "$CORE/api/actions")"
    jq -e '.error == null' <<<"$out" >/dev/null 2>&1 || { echo "refused: $out" >&2; return 1; }
}
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    fail "$2 did not come up"
}
# Retry `$1` (a shell test) for up to `$2` seconds.
within() {
    local deadline=$((SECONDS + $2))
    until eval "$1"; do
        ((SECONDS < deadline)) || return 1
        sleep 0.1
    done
}
playback_flow() { curl -sf "$STROM/api/flows" | jq -r '.flows[] | select(.name == "lmix playback") | .id'; }
playback() { curl -sf "$STROM/api/flows/$(playback_flow)/blocks/show/show_playback"; }
sounds() { playback | jq -c '[.sounds[] | {id, state}]'; }
pictures() { playback | jq -c '[.pictures[] | {id, state}]'; }
running() { curl -sf "$CORE/api/state" | jq -r '[.running_cues[].number] | join(",")'; }
zone_peak() { curl -sf "$CORE/api/audio/levels" | jq -r '.zones[] | select(.zone == "Hall") | .peak_db // "none"'; }
zone_gain() { curl -sf "$CORE/api/state" | jq -r '.zones[] | select(.name == "Hall") | .gain_db'; }

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"

# A 1 s and a 10 s tone, a still image.
for s in 1 10; do
    gst-launch-1.0 -q audiotestsrc num-buffers=$((s * 100)) samplesperbuffer=480 freq=440 \
        ! audio/x-raw,rate=48000,channels=1 ! wavenc ! filesink location="$WORK/tone$s.wav"
done
gst-launch-1.0 -q videotestsrc pattern=red num-buffers=1 ! video/x-raw,width=64,height=64 \
    ! pngenc ! filesink location="$WORK/red.png"
gst-launch-1.0 -q videotestsrc pattern=green num-buffers=15 ! video/x-raw,width=64,height=36,framerate=30/1 \
    ! jpegenc ! avimux ! filesink location="$WORK/green.avi"

# A screen for the stage's window.
DISPLAY_NUM=:96
Xvfb "$DISPLAY_NUM" -screen 0 1280x720x24 >/dev/null 2>&1 &
PIDS+=($!)
(cd "$WORK" && unset WAYLAND_DISPLAY && DISPLAY="$DISPLAY_NUM" GST_GL_WINDOW=x11 LIBGL_ALWAYS_SOFTWARE=1 exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
    --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "$STROM/health" strom
"$CORE_BIN" --strom "$STROM" --listen "127.0.0.1:${CORE_PORT}" --mixer cpu --slots 4 \
    --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" \
    --layout-templates "$WORK/layout-templates.json" --targets "$WORK/targets.json" \
    >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/health" lmix-core
sleep 2

# --- zone and stage: the flow follows the show and this machine's patches ---
act '{"action":"set_zone","zone":{"name":"Hall","channels":2}}' || fail "zone refused"
act '{"action":"set_zone_patch","patch":{"zone":"Hall","outputs":[{"device":"","channels":[1,2]}]}}' \
    || fail "zone patch refused"
act '{"action":"set_stage","stage":{"name":"Back","width":320,"height":180}}' || fail "stage refused"
act '{"action":"set_stage_patch","patch":{"stage":"Back","outputs":[{"display":{"mode":"window"}}]}}' \
    || fail "stage patch refused"
within '[[ -n "$(playback_flow)" ]]' 10 || fail "no playback flow in strom"
within '[[ "$(curl -sf "$STROM/api/flows/$(playback_flow)" | jq -r .flow.running)" == true ]]' 10 \
    || fail "playback flow not running"
[[ "$(playback | jq -r '[.zones[].name, .stages[].name] | join(",")')" == "Hall,Back" ]] \
    || fail "zones and stages: $(playback | jq -c '[.zones, .stages]')"
curl -sf "$STROM/api/flows/$(playback_flow)" | jq -e '.flow.elements[] | select(.id == "s0o0_screen" and .element_type == "glimagesink")' >/dev/null \
    || fail "the stage has no window"
pass "zone Hall and stage Back (window): playback flow runs"

# --- a sound cue ends with its file; the next cue follows ---
cues="$(jq -cn --arg one "$WORK/tone1.wav" --arg ten "$WORK/tone10.wav" --arg red "$WORK/red.png" --arg green "$WORK/green.avi" '[
    {number: "1", kind: "sound", sound: {file: $one}, follow: "after_end"},
    {number: "2", kind: "wait", duration_ms: 1000},
    {number: "3", kind: "sound", sound: {file: $ten, fade_out_ms: 300}},
    {number: "4", kind: "fade", duration_ms: 200, fade: {target: {zone: "Hall"}, gain_db: -20}},
    {number: "5", kind: "picture", picture: {file: $red, stage: "Back", fade_in_ms: 300}},
    {number: "6", kind: "fade", duration_ms: 200, fade: {target: {stage: "Back"}, brightness: 0}},
    {number: "7", kind: "fade", duration_ms: 300, fade: {target: {cue: "5"}, opacity: 0, then: "stop"}},
    {number: "8", kind: "sound", sound: {file: $ten, fade_in_ms: 500}},
    {number: "9", kind: "picture", picture: {file: $red, stage: "Back"}},
    {number: "10", kind: "sound", sound: {file: $one, repeat: {times: 2}}},
    {number: "11", kind: "picture", picture: {file: $green, stage: "Back", end: "stop", repeat: {times: 3}}}
]')"
act '{"action":"set_go_guard","ms":0}' || fail "GO guard refused"
act "{\"action\":\"add_cue_list\",\"name\":\"Sound\",\"cues\":$cues}" || fail "cue list refused"
within '[[ "$(sounds)" == *ready* ]]' 3 || fail "the standby sound is not held ready: $(sounds)"
pass "standby sound cue held ready (preloaded)"
act '{"action":"cue_go"}' || fail "GO refused"
within '[[ "$(sounds)" == *playing* ]]' 2 || fail "cue 1 does not play: $(sounds)"
within '[[ "$(zone_peak)" != none ]] && (( $(zone_peak | cut -d. -f1) > -60 ))' 2 \
    || fail "zone Hall has no level while it plays: $(zone_peak)"
[[ "$(running)" == 1 ]] || fail "running: $(running)"
within '[[ "$(running)" == 2 ]]' 3 || fail "cue 2 did not follow the end of the file: $(running)"
[[ "$(sounds)" != *playing* ]] || fail "the sound stayed: $(sounds)"
pass "sound cue ends with its file (metered in its zone), the next follows"

# --- stop with the cue's fade out ---
act '{"action":"cue_standby","to":{"cue":"3"}}' || fail "standby refused"
within '[[ "$(sounds)" == *ready* ]]' 3 || fail "standby 3 is not held ready: $(sounds)"
act '{"action":"cue_go"}' || fail "GO 3 refused"
within '[[ "$(sounds)" == *playing* ]]' 2 || fail "cue 3 does not play"
act '{"action":"cue_stop","cue":"3"}' || fail "stop refused"
[[ "$(sounds)" == *playing* ]] || fail "gone before its fade out"
within '[[ "$(sounds)" == "[]" ]]' 2 || fail "still there after the fade out: $(sounds)"
pass "stop fades the sound out, then it is gone"

# --- a fade cue on the zone ---
act '{"action":"cue_go"}' || fail "GO 4 refused"
within '[[ "$(running)" != *4* ]]' 2 || fail "fade cue still running: $(running)"
! grep -q "cue action failed" "$WORK/core.log" || fail "a cue action failed: $(grep 'cue action failed' "$WORK/core.log" | tail -1)"
(( $(zone_gain | cut -d. -f1) == -20 )) || fail "zone Hall at $(zone_gain) dB after the fade, not -20"
pass "fade cue sets the zone level, which the state shows"

# --- a new flow keeps the zone's level (a new zone rebuilds the flow) ---
zone_volume() { curl -sf "$STROM/api/flows/$(playback_flow)/elements/show:zone_vol_0/properties" | jq -r '.properties.volume // "none"'; }
at_minus_20() { [[ "$(zone_volume)" != none ]] && awk -v v="$(zone_volume)" 'BEGIN { exit !(v > 0.095 && v < 0.105) }'; }
zone_names() { playback | jq -r '[.zones[].name] | join(",")'; }
within 'at_minus_20' 2 || fail "zone Hall plays at volume $(zone_volume), not -20 dB"
act '{"action":"set_zone","zone":{"name":"Foyer","channels":2}}' || fail "second zone refused"
act '{"action":"set_zone_patch","patch":{"zone":"Foyer","outputs":[{"device":"","channels":[1,2]}]}}' \
    || fail "second zone patch refused"
within '[[ "$(zone_names)" == Hall,Foyer ]]' 10 || fail "the flow did not take the new zone: $(zone_names)"
within 'at_minus_20' 3 || fail "zone Hall back at volume $(zone_volume) in the new flow, not -20 dB"
act '{"action":"remove_zone","name":"Foyer"}' || fail "remove second zone refused"
within '[[ "$(zone_names)" == Hall ]]' 10 || fail "the flow kept the removed zone: $(zone_names)"
within 'at_minus_20' 3 || fail "zone Hall back at volume $(zone_volume) after removing a zone, not -20 dB"
! grep -q "zones are not running\|flow is not running" "$WORK/core.log" \
    || fail "a zone level met a flow that was not running: $(grep -m1 'not running' "$WORK/core.log")"
pass "a new flow plays the zone at its level"

# --- a picture cue shows until stopped; fades on the stage and the cue ---
within '[[ "$(pictures)" == *ready* ]]' 3 || fail "the standby picture is not held ready: $(pictures)"
act '{"action":"cue_go"}' || fail "GO 5 refused"
within '[[ "$(pictures)" == *playing* ]]' 2 || fail "cue 5 does not show: $(pictures)"
sleep 1
[[ "$(running)" == *5* ]] || fail "a still image ended by itself: $(running)"
act '{"action":"cue_go"}' || fail "GO 6 refused"
within '[[ "$(running)" != *6* ]]' 2 || fail "stage fade still running: $(running)"
act '{"action":"cue_go"}' || fail "GO 7 refused"
within '[[ "$(pictures)" == "[]" ]]' 2 || fail "the picture stayed after its fade with stop: $(pictures)"
within '[[ "$(running)" != *5* ]]' 2 || fail "cue 5 did not end with its picture: $(running)"
! grep -q "cue action failed" "$WORK/core.log" || fail "a cue action failed: $(grep 'cue action failed' "$WORK/core.log" | tail -1)"
pass "picture cue shows, stage and opacity fade, the cue ends with its picture"

# --- panic fades every sound and picture out ---
act '{"action":"cue_go"}' || fail "GO 8 refused"
act '{"action":"cue_go"}' || fail "GO 9 refused"
within '[[ "$(sounds)" == *playing* && "$(pictures)" == *playing* ]]' 2 \
    || fail "cues 8 and 9 do not play: $(sounds) $(pictures)"
act '{"action":"cue_panic","ms":400}' || fail "panic refused"
within '[[ "$(sounds)" != *playing* && "$(pictures)" != *playing* ]]' 2 \
    || fail "panic left: $(sounds) $(pictures)"
[[ "$(sounds)" == *ready* ]] || fail "the panic took the standby cue's preload: $(sounds)"
[[ -z "$(running)" ]] || fail "cues run after the panic: $(running)"
pass "panic stops every sound and picture, the standby cue stays ready"

# --- a sound cue played twice runs 2 s without a gap, then ends ---
act '{"action":"cue_go"}' || fail "GO 10 refused"
within '[[ "$(sounds)" == *playing* ]]' 2 || fail "cue 10 does not play"
sleep 1.4
[[ "$(running)" == *10* ]] || fail "cue 10 ended after one play"
within '[[ "$(running)" != *10* ]]' 2 || fail "cue 10 did not end after two plays: $(running)"
pass "a sound cue repeated twice ends after its second play"

# --- a video cue played 3 times (0.5 s each) leaves after its last play ---
act '{"action":"cue_go"}' || fail "GO 11 refused"
within '[[ "$(pictures)" == *playing* ]]' 2 || fail "cue 11 does not show"
sleep 1
[[ "$(running)" == *11* ]] || fail "cue 11 ended after one play"
within '[[ "$(running)" != *11* ]]' 2 || fail "cue 11 did not end after three plays: $(running)"
[[ "$(pictures)" == "[]" ]] || fail "the video stayed: $(pictures)"
pass "a video cue played 3 times leaves after its last play"

# --- no zones and no stages: the flow goes ---
act '{"action":"remove_zone","name":"Hall"}' || fail "remove zone refused"
[[ -n "$(playback_flow)" ]] || fail "the stage alone has no flow"
act '{"action":"remove_stage","name":"Back"}' || fail "remove stage refused"
within '[[ -z "$(playback_flow)" ]]' 5 || fail "playback flow stayed without zones and stages"
pass "without zones and stages no playback flow"
