#!/usr/bin/env bash
# Where does lmix's GUI thread spend its time during a fader drag? Like
# measure-gui-frames.sh (engine, lmix-core, lmix on Xvfb, set_volume on
# input 1 at RATE per second), but lmix is built with the feature
# qml-profile and qmlprofiler records the drag; scripts/qml-profile.py
# lists bindings, functions and signal handlers by time and count.
# A tool for finding work, not a check; stays out of CI.
#
#   scripts/profile-gui.sh                        # 8 inputs, CUDA mixer
#   INPUTS=31 MIXER=cpu PHASE=10 scripts/profile-gui.sh
#   PAGE=audio INPUTS=31 …                        # another page (default mix)
#   KEEP_WORK=1 …                                 # keep the trace (.qtd, opens in Qt Creator)
#
# Needs Xvfb, bc, Python 3, qmlprofiler (Qt 6), Qt 6 with Kirigami and gst-plugin-qml6.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STROM_PORT="${STROM_PORT:-8792}"
CORE_PORT="${CORE_PORT:-8892}"
PROFILE_PORT="${PROFILE_PORT:-3768}"
DISPLAY_NO="${DISPLAY_NO:-:94}"
MIXER="${MIXER:-cuda}"
INPUTS="${INPUTS:-8}"
PAGE="${PAGE:-mix}"
RATE="${RATE:-40}"
PHASE="${PHASE:-10}"
CORE="http://127.0.0.1:${CORE_PORT}/api"
WORK="$(mktemp -d)"
LIVE_DIR="${XDG_RUNTIME_DIR:-/tmp}/lmix-profile${DISPLAY_NO#:}"
# A target of its own: the feature changes the build.
PROFILE_TARGET="${PROFILE_TARGET:-$HOME/.cache/claude/lmix-profile-target}"
PIDS=()
# Another run (or session) on the same display would get this window's
# keys and pictures, and its numbers ours.
if [[ -e "/tmp/.X11-unix/X${DISPLAY_NO#:}" ]]; then
    echo "display $DISPLAY_NO is taken; pick another with DISPLAY_NO (and the ports)" >&2
    exit 2
fi

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

profiler=""
for candidate in qmlprofiler6 qmlprofiler-qt6 /usr/lib/qt6/bin/qmlprofiler /usr/lib64/qt6/bin/qmlprofiler qmlprofiler; do
    if command -v "$candidate" >/dev/null 2>&1; then profiler="$candidate"; break; fi
done
[[ -n "$profiler" ]] || { echo "qmlprofiler not found (Qt 6 declarative tools)" >&2; exit 2; }

STROM_BIN="$("$ROOT/scripts/fetch-strom.sh")"
(cd "$ROOT/workspace/lmix-core" && cargo build --release -q)
(cd "$ROOT/workspace/console-qt" && CARGO_TARGET_DIR="$PROFILE_TARGET" cargo build --release -q --features qml-profile)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
mkdir -p "$LIVE_DIR" "$WORK/config"

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

Xvfb "$DISPLAY_NO" -screen 0 1920x1080x24 >/dev/null 2>&1 &
PIDS+=($!)
sleep 1
XDG_CONFIG_HOME="$WORK/config" DISPLAY="$DISPLAY_NO" QT_QPA_PLATFORM=xcb LMIX_QML_PROFILE_PORT="$PROFILE_PORT" \
    LMIX_CORE="http://127.0.0.1:${CORE_PORT}" \
    "$PROFILE_TARGET/release/lmix" --page "$PAGE" --window maximized >"$WORK/lmix.log" 2>&1 &
PIDS+=($!)
# Pictures attach. Then the profiler, steered through a FIFO: it records
# only the drag.
sleep 15
mkfifo "$WORK/ctl"
"$profiler" --interactive --record off --attach 127.0.0.1 -p "$PROFILE_PORT" -o "$WORK/trace.qtd" \
    <"$WORK/ctl" >"$WORK/profiler.log" 2>&1 &
PIDS+=($!)
exec 3>"$WORK/ctl"
sleep 2

echo r >&3
end="$(echo "$(now) + $PHASE" | bc)"
volume=100 step=-2 sent=0
while (($(echo "$(now) < $end" | bc))); do
    act "{\"action\":\"set_volume\",\"input\":1,\"volume\":$volume}" &
    volume=$((volume + step))
    if ((volume <= 0 || volume >= 100)); then step=$((-step)); fi
    sent=$((sent + 1))
    sleep "$(echo "1 / $RATE" | bc -l)"
done
echo r >&3
sleep 2
echo o >&3
for _ in $(seq 1 60); do [[ -s "$WORK/trace.qtd" ]] && break; sleep 0.5; done
[[ -s "$WORK/trace.qtd" ]] || { echo "no trace (logs: $WORK)" >&2; KEEP_WORK=1; exit 1; }
sleep 1
echo q >&3
echo y >&3
exec 3>&-
echo "$INPUTS inputs, $sent set_volume in $PHASE s"
python3 -I "$ROOT/scripts/qml-profile.py" "$WORK/trace.qtd" "$PHASE"
