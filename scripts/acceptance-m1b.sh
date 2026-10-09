#!/usr/bin/env bash
# M1b acceptance: N x 1080p30 H.264 file inputs (hardware decode) ->
# vision mixer (backend picked for the GPU) -> PGM + multiview, both
# hardware-encoded; checks fps on every input and output, PGM age,
# takes, WS state. Exit code 0 = all budgets held.
#
#   scripts/acceptance-m1b.sh [INPUTS=16] [SECONDS=600]
#
# Starts its own release backend on LMIX_TEST_PORT (default 8199) and
# stops it afterwards. The test clip is generated once into
# ${XDG_CACHE_HOME:-~/.cache}/lmix/.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INPUTS="${1:-16}"
SECONDS_LOAD="${2:-600}"
PORT="${LMIX_TEST_PORT:-8199}"
BASE_URL="http://127.0.0.1:${PORT}"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/lmix"
CLIP="$CACHE/ball-1080p30-h264.mp4"
CLIP_FRAMES=1800
HEALTH_TIMEOUT_S=60

# Hardware H.264 encoder for the clip; software only if none exists.
pick_encoder() {
    for enc in nvh264enc vah264enc vah264lpenc x264enc openh264enc; do
        if gst-inspect-1.0 "$enc" >/dev/null 2>&1; then
            echo "$enc"
            return
        fi
    done
    echo "no H.264 encoder found" >&2
    exit 1
}

make_clip() {
    [[ -f "$CLIP" ]] && return
    mkdir -p "$CACHE"
    local enc
    enc="$(pick_encoder)"
    echo "Generating test clip with $enc: $CLIP"
    gst-launch-1.0 -q videotestsrc num-buffers="$CLIP_FRAMES" pattern=ball \
        ! video/x-raw,width=1920,height=1080,framerate=30/1 \
        ! timeoverlay font-desc="Sans 60" ! "$enc" ! h264parse ! mp4mux \
        ! filesink location="$CLIP"
}

BACKEND_PID=""
cleanup() {
    if [[ -n "$BACKEND_PID" ]] && kill -0 "$BACKEND_PID" 2>/dev/null; then
        kill "$BACKEND_PID" 2>/dev/null || true
        wait "$BACKEND_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

echo "Building release binaries ..."
(cd "$ROOT/workspace/strom" && cargo build --release --features no-gui)
(cd "$ROOT/workspace/selftest" && cargo build --release)
make_clip

echo "Starting backend on ${BASE_URL} ..."
"$ROOT/workspace/strom/target/release/strom" --host 127.0.0.1 --port "$PORT" \
    --no-auto-restart > "$CACHE/acceptance-backend.log" 2>&1 &
BACKEND_PID=$!
for _ in $(seq 1 "$HEALTH_TIMEOUT_S"); do
    curl -sf "${BASE_URL}/health" >/dev/null && break
    sleep 1
done

files=()
for _ in $(seq 1 "$INPUTS"); do
    files+=(--file "$CLIP")
done

"$ROOT/workspace/selftest/target/release/lmix-selftest" --server "$BASE_URL" \
    --patterns 0 "${files[@]}" \
    --encode --secs "$SECONDS_LOAD"
