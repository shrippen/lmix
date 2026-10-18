#!/usr/bin/env bash
# Global shortcuts (AUT.12) without a desktop: a private session bus with
# a stand-in GlobalShortcuts portal (workspace/tools/fake-portal), lmix
# Mixer on Xvfb and lmix-core without engine. Checks that the mixer binds
# the show's shortcut keys and runs a shortcut when the portal reports a
# press. Defaults (owner, 18.10.2026): lmix Mixer binds only when switched
# on, lmix Show binds unless switched off.
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
"$(target lmix-core lmix-core)" --engine none --listen "127.0.0.1:${PORT}" --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" --layout-templates "$WORK/layout-templates.json" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_for curl -sf "http://127.0.0.1:${PORT}/health" -o /dev/null || fail "lmix-core did not come up"

act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Bars"}'
act '{"action":"add_input","source":{"kind":"pattern","pattern":"ball"},"name":"Ball"}'
act '{"action":"cut_direct","input":"Bars"}'
act '{"action":"add_shortcut","shortcut":{"label":"Ball on air","key":"Ctrl+Shift+F1","actions":[{"action":"cut_direct","input":"Ball"}]}}'

window() { DISPLAY="$DISPLAY_NUM" xdotool search --name "$1" 2>/dev/null | wc -l; }

# The mixer by default: binds nothing.
mkdir -p "$WORK/fresh"
XDG_CONFIG_HOME="$WORK/fresh" DISPLAY="$DISPLAY_NUM" QT_QPA_PLATFORM=xcb LMIX_CORE="http://127.0.0.1:${PORT}" \
    "$(target console-qt lmix-mixer)" >"$WORK/mixer-default.log" 2>&1 &
first=$!
mixer_up() { [[ "$(window '^lmix (Mixer|Mischer)')" -ge 1 ]]; }
wait_for mixer_up || fail "lmix Mixer did not open"
sleep 3
[[ ! -s "$WORK/portal.out" ]] || fail "lmix Mixer bound keys by default: $(cat "$WORK/portal.out")"
kill "$first"
wait "$first" 2>/dev/null || true
pass "lmix Mixer binds no global keys by default"

# The mixer with "keys also work when not in front" switched on.
mkdir -p "$WORK/config/lmix"
printf '[keys]\nglobal=true\n' >"$WORK/config/lmix/mixer.conf"
XDG_CONFIG_HOME="$WORK/config" DISPLAY="$DISPLAY_NUM" QT_QPA_PLATFORM=xcb LMIX_CORE="http://127.0.0.1:${PORT}" \
    "$(target console-qt lmix-mixer)" >"$WORK/console.log" 2>&1 &
PIDS+=($!)

bound() { grep -q '"trigger":"CTRL+SHIFT+F1"' "$WORK/portal.out"; }
wait_for bound || fail "mixer did not bind the shortcut key: $(cat "$WORK/portal.out")"
grep -q '"description":"Ball on air"' "$WORK/portal.out" || fail "binding without the label"
pass "mixer binds the show's shortcut keys at the portal"

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

# lmix Show by default: binds the keys too, also after an update, when it
# takes over the old console.conf with its "global=false" written by default.
printf '[keys]\nglobal=false\n' >"$WORK/config/lmix/console.conf"
before="$(wc -l <"$WORK/portal.out")"
XDG_CONFIG_HOME="$WORK/config" DISPLAY="$DISPLAY_NUM" QT_QPA_PLATFORM=xcb LMIX_CORE="http://127.0.0.1:${PORT}" \
    "$(target console-qt lmix-show)" >"$WORK/show.log" 2>&1 &
PIDS+=($!)
show_bound() { tail -n +"$((before + 1))" "$WORK/portal.out" | grep -q '"trigger":"F9"'; }
wait_for show_bound || fail "lmix Show did not bind by default: $(tail -n +"$((before + 1))" "$WORK/portal.out")"
grep -q "settings taken over" "$WORK/show.log" || fail "show did not take over console.conf"
pass "lmix Show binds the shortcut keys by default, also with an old console.conf"
echo "global keys check passed"
