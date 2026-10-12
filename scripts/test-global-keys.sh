#!/usr/bin/env bash
# Global shortcuts (AUT.12) without a desktop: a private session bus with
# a stand-in GlobalShortcuts portal (workspace/tools/fake-portal), the
# Console on Xvfb and lmix-core without engine. Checks that the Console
# binds the show's shortcut keys and runs a shortcut when the portal
# reports a press.
#
#   scripts/test-global-keys.sh
#   KEEP_WORK=1 scripts/test-global-keys.sh   # keep logs and screenshot
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# Re-run inside a private session bus.
if [[ -z "${LMIX_IN_BUS:-}" ]]; then
    for crate in lmix-core console-qt tools/fake-portal; do
        (cd "$ROOT/workspace/$crate" && cargo build --release >/dev/null 2>&1) || {
            echo "FAIL: build $crate"
            exit 1
        }
    done
    LMIX_IN_BUS=1 exec dbus-run-session -- "$0" "$@" 2> >(grep -v "fd limit" >&2)
fi

PORT=8893
CORE="http://127.0.0.1:${PORT}/api"
DISPLAY_NUM=:94
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
act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null; }
wait_for() { for _ in $(seq 1 40); do "$@" && return 0; sleep 0.25; done; return 1; }
target() { echo "${CARGO_TARGET_DIR:-$ROOT/workspace/$1/target}/release/$2"; }

mkfifo "$WORK/portal.in"
exec 5<>"$WORK/portal.in"
"$(target tools/fake-portal fake-portal)" <"$WORK/portal.in" >"$WORK/portal.out" 2>"$WORK/portal.err" &
PIDS+=($!)
Xvfb "$DISPLAY_NUM" -screen 0 1440x900x24 >/dev/null 2>&1 &
PIDS+=($!)
"$(target lmix-core lmix-core)" --engine none --listen "127.0.0.1:${PORT}" --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_for curl -sf "http://127.0.0.1:${PORT}/health" -o /dev/null || fail "lmix-core did not come up"

act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Bars"}'
act '{"action":"add_input","source":{"kind":"pattern","pattern":"ball"},"name":"Ball"}'
act '{"action":"cut_direct","input":"Bars"}'
act '{"action":"add_shortcut","shortcut":{"label":"Ball on air","key":"Ctrl+Shift+F1","actions":[{"action":"cut_direct","input":"Ball"}]}}'

# The Console with "keys also work when not in front" switched on.
mkdir -p "$WORK/config/lmix"
printf '[keys]\nglobal=true\n' >"$WORK/config/lmix/console.conf"
XDG_CONFIG_HOME="$WORK/config" DISPLAY="$DISPLAY_NUM" QT_QPA_PLATFORM=xcb LMIX_CORE="http://127.0.0.1:${PORT}" \
    "$(target console-qt lmix-console)" >"$WORK/console.log" 2>&1 &
PIDS+=($!)

bound() { grep -q '"trigger":"CTRL+SHIFT+F1"' "$WORK/portal.out"; }
wait_for bound || fail "Console did not bind the shortcut key: $(cat "$WORK/portal.out")"
grep -q '"description":"Ball on air"' "$WORK/portal.out" || fail "binding without the label"
pass "Console binds the show's shortcut keys at the portal"

id="$(curl -sf "$CORE/state" | jq -r '.shortcuts[0].id')"
ball="$(curl -sf "$CORE/state" | jq -r '.inputs[] | select(.name=="Ball") | .id')"
echo "press $id" >&5
on_air() { [[ "$(curl -sf "$CORE/state" | jq -r '.program')" == "$ball" ]]; }
wait_for on_air || fail "a press reported by the portal did not run the shortcut"
pass "a press reported by the portal runs the shortcut"

# A changed key is bound again.
act "{\"action\":\"set_shortcut\",\"target\":1,\"shortcut\":{\"label\":\"Ball on air\",\"key\":\"F9\",\"actions\":[{\"action\":\"cut_direct\",\"input\":\"Ball\"}]}}"
rebound() { grep -q '"trigger":"F9"' "$WORK/portal.out"; }
wait_for rebound || fail "changed key not bound again"
pass "a changed key is bound again"
echo "global keys check passed"
