#!/usr/bin/env bash
# Global shortcuts (AUT.12) without a desktop: a private session bus with
# a stand-in GlobalShortcuts portal (workspace/tools/fake-portal), lmix on
# Xvfb and lmix-core without engine. Checks that lmix binds the show's
# shortcut keys and runs a shortcut when the portal reports a press. lmix
# binds unless switched off (the show's default of 18.10.2026, kept for
# the one app on 23.10.), also after taking over the settings of the two
# programs before it.
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
# exec: started with &, the PID is lmix's own, so kill reaches it.
lmix() { XDG_CONFIG_HOME="$1" DISPLAY="$DISPLAY_NUM" QT_QPA_PLATFORM=xcb LMIX_CORE="http://127.0.0.1:${PORT}" exec "$(target console-qt lmix)"; }

# Switched off: binds nothing.
mkdir -p "$WORK/off/lmix"
printf '[keys]\nglobal=false\n' >"$WORK/off/lmix/lmix.conf"
lmix "$WORK/off" >"$WORK/lmix-off.log" 2>&1 &
first=$!
lmix_up() { [[ "$(window '^lmix')" -ge 1 ]]; }
wait_for lmix_up || fail "lmix did not open"
sleep 3
[[ ! -s "$WORK/portal.out" ]] || fail "lmix bound keys although switched off: $(cat "$WORK/portal.out")"
kill "$first"
wait "$first" 2>/dev/null || true
pass "lmix binds no global keys when switched off"

# By default, with the settings of the two programs before it: the
# mixer's default "global=false" must not switch it off.
mkdir -p "$WORK/config/lmix"
printf '[keys]\nglobal=false\n[view]\nrows=3\n' >"$WORK/config/lmix/mixer.conf"
printf '[view]\ndarkRoom=false\n' >"$WORK/config/lmix/show.conf"
lmix "$WORK/config" >"$WORK/lmix.log" 2>&1 &
first_window=$!
PIDS+=($first_window)

bound() { grep -q '"trigger":"CTRL+SHIFT+F1"' "$WORK/portal.out"; }
wait_for bound || fail "lmix did not bind the shortcut key: $(cat "$WORK/portal.out")"
grep -q '"description":"Ball on air"' "$WORK/portal.out" || fail "binding without the label"
grep -q "rows=3" "$WORK/config/lmix/lmix.conf" || fail "the mixer's settings were not taken over"
! grep -q "global=false" "$WORK/config/lmix/lmix.conf" || fail "took over the mixer's default global=false"
pass "lmix binds the show's shortcut keys by default, also with the old mixer.conf"

id="$(curl -sf "$CORE/state" | jq -r '.shortcuts[0].id')"
ball="$(curl -sf "$CORE/state" | jq -r '.inputs[] | select(.name=="Ball") | .id')"
echo "press $id" >&5
on_air() { [[ "$(curl -sf "$CORE/state" | jq -r '.program')" == "$ball" ]]; }
wait_for on_air || fail "a press reported by the portal did not run the shortcut"
pass "a press reported by the portal runs the shortcut"

# A second window on the same computer leaves the keys to the first:
# with both bound, a press would run its shortcut twice.
before="$(wc -l <"$WORK/portal.out")"
lmix "$WORK/config" >"$WORK/lmix-second.log" 2>&1 &
second=$!
second_up() { [[ "$(window '^lmix')" -ge 2 ]]; }
wait_for second_up || fail "the second lmix window did not open"
sleep 3
[[ "$(wc -l <"$WORK/portal.out")" == "$before" ]] || fail "a second window bound the keys too: $(tail -n +"$((before + 1))" "$WORK/portal.out")"
pass "a second lmix window leaves the keys to the first"

# The first window closes: the second takes the keys over by itself.
kill "$first_window"
wait "$first_window" 2>/dev/null || true
PIDS+=($second)
taken_over() { tail -n +"$((before + 1))" "$WORK/portal.out" | grep -q '"trigger":"CTRL+SHIFT+F1"'; }
for _ in $(seq 1 6); do taken_over && break; sleep 2; done
taken_over || fail "the second window did not take the keys over after the first closed"
pass "the second window takes the keys over when the first closes"

# A changed key is bound again.
act "{\"action\":\"set_shortcut\",\"target\":1,\"shortcut\":{\"label\":\"Ball on air\",\"key\":\"F9\",\"actions\":[{\"action\":\"cut_direct\",\"input\":\"Ball\"}]}}"
rebound() { grep -q '"trigger":"F9"' "$WORK/portal.out"; }
wait_for rebound || fail "changed key not bound again"
pass "a changed key is bound again"
echo "global keys check passed"
