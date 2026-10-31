#!/usr/bin/env bash
# What do the live tiles cost? Engine, lmix-core and lmix (Mix page) with
# INPUTS pattern inputs, the window in a headless KWin on this machine's
# GPU (not Xvfb's software GL): idle for PHASE seconds, then the CPU of
# strom, lmix-core and lmix (busiest threads of each), GPU and video
# engine load, and the frames lmix drew. A tool for finding work, not a
# check; stays out of CI.
#
#   scripts/measure-tiles.sh                  # 31 inputs, CUDA mixer, 20 s
#   INPUTS=8 MIXER=gl PHASE=30 scripts/measure-tiles.sh
#   NO_WINDOW=1 …                             # engine and core only
#   PAGE=setup …                              # another page than Mix
#   STROM_BIN=workspace/strom/target/release/strom …   # a local strom
#   KEEP_WORK=1 …                             # keep logs
#   SHOT=$PWD/tiles.png dbus-run-session scripts/measure-tiles.sh
#                                             # and a screenshot (spectacle
#                                             # needs this KWin on its bus)
#
# Needs kwin_wayland (--virtual), nvidia-smi, jq, Python 3, Qt 6 with
# Kirigami and gst-plugin-qml6.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STROM_PORT="${STROM_PORT:-8793}"
CORE_PORT="${CORE_PORT:-8893}"
MIXER="${MIXER:-cuda}"
INPUTS="${INPUTS:-31}"
PHASE="${PHASE:-20}"
SOCKET="lmix-tiles-$$"
CORE="http://127.0.0.1:${CORE_PORT}/api"
WORK="$(mktemp -d)"
# Unix socket paths must stay short (107 bytes).
LIVE_DIR="${XDG_RUNTIME_DIR:-/tmp}/lmix-tiles$$"
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

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
(cd "$ROOT/workspace/lmix-core" && cargo build --release -q)
(cd "$ROOT/workspace/console-qt" && cargo build --release -q)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
LMIX_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/console-qt/target}/release/lmix"
mkdir -p "$LIVE_DIR" "$WORK/config"

(cd "$WORK" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
    --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
STROM_PID=$!
PIDS+=("$STROM_PID")
wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
"$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" --mixer "$MIXER" \
    --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" \
    --layout-templates "$WORK/layout-templates.json" --live-dir "$LIVE_DIR" >"$WORK/core.log" 2>&1 &
CORE_PID=$!
PIDS+=("$CORE_PID")
wait_up "$CORE/state" lmix-core
patterns=(ball bars snow checkers)
for i in $(seq 1 "$INPUTS"); do
    pattern="${patterns[$(((i - 1) % ${#patterns[@]}))]}"
    act "{\"action\":\"add_input\",\"source\":{\"kind\":\"pattern\",\"pattern\":\"$pattern\"},\"name\":\"${pattern^} $i\"}"
done
act '{"action":"cut_direct","input":1}'

LMIX_PID=""
if [[ -z "${NO_WINDOW:-}" ]]; then
    kwin_wayland --virtual --width 1920 --height 1080 --socket "$SOCKET" --no-lockscreen \
        --no-global-shortcuts >"$WORK/kwin.log" 2>&1 &
    PIDS+=($!)
    for _ in $(seq 1 40); do [[ -S "$XDG_RUNTIME_DIR/$SOCKET" ]] && break; sleep 0.25; done
    XDG_CONFIG_HOME="$WORK/config" WAYLAND_DISPLAY="$SOCKET" QT_QPA_PLATFORM=wayland QT_FORCE_STDERR_LOGGING=1 \
        QT_LOGGING_RULES="qt.scenegraph.time.renderloop=true" QT_MESSAGE_PATTERN='%{time process} %{category} %{message}' \
        LMIX_CORE="http://127.0.0.1:${CORE_PORT}" env ${LMIX_ENV:-} \
        "$LMIX_BIN" --page "${PAGE:-mix}" --window maximized >"$WORK/lmix.log" 2>&1 &
    LMIX_PID=$!
    PIDS+=("$LMIX_PID")
fi
# Inputs start, pictures attach.
sleep "${SETTLE:-20}"

nvidia-smi dmon -s u -d 1 -c "$PHASE" >"$WORK/gpu" 2>&1 &
DMON_PID=$!
FRAMES_BEFORE="$(grep -c "frame rendered" "$WORK/lmix.log" 2>/dev/null || true)"
python3 -I - "$PHASE" "strom=$STROM_PID" "core=$CORE_PID" ${LMIX_PID:+"lmix=$LMIX_PID"} <<'EOF'
import os, sys, time
secs = float(sys.argv[1])
procs = [a.split("=") for a in sys.argv[2:]]
hz = os.sysconf("SC_CLK_TCK")
def snap(pid):
    out = {}
    for tid in os.listdir(f"/proc/{pid}/task"):
        try:
            s = open(f"/proc/{pid}/task/{tid}/stat").read()
        except OSError:
            continue
        f = s[s.rindex(")") + 2:].split()
        out[tid] = (s[s.index("(") + 1:s.rindex(")")], int(f[11]) + int(f[12]))
    return out
a = {n: snap(p) for n, p in procs}
time.sleep(secs)
b = {n: snap(p) for n, p in procs}
total = 0
for name, _ in procs:
    rows = []
    for tid, (comm, ticks) in b[name].items():
        rows.append(((ticks - a[name].get(tid, (comm, 0))[1]) / hz / secs * 100, comm))
    # Threads of one name together (14 llvmpipe threads, 31 queues).
    by = {}
    for pct, comm in rows:
        by[comm] = by.get(comm, 0) + pct
    sum_ = sum(by.values())
    total += sum_
    top = sorted(by.items(), key=lambda kv: -kv[1])[:6]
    print(f"{name:6} {sum_:6.0f} %   " + "  ".join(f"{c}={p:.0f}" for c, p in top))
print(f"{'total':6} {total:6.0f} %")
EOF
wait "$DMON_PID"
awk '!/^#/ { sm += $2; enc += $4; dec += $5; n++ } END { if (n) printf "gpu     sm %.0f %%  enc %.0f %%  dec %.0f %%\n", sm / n, enc / n, dec / n }' "$WORK/gpu"
if [[ -n "$LMIX_PID" && -n "${SHOT:-}" ]]; then
    WAYLAND_DISPLAY="$SOCKET" timeout 20 spectacle -b -n -f -o "$SHOT" >"$WORK/spectacle.log" 2>&1 \
        && echo "shot    $SHOT" || echo "shot    failed (see $WORK/spectacle.log)"
fi
if [[ -n "$LMIX_PID" ]]; then
    # Frames the window drew over the phase (render loop timing lines).
    frames="$(grep -c 'frame rendered' "$WORK/lmix.log" || true)"
    echo "lmix    $(( (frames - FRAMES_BEFORE) / PHASE )) frames/s drawn ($(grep -m1 -o 'gui thread\|render thread' "$WORK/lmix.log" || echo '?') loop)"
fi
