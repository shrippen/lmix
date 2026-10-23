#!/usr/bin/env bash
# Lip sync of recordings: a clip that flashes and beeps at the same time
# (once a second) plays as the only input, on program; program and the
# input itself (ISO) are recorded, scripts/av-offset.py measures the
# sound against the picture in each file. Real hardware, stays out of CI.
# Measures, does not judge: the AAC encoder delay alone is 21 ms
# (DECISIONS 23.10. „Ton in Aufnahmen hinter dem Bild“).
#
#   scripts/measure-av.sh             # CUDA mixer, 25 s, 3 runs
#   MIXER=cpu RUNS=5 SECS=40 scripts/measure-av.sh
#   KEEP_WORK=1 …                     # keep logs and recordings
#   STROM_BIN=…/strom CORE_BIN=…/lmix-core …   # skip the builds
#
# Needs ffmpeg, jq and Python 3 with numpy.
set -euo pipefail

MIXER="${MIXER:-cuda}"
RUNS="${RUNS:-3}"
SECS="${SECS:-25}"
STROM_PORT="${STROM_PORT:-8711}"
CORE_PORT="${CORE_PORT:-8712}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="http://127.0.0.1:${CORE_PORT}/api"
mkdir -p "$HOME/.cache/lmix-av"
WORK="$(mktemp -d -p "$HOME/.cache/lmix-av")"
PIDS=()

cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
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

python3 -c 'import numpy' 2>/dev/null || { echo "needs numpy for Python 3" >&2; exit 1; }
for tool in ffmpeg jq; do
    command -v "$tool" >/dev/null || { echo "needs $tool" >&2; exit 1; }
done

# One white frame and a 1 kHz beep of one frame's length, every second.
CLIP="$WORK/flashbeep.mp4"
ffmpeg -v error -y \
    -f lavfi -i "color=black:s=1280x720:r=30:d=$((SECS + 10)),geq=lum='if(lt(mod(N,30),1),235,16)':cb=128:cr=128" \
    -f lavfi -i "aevalsrc='if(lt(mod(t,1),1/30),0.8*sin(2*PI*1000*t),0)':s=48000:d=$((SECS + 10))" \
    -c:v libx264 -preset veryfast -g 30 -pix_fmt yuv420p -c:a aac -b:a 128k -shortest "$CLIP"
echo "clip: $("$ROOT/scripts/av-offset.py" "$CLIP" | cut -d' ' -f1) ms (reference)"

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
if [[ -z "${CORE_BIN:-}" ]]; then
    (cd "$ROOT/workspace/lmix-core" && cargo build --release -q)
    CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
fi

for run in $(seq 1 "$RUNS"); do
    dir="$WORK/run$run"
    mkdir -p "$dir/pgm" "$dir/iso"
    (cd "$dir" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
        --data-dir "$dir/strom-data" >"$dir/strom.log" 2>&1) &
    PIDS=($!)
    wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
    "$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" \
        --mixer "$MIXER" --slots 1 --autosave "$dir/autosave.lmix" --shortcuts "$dir/shortcuts.json" \
        --layout-templates "$dir/layout-templates.json" --live-dir "$dir/live" >"$dir/core.log" 2>&1 &
    PIDS+=($!)
    wait_up "$CORE/state" lmix-core

    act "{\"action\":\"add_input\",\"source\":{\"kind\":\"file\",\"path\":\"$CLIP\"},\"name\":\"Clip\"}"
    act '{"action":"cut_direct","input":1}'
    sleep 4
    id="$(curl -sf "$CORE/state" | jq -r '.inputs[0].id')"
    act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"record\",\"folder\":\"$dir/pgm\",\"container\":\"mkv\"},\"name\":\"Pgm\"}"
    act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"record\",\"folder\":\"$dir/iso\",\"container\":\"mkv\"},\"source\":{\"from\":\"input\",\"input\":\"$id\"},\"name\":\"Iso\"}"
    act '{"action":"start_output","output":"Pgm"}'
    act '{"action":"start_output","output":"Iso"}'
    sleep "$SECS"
    act '{"action":"stop_output","output":"Pgm"}'
    act '{"action":"stop_output","output":"Iso"}'
    sleep 3

    for feed in pgm iso; do
        if ! out="$("$ROOT/scripts/av-offset.py" "$dir/$feed"/*.mkv)"; then
            echo "run $run $feed: not measured" >&2
            KEEP_WORK=1
            continue
        fi
        read -r med min max n <<<"$out"
        echo "run $run $feed: sound ${med} ms after picture (min ${min}, max ${max}, ${n} flashes)"
    done
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    PIDS=()
done
