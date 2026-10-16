#!/usr/bin/env bash
# Looks on the GL mixer (software GL on a virtual display is enough):
# a grey input gets brighter with a look and back to grey without.
# Needs Xvfb, ImageMagick, jq.
#
#   scripts/test-looks-gl.sh
#   KEEP_WORK=1 scripts/test-looks-gl.sh   # keep logs
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STROM_PORT=8798
CORE_PORT=8898
CORE="http://127.0.0.1:${CORE_PORT}/api"
DISPLAY_NO=:96
WORK="$(mktemp -d)"
PIDS=()
# Software GL renders the program seconds late.
SETTLE=15

cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT
pass() { echo "ok   $*"; }
fail() {
    echo "FAIL: $*"
    KEEP_WORK=1
    exit 1
}
act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null; }
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    fail "$2 did not come up"
}
# Mean grey of the program picture, 0-255. The first request wakes the
# picture tap (while it sleeps lmix-core serves the last frame).
grey() {
    curl -sf "$CORE/pictures/program" -o /dev/null || true
    sleep 3
    for _ in $(seq 1 15); do curl -sf "$CORE/pictures/program" -o "$WORK/p.jpg" && break; sleep 1; done
    convert "$WORK/p.jpg" -colorspace gray -format '%[fx:int(mean*255)]' info:
}

STROM_BIN="$("$ROOT/scripts/fetch-strom.sh")"
(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"

Xvfb "$DISPLAY_NO" -screen 0 1280x720x24 >/dev/null 2>&1 &
PIDS+=($!)
(cd "$WORK" && GST_GL_WINDOW=x11 DISPLAY="$DISPLAY_NO" exec "$STROM_BIN" \
    --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart --data-dir "$WORK/strom-data" \
    >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
"$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" --mixer gl \
    --slots 2 --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" --layout-templates "$WORK/layout-templates.json" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core

[[ "$(curl -sf "$CORE/state" | jq '.looks_available')" == true ]] || fail "GL mixer offers no looks"
act '{"action":"add_input","source":{"kind":"color","color":"#808080"},"name":"Grey"}'
act '{"action":"cut_direct","input":"Grey"}'
sleep "$SETTLE"
before="$(grey)"
act '{"action":"set_look","input":"Grey","look":{"brightness":0.4}}'
sleep "$SETTLE"
bright="$(grey)"
((bright > before + 50)) || fail "look did not brighten program ($before -> $bright)"
act '{"action":"set_look","input":"Grey","look":{}}'
sleep "$SETTLE"
back="$(grey)"
((back < before + 10)) || fail "neutral look did not restore program ($before -> $back)"
pass "looks on the GL mixer: grey $before, brighter $bright, neutral again $back"
