#!/usr/bin/env bash
# lmix-core without GUI (OPS.1): show loaded at start, text protocol over
# TCP (API.5), OSC (AUT.13), MIDI from a FIFO (AUT.4), /metrics (OPS.3),
# clean stop on SIGTERM, layout templates (LAY.5). Null engine, no strom
# needed.
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
        --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" --layout-templates "$WORK/layout-templates.json" "$@" >>"$WORK/core.log" 2>&1 &
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
grep -q '^# TYPE lmix_cue_late_over_budget_total counter$' <<<"$metrics" || fail "metrics: cue timing"
pass "metrics: Prometheus text with inputs and tally"

# OVL.8 over OSC: all overlays off, the last input of a channel again.
# An address and string arguments.
osc_send() {
    python3 -I - "$OSC_PORT" "$@" <<'PY'
import socket, sys
def pad(b): return b + b"\0" * (4 - len(b) % 4)
args = sys.argv[3:]
packet = pad(sys.argv[2].encode()) + pad(("," + "s" * len(args)).encode())
packet += b"".join(pad(a.encode()) for a in args)
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.sendto(packet, ("127.0.0.1", int(sys.argv[1])))
PY
}
act '{"action":"overlay_in","channel":1,"input":"Bars","duration_ms":0}'
act '{"action":"overlay_in","channel":2,"input":"Ball","duration_ms":0}'
osc_send /lmix/overlay/out
wait_for state_is '.overlays == [null,null,null,null]' || fail "OSC: overlays not all off"
osc_send /lmix/overlay/2/last
wait_for state_is ".overlays[1] == \"$ball_id\" and .overlays[0] == null" || fail "OSC: last input not back"
pass "OSC: all overlays off, last input of a channel again"

# SWI.8, INP.9 over OSC: quick play of preview, GO of an input.
bars_id="$(curl -sf "$CORE/api/state" | jq -r '.inputs[] | select(.name=="Bars") | .id')"
act '{"action":"t_bar","position":0}'
act '{"action":"cut_direct","input":"Ball"}'
act '{"action":"preview_input","input":"Bars"}'
osc_send /lmix/quickplay
wait_for state_is ".program == \"$bars_id\" and .preview == \"$ball_id\"" || fail "OSC: quick play did not take preview"
act '{"action":"preview_input","input":"Ball"}'
act '{"action":"set_input_press","input":"Ball","go":"cut_direct"}'
osc_send /lmix/go Ball
wait_for state_is ".program == \"$ball_id\"" || fail "OSC: GO did not run the input's GO action"
pass "OSC: quick play of preview, GO of an input"

# OVL.3 over OSC, OVL.2: an overlay prepared on preview goes on with the
# take (cut, the channel's setting) and off by itself after 1.5 s.
act '{"action":"overlay_out_all","duration_ms":0}'
act '{"action":"set_overlay","channel":3,"effect":"cut","duration_ms":0,"auto_off_ms":1500}'
osc_send /lmix/overlay/3/preview Ball
wait_for state_is ".overlay_preview[2] == \"$ball_id\" and .overlay_channels[2].auto_off_ms == 1500" \
    || fail "OSC: overlay not prepared"
act '{"action":"cut"}'
state_is ".overlays[2] == \"$ball_id\" and .overlay_preview[2] == null" || fail "prepared overlay did not go on with the take"
wait_for state_is '.overlays[2] == null' || fail "overlay did not go off by itself"
pass "OSC: overlay prepared on preview goes on with the take, off by itself"

# LAY.5: a layout as a template in its own file, loaded as a new layout.
act '{"action":"add_layout","name":"Corner","background":"Bars","boxes":[{"rect":{"x":0.7,"y":0.05,"w":0.25,"h":0.25},"input":"Ball"}]}'
act '{"action":"save_layout_template","layout":"Corner"}'
[[ "$(jq -r '.format + " " + .templates[0].name' "$WORK/layout-templates.json")" == "lmix-layouts Corner" ]] \
    || fail "layout template not in its file"
act '{"action":"load_layout_template","template":"Corner","name":"Corner 2"}'
state_is '.layouts[1].name == "Corner 2" and .layouts[1].boxes[0].rect.x == 0.7' || fail "template not loaded"
pass "layout template saved apart from the show and loaded"
# CTL.3: an OSC subscription per topic and field: name and remaining time
# of the running cue, cue events; one empty running message at the end.
act '{"action":"add_cue_list","name":"Szene","cues":[{"number":"1","name":"Pause","kind":"wait","duration_ms":800}]}'
topics="$(python3 -I - "$OSC_PORT" <<'PY'
import socket, struct, sys
def pad(b): return b + b"\0" * (4 - len(b) % 4)
def msg(addr, *args):
    out = pad(addr.encode()) + pad(("," + "s" * len(args)).encode())
    for a in args:
        out += pad(a.encode())
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
        elif t == "f":
            v = struct.unpack(">f", data[at:at + 4])[0]; at += 4
        elif t == "i":
            v = struct.unpack(">i", data[at:at + 4])[0]; at += 4
        args.append(v)
    return addr, args
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); s.settimeout(5)
dest = ("127.0.0.1", int(sys.argv[1]))
s.sendto(msg("/lmix/subscribe", "running", "name", "remaining"), dest)
s.sendto(msg("/lmix/subscribe", "cue", "what", "name"), dest)
s.sendto(msg("/lmix/cue/go"), dest)
seen = []
mixer = False
while True:
    addr, args = parse(s.recv(65536))
    mixer = mixer or addr in ("/lmix/tally", "/lmix/program")
    if addr == "/lmix/cue/event":
        seen.append("event " + "/".join(args))
    elif addr == "/lmix/cue/running":
        name, remaining = args
        seen.append("running " + (name or "-") + (" counting" if 0 < remaining <= 0.8 else ""))
        if not name:
            break
# Repeats at the event rate count once.
out = []
for line in seen:
    if not out or out[-1] != line:
        out.append(line)
print(", ".join(out), "| mixer" if mixer else "")
PY
)" || fail "OSC topic client failed"
[[ "$topics" == "event started/Pause, running Pause counting, event ended/Pause, running -"* && "$topics" != *mixer* ]] \
    || fail "OSC topics: got '$topics'"
pass "OSC: subscription per topic and field (running, cue events), no mixer messages"
echo "headless check passed"
