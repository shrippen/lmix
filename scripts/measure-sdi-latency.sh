#!/usr/bin/env bash
# Loop latency of a card (SRC.1, OUT.4, H.7): a clip that flashes once a
# second plays on program, program goes out on SDI_OUT and comes back on
# SDI_IN through a cable. The multiview, which shows program and the
# card input side by side, is recorded; scripts/sdi-latency.py measures
# how much later the card input's tile flashes. That is the way out of
# the mixer, through both card connectors and back into the mixer.
# Needs the card: stays out of CI. Measures, does not judge.
#
#   scripts/measure-sdi-latency.sh
#   SDI_OUT=1 SDI_IN=2 MODE=1080p25 MIXER=cuda|cpu RUNS=3 SECS=15 …
#   STROM_BIN=…/strom CORE_BIN=…/lmix-core …   # skip the builds
#   DECKLINK_PLUGIN=system                      # decklink plugin without lmix's patch
#   KEEP_WORK=1 …                              # keep logs and recordings
#
# Needs ffmpeg, jq and Python 3 with numpy.
set -euo pipefail

SDI_OUT="${SDI_OUT:-1}"
SDI_IN="${SDI_IN:-2}"
MODE="${MODE:-1080p25}"
MIXER="${MIXER:-cuda}"
RUNS="${RUNS:-3}"
SECS="${SECS:-15}"
STROM_PORT="${STROM_PORT:-8761}"
CORE_PORT="${CORE_PORT:-8762}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="http://127.0.0.1:${CORE_PORT}/api"
# The card input locks to the signal and its tile settles.
SETTLE=12
mkdir -p "$HOME/.cache/lmix-sdi-latency"
WORK="$(mktemp -d -p "$HOME/.cache/lmix-sdi-latency")"
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
# A card connector takes one user; an engine left over holds it.
pgrep -f '^[^ ]*/strom --host' >/dev/null && { echo "another strom is running; it may hold the card" >&2; exit 2; }

# White for three frames once a second: survives any rate change on the way.
CLIP="$WORK/flash.mp4"
LENGTH=$((RUNS * (SECS + 30) + 60))
ffmpeg -v error -y \
    -f lavfi -i "color=black:s=1280x720:r=25:d=$LENGTH,geq=lum='if(lt(mod(N,25),3),235,16)':cb=128:cr=128" \
    -c:v libx264 -preset veryfast -g 25 -pix_fmt yuv420p "$CLIP"

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
# GStreamer's decklink plugin with lmix's patch (a card output started
# again shows every frame late without it); DECKLINK_PLUGIN=system keeps
# the installed one.
if [[ "${DECKLINK_PLUGIN:-patched}" == patched ]]; then
    GST_PLUGIN_PATH="$("$ROOT/scripts/decklink-plugin.sh")${GST_PLUGIN_PATH:+:$GST_PLUGIN_PATH}"
    export GST_PLUGIN_PATH
fi
# The plugin to use, ours or the system's, is on the path by now.
gst-inspect-1.0 decklinkvideosink >/dev/null 2>&1 || { echo "needs the GStreamer decklink plugin" >&2; exit 2; }
if [[ -z "${CORE_BIN:-}" ]]; then
    (cd "$ROOT/workspace/lmix-core" && cargo build --release -q)
    CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
fi

# A fresh engine for each run: a card output started a second time in
# the same strom shows frames late (DECISIONS 27.10.).
for run in $(seq 1 "$RUNS"); do
    dir="$WORK/run$run"
    mkdir -p "$dir/mv"
    (cd "$dir" && exec env -u WAYLAND_DISPLAY GST_GL_WINDOW=x11 "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" \
        --no-auto-restart --data-dir "$dir/strom-data" >"$dir/strom.log" 2>&1) &
    PIDS=($!)
    wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
    "$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" \
        --mixer "$MIXER" --slots 2 --autosave "$dir/autosave.lmix" --shortcuts "$dir/shortcuts.json" \
        --layout-templates "$dir/layout-templates.json" >"$dir/core.log" 2>&1 &
    PIDS+=($!)
    wait_up "$CORE/state" lmix-core
    # The cards start switched off.
    act '{"action":"set_cards","on":true}'

    act "{\"action\":\"add_input\",\"source\":{\"kind\":\"file\",\"path\":\"$CLIP\"},\"name\":\"Flash\"}"
    act "{\"action\":\"add_input\",\"source\":{\"kind\":\"card\",\"device\":$SDI_IN},\"name\":\"SDI in\"}"
    act '{"action":"set_audio","input":"SDI in","on":false}'
    act '{"action":"cut_direct","input":"Flash"}'
    act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"card\",\"device\":$SDI_OUT,\"mode\":\"$MODE\"},\"name\":\"SDI out\"}"
    act '{"action":"start_output","output":"SDI out"}'
    sleep "$SETTLE"

    act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"record\",\"folder\":\"$dir/mv\",\"container\":\"mkv\",\"video_kbps\":20000},\"source\":{\"from\":\"multiview\"},\"name\":\"MV\"}"
    act '{"action":"start_output","output":"MV"}'
    sleep "$SECS"
    act '{"action":"stop_output","output":"MV"}'
    sleep 3

    # One recording: the output writes a single file while it runs.
    shopt -s nullglob
    recordings=("$dir"/mv/*.mkv)
    shopt -u nullglob
    if ((${#recordings[@]} != 1)); then
        echo "run $run: ${#recordings[@]} recordings in $dir/mv, want one" >&2
        KEEP_WORK=1
    elif out="$("$ROOT/scripts/sdi-latency.py" "${recordings[0]}")"; then
        read -r med min max n <<<"$out"
        echo "run $run ($MODE, $MIXER): card input ${med} ms behind program (min ${min}, max ${max}, ${n} cells)"
    else
        echo "run $run: not measured" >&2
        KEEP_WORK=1
    fi
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    PIDS=()
    sleep 2
done
