#!/usr/bin/env bash
# M2b check without GPU: strom (CPU mixer) + lmix-core, driven over the
# lmix API. Proves slot flows, preview/take, source swap and show file.
# Picture quality, latency and GPU paths: docs/HARDWARE-TODO.md.
#
#   scripts/test-m2b-vm.sh            # builds, runs, cleans up
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STROM_PORT=8199
CORE_PORT=8299
STROM="http://127.0.0.1:${STROM_PORT}"
CORE="http://127.0.0.1:${CORE_PORT}/api"
WORK="$(mktemp -d)"
# Mixer slots for the test; small to fit a VM.
SLOTS=4
# Frames the PGM thumbnail must deliver as proof of a picture.
MIN_JPEG_BYTES=1000

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
(cd "$ROOT/workspace/strom" && env -u CARGO_TARGET_DIR cargo build --release --features no-gui --bin strom)
(cd "$ROOT/workspace/lmix-core" && cargo build)
STROM_BIN="$ROOT/workspace/strom/target/release/strom"
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
# The first request switches the tap on; frames follow within a second or two.
for _ in $(seq 1 10); do
    curl -sf "$STROM/api/flows/$MIXER_ID/blocks/tap_pgm/thumbnail" -o "$WORK/pgm.jpg" && break
    sleep 1
done
[[ -s "$WORK/pgm.jpg" ]] || fail "no PGM thumbnail"
[[ $(stat -c %s "$WORK/pgm.jpg") -gt $MIN_JPEG_BYTES ]] || fail "PGM thumbnail empty"
pass "PGM carries a picture"

# --- swap source while on air ---
act '{"action":"replace_source","input":"Ball","source":{"kind":"pattern","pattern":"checkers"}}' >/dev/null
sleep 2
running="$(curl -sf "$STROM/api/flows/$MIXER_ID" | jq -r '.flow.running')"
[[ "$running" == true ]] || fail "mixer stopped after source swap"
pass "source swapped, mixer kept running"

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

echo "M2b VM check passed"
