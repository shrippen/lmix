#!/usr/bin/env bash
# lmix-core without GUI (OPS.1): show loaded at start, text protocol over
# TCP (API.5), /metrics (OPS.3), clean stop on SIGTERM. Null engine, no
# strom needed.
#
#   scripts/test-headless.sh
#   KEEP_WORK=1 scripts/test-headless.sh   # keep logs
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT=8898
TCP_PORT=8899
CORE="http://127.0.0.1:${PORT}"
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
    KEEP_WORK=1
    exit 1
}
pass() { echo "ok   $*"; }
act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/api/actions" >/dev/null; }
up() { for _ in $(seq 1 60); do curl -sf "$CORE/health" >/dev/null && return 0; sleep 0.5; done; return 1; }

(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
start() {
    "$CORE_BIN" --engine none --listen "127.0.0.1:${PORT}" --tcp "127.0.0.1:${TCP_PORT}" \
        --autosave "$WORK/autosave.lmix" "$@" >>"$WORK/core.log" 2>&1 &
    PIDS+=($!)
    up || fail "lmix-core did not come up"
}

# A show saved by one run is loaded by the next one at start.
start
act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Bars"}'
act '{"action":"add_input","source":{"kind":"pattern","pattern":"ball"},"name":"Ball"}'
act "{\"action\":\"save_show\",\"path\":\"$WORK/sunday.lmix\"}"
act '{"action":"new_show"}'
kill -TERM "${PIDS[-1]}"
wait "${PIDS[-1]}" || fail "lmix-core did not stop cleanly on SIGTERM"
grep -q "stopping" "$WORK/core.log" || fail "no stop logged"
pass "SIGTERM: stopped cleanly"

LMIX_SHOW="$WORK/sunday.lmix" start
[[ "$(curl -sf "$CORE/api/state" | jq -r '[.inputs[].name] | join(",")')" == "Bars,Ball" ]] \
    || fail "show from LMIX_SHOW not loaded"
pass "show loaded at start"

# Text protocol: greeting, subscription, an action, its tally.
exec 3<>"/dev/tcp/127.0.0.1/${TCP_PORT}"
read_line() { local line; IFS= read -r -t 5 line <&3 || fail "no line from the text protocol"; printf '%s' "${line%$'\r'}"; }
[[ "$(read_line)" == "LMIX 1" ]] || fail "no greeting"
printf 'SUB TALLY\n' >&3
[[ "$(read_line)" == OK ]] || fail "SUB refused"
printf '{"action":"cut_direct","input":"Ball"}\n' >&3
got="$(read_line) $(read_line)"
[[ "$got" == "OK TALLY 01" || "$got" == "TALLY 01 OK" ]] || fail "action over TCP: got '$got'"
printf '{"action":"preview_input","input":7}\n' >&3
[[ "$(read_line)" == "ER unknown_input"* ]] || fail "refusal without reason"
printf 'QUIT\n' >&3
exec 3<&-
pass "text protocol: action, tally subscription, refusal with reason"

metrics="$(curl -sf "$CORE/metrics")"
grep -q '^lmix_input_tally{input="2",name="Ball"} 1$' <<<"$metrics" || fail "metrics: tally of Ball"
grep -q '^lmix_uptime_seconds [0-9]' <<<"$metrics" || fail "metrics: uptime"
grep -q '^# TYPE lmix_input_online gauge$' <<<"$metrics" || fail "metrics: input health"
pass "metrics: Prometheus text with inputs and tally"
echo "headless check passed"
