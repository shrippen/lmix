#!/usr/bin/env bash
# Channel delay of an input (AUD.6), measured on program: a clip that
# flashes and beeps at the same time (scripts/measure-av.sh) plays on
# program, which is recorded without delay and with DELAY_MS;
# scripts/av-offset.py measures the sound against the picture in each.
# The sound must come the delay later. No GPU needed; stays out of CI
# (records and decodes, a minute on a fast machine).
#
#   scripts/test-channel-delay.sh
#   DELAY_MS=300 SECS=8 MIXER=cpu …   # DELAY_MS under 500
#   STROM_BIN=…/strom CORE_BIN=…/lmix-core …   # skip the builds
#   KEEP_WORK=1 …                              # keep logs and recordings
#
# Needs ffmpeg, jq and Python 3 with numpy.
set -euo pipefail

MIXER="${MIXER:-cpu}"
# Under half the clip's second between beeps: av-offset.py pairs each
# beep with the nearest flash.
DELAY_MS="${DELAY_MS:-300}"
SECS="${SECS:-8}"
STROM_PORT="${STROM_PORT:-8731}"
CORE_PORT="${CORE_PORT:-8732}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="http://127.0.0.1:${CORE_PORT}/api"
# The measured shift may differ from the delay by this much (a frame of
# the clip is 33 ms).
SLACK_MS=40
mkdir -p "$HOME/.cache/lmix-channel-delay"
WORK="$(mktemp -d -p "$HOME/.cache/lmix-channel-delay")"
PIDS=()

cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT
fail() { echo "FAIL: $*" >&2; KEEP_WORK=1; exit 1; }
act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null || fail "action $1"; }
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    fail "$2 did not come up (logs: $WORK)"
}

python3 -c 'import numpy' 2>/dev/null || { echo "needs numpy for Python 3" >&2; exit 2; }
for tool in ffmpeg jq; do
    command -v "$tool" >/dev/null || { echo "needs $tool" >&2; exit 2; }
done

# One white frame and a 1 kHz beep of one frame's length, every second,
# long enough for both recordings.
CLIP="$WORK/flashbeep.mp4"
LENGTH=$((3 * SECS + 30))
ffmpeg -v error -y \
    -f lavfi -i "color=black:s=1280x720:r=30:d=$LENGTH,geq=lum='if(lt(mod(N,30),1),235,16)':cb=128:cr=128" \
    -f lavfi -i "aevalsrc='if(lt(mod(t,1),1/30),0.8*sin(2*PI*1000*t),0)':s=48000:d=$LENGTH" \
    -c:v libx264 -preset veryfast -g 30 -pix_fmt yuv420p -c:a aac -b:a 128k -shortest "$CLIP"

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
if [[ -z "${CORE_BIN:-}" ]]; then
    (cd "$ROOT/workspace/lmix-core" && cargo build --release -q)
    CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
fi

(cd "$WORK" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
    --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
"$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" \
    --mixer "$MIXER" --slots 1 --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" \
    --layout-templates "$WORK/layout-templates.json" --live-dir "$WORK/live" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core

act "{\"action\":\"add_input\",\"source\":{\"kind\":\"file\",\"path\":\"$CLIP\"},\"name\":\"Clip\"}"
act '{"action":"cut_direct","input":1}'
sleep 4

# Sound after picture on program with delay $1, in ms (median).
offset() {
    local dir="$WORK/delay$1"
    mkdir -p "$dir"
    act "{\"action\":\"set_channel_strip\",\"input\":1,\"strip\":{\"delay_ms\":$1}}"
    sleep 2
    act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"record\",\"folder\":\"$dir\",\"container\":\"mkv\"},\"name\":\"Pgm\"}"
    act '{"action":"start_output","output":"Pgm"}'
    sleep "$SECS"
    act '{"action":"stop_output","output":"Pgm"}'
    sleep 3
    act '{"action":"remove_output","output":"Pgm"}'
    local out
    out="$("$ROOT/scripts/av-offset.py" "$dir"/*.mkv)" || fail "delay $1 ms: not measured"
    echo "delay $1 ms: sound $out (median min max flashes, ms after picture)" >&2
    echo "${out%% *}"
}

# offset runs in a subshell: its failure is reported here, so the logs stay.
plain="$(offset 0)" || fail "no measurement without delay (logs: $WORK)"
delayed="$(offset "$DELAY_MS")" || fail "no measurement with $DELAY_MS ms (logs: $WORK)"
shift_ms="$(awk -v a="$plain" -v b="$delayed" 'BEGIN { printf "%.0f", b - a }')"
echo "delay $DELAY_MS ms moved the sound $shift_ms ms"
awk -v s="$shift_ms" -v d="$DELAY_MS" -v t="$SLACK_MS" 'BEGIN { x = s - d; if (x < 0) x = -x; exit !(x <= t) }' \
    || fail "delay $DELAY_MS ms moved the sound $shift_ms ms"
echo "channel delay check passed"
