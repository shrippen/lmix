#!/usr/bin/env bash
# Sound cues on strom (show mode SND, S-E.3 B): a zone on this machine's
# default output, a sound cue that ends with its file and lets the next
# cue follow, a stop with fade, a fade cue on a zone, a panic. Runs
# strom's Show Sound block for real; the VM has no sound card, so the
# default output is whatever autoaudiosink finds (a fakesink without
# one). What is heard is checked by strom's own tests
# (show_sound_test.rs); here it is the way from cue to engine and back.
#
#   scripts/test-sound-cues.sh
#   KEEP_WORK=1 scripts/test-sound-cues.sh   # keep logs
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
sound_flow() { curl -sf "$STROM/api/flows" | jq -r '.flows[] | select(.name == "lmix sound") | .id'; }
sounds() { curl -sf "$STROM/api/flows/$(sound_flow)/blocks/snd/show_sound" | jq -c '[.sounds[] | {id, state}]'; }
running() { curl -sf "$CORE/api/state" | jq -r '[.running_cues[].number] | join(",")'; }

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"

# A 1 s and a 10 s tone.
for s in 1 10; do
    gst-launch-1.0 -q audiotestsrc num-buffers=$((s * 100)) samplesperbuffer=480 freq=440 \
        ! audio/x-raw,rate=48000,channels=1 ! wavenc ! filesink location="$WORK/tone$s.wav"
done

(cd "$WORK" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
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

# --- zones: the flow follows the show and this machine's patch ---
act '{"action":"set_zone","zone":{"name":"Hall","channels":2}}' || fail "zone refused"
act '{"action":"set_zone_patch","patch":{"zone":"Hall","outputs":[{"device":"","channels":[1,2]}]}}' \
    || fail "patch refused"
within '[[ -n "$(sound_flow)" ]]' 10 || fail "no sound flow in strom"
within '[[ "$(curl -sf "$STROM/api/flows/$(sound_flow)" | jq -r .flow.running)" == true ]]' 10 \
    || fail "sound flow not running"
pass "zone Hall: sound flow plays"

# --- a sound cue ends with its file; the next cue follows ---
cues="$(jq -cn --arg one "$WORK/tone1.wav" --arg ten "$WORK/tone10.wav" '[
    {number: "1", kind: "sound", sound: {file: $one}, follow: "after_end"},
    {number: "2", kind: "wait", duration_ms: 1000},
    {number: "3", kind: "sound", sound: {file: $ten, fade_out_ms: 300}},
    {number: "4", kind: "fade", duration_ms: 200, fade: {target: {zone: "Hall"}, gain_db: -20}},
    {number: "5", kind: "sound", sound: {file: $ten, fade_in_ms: 500}}
]')"
act '{"action":"set_go_guard","ms":0}' || fail "GO guard refused"
act "{\"action\":\"add_cue_list\",\"name\":\"Sound\",\"cues\":$cues}" || fail "cue list refused"
act '{"action":"cue_go"}' || fail "GO refused"
within '[[ "$(sounds)" == *playing* ]]' 2 || fail "cue 1 does not play: $(sounds)"
[[ "$(running)" == 1 ]] || fail "running: $(running)"
within '[[ "$(running)" == 2 ]]' 3 || fail "cue 2 did not follow the end of the file: $(running)"
[[ "$(sounds)" == "[]" ]] || fail "the sound stayed: $(sounds)"
pass "sound cue ends with its file, the next follows"

# --- stop with the cue's fade out ---
act '{"action":"cue_standby","to":{"cue":"3"}}' || fail "standby refused"
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
pass "fade cue sets the zone level"

# --- panic fades every sound out ---
act '{"action":"cue_go"}' || fail "GO 5 refused"
within '[[ "$(sounds)" == *playing* ]]' 2 || fail "cue 5 does not play"
act '{"action":"cue_panic","ms":400}' || fail "panic refused"
within '[[ "$(sounds)" == "[]" ]]' 2 || fail "panic left sounds: $(sounds)"
[[ -z "$(running)" ]] || fail "cues run after the panic: $(running)"
pass "panic stops every sound"

# --- no zones: the flow goes ---
act '{"action":"remove_zone","name":"Hall"}' || fail "remove zone refused"
within '[[ -z "$(sound_flow)" ]]' 5 || fail "sound flow stayed without zones"
pass "without zones no sound flow"
