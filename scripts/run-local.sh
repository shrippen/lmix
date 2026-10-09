#!/usr/bin/env bash
# Engine, lmix-core and the Qt Console on this machine, with a few test
# inputs; closing the Console stops everything.
#
#   scripts/run-local.sh              # CUDA mixer, 4 test inputs
#   MIXER=gl INPUTS=8 scripts/run-local.sh
#   KEEP_WORK=1 scripts/run-local.sh  # keep logs (path printed at the end)
set -euo pipefail

MIXER="${MIXER:-cuda}"
INPUTS="${INPUTS:-4}"
STROM_PORT="${STROM_PORT:-8500}"
CORE_PORT="${CORE_PORT:-8600}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="http://127.0.0.1:${CORE_PORT}/api"
WORK="$(mktemp -d)"
LIVE_DIR="${XDG_RUNTIME_DIR:-/tmp}/lmix-local"
PIDS=()

cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    rm -rf "$LIVE_DIR"
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT

act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null; }
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    echo "$2 did not come up (logs: $WORK)" >&2
    KEEP_WORK=1
    exit 1
}

STROM_BIN="$("$ROOT/scripts/fetch-strom.sh")"
(cd "$ROOT/workspace/lmix-core" && cargo build --release -q)
(cd "$ROOT/workspace/console-qt" && cargo build --release -q)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
CONSOLE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/console-qt/target}/release/lmix-console"

(cd "$WORK" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
    --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
"$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" --mixer "$MIXER" \
    --slots "$INPUTS" --autosave "$WORK/autosave.lmix" --live-dir "$LIVE_DIR" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core

patterns=(ball bars snow checkers)
for i in $(seq 1 "$INPUTS"); do
    pattern="${patterns[$(((i - 1) % ${#patterns[@]}))]}"
    act "{\"action\":\"add_input\",\"source\":{\"kind\":\"pattern\",\"pattern\":\"$pattern\"},\"name\":\"${pattern^}\"}"
done
act '{"action":"cut_direct","input":1}'

LMIX_CORE="http://127.0.0.1:${CORE_PORT}" "$CONSOLE_BIN" "$@" >"$WORK/console.log" 2>&1
