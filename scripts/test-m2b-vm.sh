#!/usr/bin/env bash
# M2b check without GPU: strom (CPU mixer) + lmix-core, driven over the
# lmix API. Proves slot flows, preview/take, source swap and show file.
# Picture quality, latency and GPU paths: docs/HARDWARE-TODO.md.
#
#   scripts/test-m2b-vm.sh            # builds, runs, cleans up
#   CONSOLE_SHOT=out.png scripts/test-m2b-vm.sh
#                                     # also screenshots the Console
#                                     # (lmix-core client) on Xvfb
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STROM_PORT=8199
CORE_PORT=8299
STROM="http://127.0.0.1:${STROM_PORT}"
CORE="http://127.0.0.1:${CORE_PORT}/api"
WORK="$(mktemp -d)"
# Mixer slots for the test; small to fit a VM.
SLOTS=4
# Mean brightness (0-100) bars or snow exceed; black is 0.
MIN_BRIGHTNESS=30
# Thumbnail taps refresh about once a second.
TAP_INTERVAL=1.5

PIDS=()
cleanup() {
    for pid in "${PIDS[@]}"; do
        kill "$pid" 2>/dev/null || true
    done
    rm -rf "$WORK"
}
trap cleanup EXIT

fail() { echo "FAIL: $*" >&2; exit 1; }
pass() { echo "ok   $*"; }

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
(cd "$WORK" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "$STROM/health" strom

start_core() {
    "$CORE_BIN" --strom "$STROM" --listen "127.0.0.1:${CORE_PORT}" --mixer cpu \
        --slots "$SLOTS" --autosave "$WORK/autosave.lmix" >>"$WORK/core.log" 2>&1 &
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
state="$(curl -sf "$CORE/state")"
[[ "$(jq '[.inputs[] | select(.status=="online")] | length' <<<"$state")" == 2 ]] \
    || fail "inputs not online: $state"
pass "two inputs online"

# --- preview and take ---
act '{"action":"preview_input","input":"Bars"}' >/dev/null
act '{"action":"cut"}' >/dev/null
act '{"action":"preview_input","input":"Ball"}' >/dev/null
act '{"action":"transition","slot":1}' >/dev/null
sleep 1
[[ "$(curl -sf "$CORE/tally")" == "[2,1]" ]] || fail "tally $(curl -sf "$CORE/tally")"
pass "preview, cut, fade; tally"

MIXER_ID="$(curl -sf "$STROM/api/flows" | jq -r '.flows[] | select(.name=="lmix mixer") | .id')"
# Pictures through lmix-core (503 until the tap has a frame; the first
# request switches it on).
picture() {
    local path="$1" out="$2"
    rm -f "$out"
    for _ in $(seq 1 10); do
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

program_moves || fail "program frozen or black with Ball on air"
pass "program moves (Ball over inter)"
picture inputs/preview "$WORK/pvw.jpg"
[[ $(brightness "$WORK/pvw.jpg") -gt $MIN_BRIGHTNESS ]] || fail "preview picture dark"
act '{"action":"cut_direct","input":"Bars"}' >/dev/null
sleep "$TAP_INTERVAL"
picture program "$WORK/pgm.jpg"
[[ $(brightness "$WORK/pgm.jpg") -gt $MIN_BRIGHTNESS ]] || fail "program dark with Bars on air"
pass "pictures show content (preview, program)"

# --- swap the program source while on air ---
act '{"action":"replace_source","input":"Bars","source":{"kind":"pattern","pattern":"snow"}}' >/dev/null
sleep 2
running="$(curl -sf "$STROM/api/flows/$MIXER_ID" | jq -r '.flow.running')"
[[ "$running" == true ]] || fail "mixer stopped after source swap"
program_moves || fail "program frozen after source swap"
[[ $(brightness "$WORK/b.jpg") -gt $MIN_BRIGHTNESS ]] || fail "program dark after source swap"
pass "program source swapped on air: mixer kept running, new picture moves"

# --- Console as lmix-core client, on a virtual display ---
if [[ -n "${CONSOLE_SHOT:-}" ]]; then
    (cd "$ROOT/workspace/app-desktop" && cargo build)
    CONSOLE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/app-desktop/target}/debug/app-desktop"
    Xvfb :93 -screen 0 1280x860x24 >/dev/null 2>&1 &
    PIDS+=($!)
    sleep 1
    DISPLAY=:93 WAYLAND_DISPLAY= LIBGL_ALWAYS_SOFTWARE=1 "$CONSOLE_BIN" --core "${CORE%/api}" >"$WORK/console.log" 2>&1 &
    PIDS+=($!)
    sleep "${CONSOLE_WAIT:-8}"
    DISPLAY=:93 import -window root "$CONSOLE_SHOT"
    pass "Console screenshot: $CONSOLE_SHOT"
fi

# --- show file ---
act "{\"action\":\"save_show\",\"path\":\"$WORK/test.lmix\"}" >/dev/null
act '{"action":"new_show"}' >/dev/null
[[ "$(curl -sf "$CORE/state" | jq '.inputs | length')" == 0 ]] || fail "new show not empty"
act "{\"action\":\"open_show\",\"path\":\"$WORK/test.lmix\"}" >/dev/null
sleep 2
state="$(curl -sf "$CORE/state")"
[[ "$(jq '.inputs | length' <<<"$state")" == 2 ]] || fail "show not restored: $state"
pass "show saved, cleared, reopened"

# --- crash of lmix-core: engine keeps running, show comes back ---
kill -9 "$CORE_PID"
sleep 1
running="$(curl -sf "$STROM/api/flows/$MIXER_ID" | jq -r '.flow.running')"
[[ "$running" == true ]] || fail "mixer died with lmix-core"
start_core
sleep 2
[[ "$(curl -sf "$CORE/state" | jq '.inputs | length')" == 2 ]] || fail "autosave not resumed"
pass "lmix-core killed and restarted: mixer kept running, show resumed"

criticals="$(grep -c 'GStreamer-CRITICAL' "$WORK/strom.log" || true)"
[[ "$criticals" == 0 ]] || fail "$criticals GStreamer CRITICALs in strom.log"
pass "no GStreamer CRITICALs"

echo "M2b VM check passed"
