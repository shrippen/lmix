#!/usr/bin/env bash
# Does the window keep drawing while a fader moves, and program too?
# Engine, lmix-core and lmix (Mix page, Xvfb) with INPUTS pattern inputs,
# the ball on program and a recording of it. Idle, then a fader drag
# (set_volume on input 1 at RATE per second straight to lmix-core: more
# than the window's own fader sends, which passes at most one per 33 ms),
# then idle again. scripts/gui-frames.py measures the frames lmix drew
# (Qt's render loop timing), the CPU of its GUI thread and frozen
# pictures in program, and exits 1 over budget (budgets in its header).
# A time budget: runs on a real machine, not in CI (agent.md). The GUI
# thread's work does not depend on the GPU; Xvfb draws in software.
#
#   scripts/measure-gui-frames.sh                 # CUDA mixer, 10 s phases
#   MIXER=cpu PHASE=20 RATE=60 scripts/measure-gui-frames.sh
#   INPUTS=31 …                                   # many inputs (default 4)
#   PAGE=audio INPUTS=40 …                        # another page (default mix)
#   LMIX_BIN=…/lmix …                             # another build of lmix (a baseline)
#   KEEP_WORK=1 …                                 # keep logs and the recording
#   GAP_P95_MS=40 …                               # a tighter budget
#
# Needs Xvfb, ffmpeg, jq, bc, Python 3, Qt 6 with Kirigami and gst-plugin-qml6.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STROM_PORT="${STROM_PORT:-8791}"
CORE_PORT="${CORE_PORT:-8891}"
DISPLAY_NO="${DISPLAY_NO:-:93}"
MIXER="${MIXER:-cuda}"
RATE="${RATE:-40}"
INPUTS="${INPUTS:-4}"
PAGE="${PAGE:-mix}"
PHASE="${PHASE:-10}"
CORE="http://127.0.0.1:${CORE_PORT}/api"
WORK="$(mktemp -d)"
# Unix socket paths must stay short (107 bytes).
LIVE_DIR="${XDG_RUNTIME_DIR:-/tmp}/lmix-frames${DISPLAY_NO#:}"
PIDS=()

cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    rm -rf "$LIVE_DIR"
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT

act() { curl -sf --max-time 5 -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null; }
wait_up() {
    for _ in $(seq 1 120); do curl -sf --max-time 2 "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    echo "$2 did not come up (logs: $WORK)" >&2
    KEEP_WORK=1
    exit 1
}
now() { date +%s.%N; }
# CPU of each thread of lmix over the next `seconds`.
threads() {
    local pid="$1" seconds="$2" label="$3"
    python3 -I - "$pid" "$seconds" "$label" >>"$WORK/threads" <<'EOF' &
import os, sys, time
pid, secs, label = int(sys.argv[1]), float(sys.argv[2]), sys.argv[3]
hz = os.sysconf("SC_CLK_TCK")
def snap():
    out = {}
    for tid in os.listdir(f"/proc/{pid}/task"):
        try:
            s = open(f"/proc/{pid}/task/{tid}/stat").read()
        except OSError:
            continue
        f = s[s.rindex(")") + 2:].split()
        out[tid] = (s[s.index("(") + 1:s.rindex(")")], int(f[11]) + int(f[12]))
    return out
a = snap(); time.sleep(secs); b = snap()
rows = sorted(((b[t][1] - a.get(t, (0, 0))[1]) / hz / secs * 100, b[t][0], t) for t in b)
# The busiest five, and the GUI thread (the main one) always.
shown = list(reversed(rows[-5:])) + [r for r in rows[:-5] if r[2] == str(pid)]
print(label, " ".join(f"{n}({t})={p:.0f}%" for p, n, t in shown), flush=True)
EOF
}

STROM_BIN="$("$ROOT/scripts/fetch-strom.sh")"
(cd "$ROOT/workspace/lmix-core" && cargo build --release -q)
(cd "$ROOT/workspace/console-qt" && cargo build --release -q)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
LMIX_BIN="${LMIX_BIN:-${CARGO_TARGET_DIR:-$ROOT/workspace/console-qt/target}/release/lmix}"
mkdir -p "$LIVE_DIR" "$WORK/rec" "$WORK/config"

(cd "$WORK" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
    --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
"$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" --mixer "$MIXER" \
    --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" \
    --layout-templates "$WORK/layout-templates.json" --live-dir "$LIVE_DIR" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core
patterns=(ball bars snow checkers)
for i in $(seq 1 "$INPUTS"); do
    pattern="${patterns[$(((i - 1) % ${#patterns[@]}))]}"
    act "{\"action\":\"add_input\",\"source\":{\"kind\":\"pattern\",\"pattern\":\"$pattern\"},\"name\":\"${pattern^} $i\"}"
done
act '{"action":"cut_direct","input":1}'
act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"record\",\"folder\":\"$WORK/rec\",\"container\":\"mkv\"},\"name\":\"Rec\"}"
act '{"action":"start_output","output":"Rec"}'
# The recording's first frame is about when its file gets data. Right
# after adding 31 inputs, while they still start, that came 6 s after
# the output ran (0.3 s once they ran, 29.10.2026), and frames placed by
# the start went 6 s too early.
for _ in $(seq 1 120); do
    compgen -G "$WORK/rec/*" >/dev/null && [[ -s "$(ls "$WORK"/rec/* | head -1)" ]] && break
    sleep 0.1
done
echo "rec $(now)" >"$WORK/rec-start"

Xvfb "$DISPLAY_NO" -screen 0 1920x1080x24 >/dev/null 2>&1 &
PIDS+=($!)
sleep 1
XDG_CONFIG_HOME="$WORK/config" DISPLAY="$DISPLAY_NO" QT_QPA_PLATFORM=xcb QT_FORCE_STDERR_LOGGING=1 \
    QT_LOGGING_RULES="qt.scenegraph.time.renderloop=true" QT_MESSAGE_PATTERN='%{time process} %{category} %{message}' \
    RUST_LOG=info,lmix=debug LMIX_CORE="http://127.0.0.1:${CORE_PORT}" \
    "$LMIX_BIN" --page "$PAGE" --window maximized >"$WORK/lmix.log" 2>&1 &
LMIX_PID=$!
PIDS+=("$LMIX_PID")
# Pictures attach, the recording runs.
sleep 15

echo "idle1 $(now)" >"$WORK/phases"
threads "$LMIX_PID" "$PHASE" idle1
sleep "$PHASE"
echo "fader $(now)" >>"$WORK/phases"
threads "$LMIX_PID" "$PHASE" fader
end="$(echo "$(now) + $PHASE" | bc)"
volume=100 step=-2 sent=0
while (($(echo "$(now) < $end" | bc))); do
    act "{\"action\":\"set_volume\",\"input\":1,\"volume\":$volume}" &
    volume=$((volume + step))
    if ((volume <= 0 || volume >= 100)); then step=$((-step)); fi
    sent=$((sent + 1))
    sleep "$(echo "1 / $RATE" | bc -l)"
done
echo "idle2 $(now)" >>"$WORK/phases"
threads "$LMIX_PID" "$PHASE" idle2
sleep "$PHASE"
echo "end $(now)" >>"$WORK/phases"
act '{"action":"stop_output","output":"Rec"}' || true
sleep 2
echo "fader: $sent set_volume in $PHASE s"
status=0
python3 -I "$ROOT/scripts/gui-frames.py" "$WORK" || status=$?
((status == 0)) || KEEP_WORK=1
exit "$status"
