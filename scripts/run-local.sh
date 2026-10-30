#!/usr/bin/env bash
# Engine, lmix-core and lmix on this machine, with a few test inputs;
# closing the window stops everything. APP is the page to open (mix, show,
# audio, outputs, setup; empty: the page open last), APP=both two windows,
# Mix and Show (closing the Mix one stops everything). Arguments go to
# lmix, e.g. --window maximized. ./start.sh is the short way.
#
#   scripts/run-local.sh              # CUDA mixer, 4 test inputs
#   APP=audio scripts/run-local.sh    # on the Audio page
#   MIXER=gl INPUTS=8 scripts/run-local.sh
#   SLOTS=31 scripts/run-local.sh     # mixer slots (most inputs at once); lmix-core's default otherwise
#   APP=both scripts/run-local.sh     # two windows, Mix and Show
#   KEEP_WORK=1 scripts/run-local.sh  # keep logs (path printed at the end)
#   DECKLINK_PLUGIN=system …          # decklink plugin without lmix's patch
set -euo pipefail

MIXER="${MIXER:-cuda}"
INPUTS="${INPUTS:-4}"
# Slots are what a show can hold, the test inputs only fill some: without
# SLOTS lmix-core takes its default (16), never fewer than INPUTS.
SLOTS="${SLOTS:-}"
((INPUTS <= 16)) || SLOTS="${SLOTS:-$INPUTS}"
APP="${APP:-}"
[[ "$APP" =~ ^(|mix|show|audio|outputs|setup|both)$ ]] \
    || { echo "APP is mix, show, audio, outputs, setup, both or empty, not $APP" >&2; exit 1; }
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
# With a DeckLink card: GStreamer's decklink plugin with lmix's patch (a
# card output started again shows every frame late without it).
if [[ "${DECKLINK_PLUGIN:-patched}" == patched ]] && compgen -G "/dev/blackmagic*" >/dev/null; then
    if plugin="$("$ROOT/scripts/decklink-plugin.sh")"; then
        export GST_PLUGIN_PATH="$plugin${GST_PLUGIN_PATH:+:$GST_PLUGIN_PATH}"
    else
        echo "run-local: decklink plugin without lmix's patch (scripts/decklink-plugin.sh failed)" >&2
    fi
fi
(cd "$ROOT/workspace/lmix-core" && cargo build --release -q)
(cd "$ROOT/workspace/console-qt" && cargo build --release -q)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
CONSOLE_DIR="${CARGO_TARGET_DIR:-$ROOT/workspace/console-qt/target}/release"

(cd "$WORK" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
    --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
"$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" --mixer "$MIXER" \
    ${SLOTS:+--slots "$SLOTS"} --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" --layout-templates "$WORK/layout-templates.json" --live-dir "$LIVE_DIR" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core

patterns=(ball bars snow checkers)
for i in $(seq 1 "$INPUTS"); do
    pattern="${patterns[$(((i - 1) % ${#patterns[@]}))]}"
    act "{\"action\":\"add_input\",\"source\":{\"kind\":\"pattern\",\"pattern\":\"$pattern\"},\"name\":\"${pattern^}\"}"
done
act '{"action":"cut_direct","input":1}'

export LMIX_CORE="http://127.0.0.1:${CORE_PORT}"
case "$APP" in
"")
    "$CONSOLE_DIR/lmix" "$@" >"$WORK/lmix.log" 2>&1
    ;;
mix | show | audio | outputs | setup)
    "$CONSOLE_DIR/lmix" --page "$APP" "$@" >"$WORK/lmix.log" 2>&1
    ;;
both)
    "$CONSOLE_DIR/lmix" --page show "$@" >"$WORK/lmix-show.log" 2>&1 &
    PIDS+=($!)
    "$CONSOLE_DIR/lmix" --page mix "$@" >"$WORK/lmix.log" 2>&1
    ;;
esac
