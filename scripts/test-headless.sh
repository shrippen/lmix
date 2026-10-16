#!/usr/bin/env bash
# lmix-core without GUI (OPS.1): show loaded at start, text protocol over
# TCP (API.5), OSC (AUT.13), MIDI from a FIFO (AUT.4), /metrics (OPS.3),
# clean stop on SIGTERM. Null engine, no strom needed.
#
#   scripts/test-headless.sh
#   KEEP_WORK=1 scripts/test-headless.sh   # keep logs
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT=8898
TCP_PORT=8899
OSC_PORT=8900
CORE="http://127.0.0.1:${PORT}"
WORK="$(mktemp -d)"
mkfifo "$WORK/midi"
# Held open read-write: the reader never sees an end, writes never block.
exec 4<>"$WORK/midi"
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
    "$CORE_BIN" --engine none --listen "127.0.0.1:${PORT}" --tcp "127.0.0.1:${TCP_PORT}" --osc "127.0.0.1:${OSC_PORT}" --midi "$WORK/midi" \
        --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" "$@" >>"$WORK/core.log" 2>&1 &
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

# OSC: subscribe, get tally; program input 1 by number, tally follows.
osc="$(python3 -I - "$OSC_PORT" <<'PY'
import socket, struct, sys
def pad(b): return b + b"\0" * (4 - len(b) % 4)
def msg(addr, *args):
    tags = "," + "".join("i" if isinstance(a, int) else "s" for a in args)
    out = pad(addr.encode()) + pad(tags.encode())
    for a in args:
        out += struct.pack(">i", a) if isinstance(a, int) else pad(a.encode())
    return out
def parse(data):
    def string(at):
        end = data.index(b"\0", at)
        return data[at:end].decode(), (end + 4) & ~3
    addr, at = string(0)
    tags, at = string(at)
    args = []
    for t in tags[1:]:
        if t == "s":
            v, at = string(at)
        elif t == "i":
            v = struct.unpack(">i", data[at:at + 4])[0]; at += 4
        elif t == "f":
            v = round(struct.unpack(">f", data[at:at + 4])[0], 3); at += 4
        args.append(v)
    return addr, args
def until(sock, *addresses):
    """Arguments of the next message to each address, in any order."""
    got = {}
    while len(got) < len(addresses):
        addr, args = parse(sock.recv(65536))
        if addr in addresses:
            got[addr] = args
    return got[addresses[0]] if len(addresses) == 1 else [got[a] for a in addresses]
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.settimeout(5)
dest = ("127.0.0.1", int(sys.argv[1]))
s.sendto(msg("/lmix/subscribe"), dest)
first = until(s, "/lmix/tally")[0]
s.sendto(msg("/lmix/program", 1), dest)
tally, program = until(s, "/lmix/tally", "/lmix/program")
second = tally[0]
s.sendto(msg("/lmix/preview", 9), dest)
err = until(s, "/lmix/error")
s.sendto(msg("/lmix/state"), dest)
state = until(s, "/lmix/state")[0]
print(first, second, "/".join(map(str, program)), bool(err), '"name":"Bars"' in state)
PY
)" || fail "OSC client failed"
[[ "$osc" == "01 10 Bars/1 True True" ]] || fail "OSC: got '$osc'"
pass "OSC: tally and program on subscribe and change, action by address, error reply, full state"

# MIDI: note 36 runs a shortcut, controller 7 moves the T-bar; clients
# see each message (to learn bindings).
state_is() { [[ "$(curl -sf "$CORE/api/state" | jq -r "$1")" == true ]]; }
wait_for() { for _ in $(seq 1 20); do "$@" && return 0; sleep 0.25; done; return 1; }
act '{"action":"add_shortcut","shortcut":{"label":"Ball","midi":{"kind":"note","number":36},"actions":[{"action":"cut_direct","input":"Ball"}]}}'
act '{"action":"set_midi_tbar","midi":{"kind":"cc","number":7}}'
exec 3<>"/dev/tcp/127.0.0.1/${TCP_PORT}"
read_line >/dev/null
printf 'SUB MIDI\n' >&3
[[ "$(read_line)" == OK ]] || fail "SUB MIDI refused"
printf '\x99\x24\x64\x89\x24\x00' >&4   # note 36 on and off, channel 10
[[ "$(read_line)" == 'MIDI {"kind":"note","channel":10,"number":36,"value":100}' ]] || fail "MIDI message not passed on"
ball_id="$(curl -sf "$CORE/api/state" | jq -r '.inputs[] | select(.name=="Ball") | .id')"
wait_for state_is ".program == \"$ball_id\"" || fail "MIDI note did not run the shortcut"
act '{"action":"preview_input","input":"Bars"}'
printf '\xB0\x07\x40' >&4                   # controller 7 = 64
wait_for state_is '.t_bar > 0.49 and .t_bar < 0.52' || fail "MIDI controller did not move the T-bar"
exec 3<&-
pass "MIDI: note runs a shortcut, controller moves the T-bar, messages passed on"

metrics="$(curl -sf "$CORE/metrics")"
grep -q '^lmix_input_tally{input="2",name="Ball"} 1$' <<<"$metrics" || fail "metrics: tally of Ball"
grep -q '^lmix_uptime_seconds [0-9]' <<<"$metrics" || fail "metrics: uptime"
grep -q '^# TYPE lmix_input_online gauge$' <<<"$metrics" || fail "metrics: input health"
pass "metrics: Prometheus text with inputs and tally"

# OVL.8 over OSC: all overlays off, the last input of a channel again.
osc_send() {
    python3 -I - "$OSC_PORT" "$@" <<'PY'
import socket, sys
def pad(b): return b + b"\0" * (4 - len(b) % 4)
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.sendto(pad(sys.argv[2].encode()) + pad(b","), ("127.0.0.1", int(sys.argv[1])))
PY
}
act '{"action":"overlay_in","channel":1,"input":"Bars","duration_ms":0}'
act '{"action":"overlay_in","channel":2,"input":"Ball","duration_ms":0}'
osc_send /lmix/overlay/out
wait_for state_is '.overlays == [null,null,null,null]' || fail "OSC: overlays not all off"
osc_send /lmix/overlay/2/last
wait_for state_is ".overlays[1] == \"$ball_id\" and .overlays[0] == null" || fail "OSC: last input not back"
pass "OSC: all overlays off, last input of a channel again"
echo "headless check passed"
