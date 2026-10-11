#!/usr/bin/env bash
# Titles end to end (M7 without an engine): lmix-core with the null engine
# serves the graphics page, Chromium renders it as the engine's browser
# would, and the check drives titles through the API: the built-in Kante
# templates, a CasparCG-style template with XML data from a templates
# folder that is edited while it plays (reload, data kept), a countdown
# reporting zero, and a CSV data source feeding a field. Needs node with
# playwright (Chromium); no GPU, no strom.
#
#   scripts/test-titles.sh
#   KEEP_WORK=1 scripts/test-titles.sh   # keep logs and screenshots
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT=8898
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

mkdir -p "$WORK/templates"
(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
"$CORE_BIN" --engine none --listen "127.0.0.1:${PORT}" --autosave "$WORK/autosave.lmix" \
    --templates "$WORK/templates" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 1 60); do curl -sf "$CORE/health" >/dev/null && break; sleep 0.5; done
curl -sf "$CORE/health" >/dev/null || fail "lmix-core did not start"

export NODE_PATH="${NODE_PATH:-$(npm root -g)}"
WORK="$WORK" node "$ROOT/scripts/titles-check.cjs" "$CORE" || fail "titles"
grep -E ' (ERROR|WARN) ' "$WORK/core.log" && fail "lmix-core logged errors"
echo "titles check passed"
