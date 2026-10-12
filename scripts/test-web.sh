#!/usr/bin/env bash
# lmix-core's browser pages (mixer, tally) in Chromium, against the null
# engine (no strom needed). Needs node with playwright (Chromium).
#
#   scripts/test-web.sh
#   KEEP_WORK=1 scripts/test-web.sh   # keep logs and screenshots
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT=8897
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
act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/api/actions" >/dev/null; }

(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
"$CORE_BIN" --engine none --listen "127.0.0.1:${PORT}" --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 1 60); do curl -sf "$CORE/health" >/dev/null && break; sleep 0.5; done

act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Bars"}'
act '{"action":"add_input","source":{"kind":"pattern","pattern":"ball"},"name":"Ball"}'
act '{"action":"cut_direct","input":1}'
act '{"action":"add_shortcut","shortcut":{"label":"Ball on air","key":"F2","web":true,"color":"red","actions":[{"action":"preview_input","input":2},{"action":"cut"}]}}'

export NODE_PATH="${NODE_PATH:-$(npm root -g)}"
SHOT_DIR="$WORK" node "$ROOT/scripts/web-check.cjs" "$CORE" || fail "web pages"
echo "web pages check passed"
