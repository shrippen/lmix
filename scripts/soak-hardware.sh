#!/usr/bin/env bash
# Long run on real hardware (HARDWARE-TODO H.10, H.18, H.22): engine with
# the GPU mixer, inputs that move, program switched every 30 s, a recording
# and an SRT stream to a local receiver. Every 5 minutes it measures the
# latency source → program and reads the link health; at the end it checks
# that the latency did not grow, the outputs never failed, and that the
# recording and the received stream cover the whole run without gaps.
#
#   scripts/soak-hardware.sh                 # 60 min, CUDA mixer
#   MINUTES=10 MIXER=gl scripts/soak-hardware.sh
#   KEEP_WORK=1 …                            # keep logs, recording, stream
#
# Needs: strom (scripts/fetch-strom.sh), jq, ffprobe, gst-launch-1.0 with
# srtsrc. Report: $WORK/report.txt (path printed at the end).
set -euo pipefail

MINUTES="${MINUTES:-60}"
MIXER="${MIXER:-cuda}"
INPUTS="${INPUTS:-8}"
EVERY_S=300
STROM_PORT=8599
CORE_PORT=8699
SRT_PORT=9931
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STROM="http://127.0.0.1:${STROM_PORT}"
CORE="http://127.0.0.1:${CORE_PORT}/api"
WORK="${WORK:-$(mktemp -d)}"
REPORT="$WORK/report.txt"
PIDS=()
FAILED=0

cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    if [[ "${KEEP_WORK:-}" != 1 && "$FAILED" == 0 ]]; then
        # Keep only the report.
        find "$WORK" -mindepth 1 ! -name report.txt -delete 2>/dev/null || true
    fi
}
trap cleanup EXIT

log() { echo "$(date +%H:%M:%S) $*" | tee -a "$REPORT"; }
pass() { log "ok   $*"; }
fail() { log "FAIL $*"; FAILED=1; }
act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null; }
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    echo "$2 did not come up" >&2
    exit 1
}

STROM_BIN="$("$ROOT/scripts/fetch-strom.sh")"
(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"

log "soak: ${MINUTES} min, mixer ${MIXER}, ${INPUTS} inputs, work ${WORK}"
(cd "$WORK" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
    --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
STROM_PID=$!
PIDS+=("$STROM_PID")
wait_up "$STROM/health" strom
"$CORE_BIN" --strom "$STROM" --listen "127.0.0.1:${CORE_PORT}" --mixer "$MIXER" \
    --slots "$INPUTS" --autosave "$WORK/autosave.lmix" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core

patterns=(ball bars snow checkers)
for i in $(seq 1 "$INPUTS"); do
    act "{\"action\":\"add_input\",\"source\":{\"kind\":\"pattern\",\"pattern\":\"${patterns[$((i % 4))]}\"},\"name\":\"In $i\"}"
done
sleep 5
online="$(curl -sf "$CORE/state" | jq '[.inputs[] | select(.status=="online")] | length')"
[[ "$online" == "$INPUTS" ]] && pass "$INPUTS inputs online" || fail "only $online of $INPUTS inputs online"
act '{"action":"cut_direct","input":1}'

mkdir -p "$WORK/rec"
act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"record\",\"folder\":\"$WORK/rec\",\"container\":\"mkv\"},\"name\":\"Rec\"}"
act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"srt\",\"uri\":\"srt://:${SRT_PORT}?mode=listener\"},\"name\":\"Stream\"}"
act '{"action":"start_output","output":"Rec"}'
act '{"action":"start_output","output":"Stream"}'
sleep 2
# Receiver: the stream as it arrives, checked with ffprobe at the end.
gst-launch-1.0 -q srtsrc "uri=srt://127.0.0.1:${SRT_PORT}?mode=caller" ! \
    filesink location="$WORK/received.ts" >"$WORK/receiver.log" 2>&1 &
RECEIVER_PID=$!
PIDS+=("$RECEIVER_PID")
START=$(date +%s)
END=$((START + MINUTES * 60))

cpu_ticks() { awk '{print $14+$15}' "/proc/$1/stat"; }
TK=$(getconf CLK_TCK)
medians=()
measure() {
    local now
    now=$(date +%s)
    before=$(cpu_ticks "$STROM_PID")
    curl -sf -X POST "$CORE/latency/measure?seconds=5" >/dev/null || true
    sleep 6
    cpu=$((($(cpu_ticks "$STROM_PID") - before) * 100 / TK / 6))
    latency="$(curl -sf "$CORE/latency" || echo '{}')"
    median="$(jq -r '.median_ms // "null"' <<<"$latency")"
    p95="$(jq -r '.p95_ms // "null"' <<<"$latency")"
    stats="$(curl -sf "$CORE/outputs/stats" || echo '[]')"
    link="$(jq -r '.[0] | "\(.kbps) kbit/s, lost \(.packets_lost), dropped \(.packets_dropped), rtt \(.rtt_ms)"' <<<"$stats" 2>/dev/null || echo -)"
    states="$(curl -sf "$CORE/state" | jq -r '[.outputs[] | "\(.name)=\(.status.state)"] | join(" ")')"
    gpu="$(nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv,noheader 2>/dev/null | tr -d ' ' || echo -)"
    log "t=$(((now - START) / 60))min latency median ${median} ms p95 ${p95} ms; strom cpu ${cpu}%; gpu ${gpu}; ${states}; srt ${link}"
    if [[ "$median" != null ]]; then
        medians+=("$median")
    fi
    if [[ "$states" == *failed* || "$states" == *stopped* ]]; then
        fail "an output is not running: $states"
    fi
}
next_measure=$START
switch=0
while (($(date +%s) < END)); do
    now=$(date +%s)
    if ((now >= next_measure)); then
        measure
        next_measure=$((next_measure + EVERY_S))
    fi
    # Program changes every 30 s, through the mixer's transition.
    switch=$(((switch % INPUTS) + 1))
    act "{\"action\":\"preview_input\",\"input\":$(((switch % INPUTS) + 1))}" || true
    act '{"action":"transition","slot":1}' || true
    sleep 30
done
measure
ELAPSED=$(($(date +%s) - START))

act '{"action":"stop_output","output":"Rec"}'
act '{"action":"stop_output","output":"Stream"}'
sleep 5
kill "$RECEIVER_PID" 2>/dev/null || true

# Latency: no growth over the run.
if ((${#medians[@]} >= 2)); then
    first=${medians[0]}
    last=${medians[-1]}
    max=$(printf '%s\n' "${medians[@]}" | sort -n | tail -1)
    if ((last - first <= 20)); then
        pass "latency steady: first ${first} ms, last ${last} ms, highest ${max} ms"
    else
        fail "latency grew: first ${first} ms, last ${last} ms"
    fi
else
    fail "too few latency measurements (${#medians[@]})"
fi

# Recording and stream cover the run (frames at 30 fps, 2 % slack).
expect=$((ELAPSED * 30 * 98 / 100))
probe_frames() {
    ffprobe -v error -select_streams v:0 -count_packets -show_entries stream=nb_read_packets \
        -of csv=p=0 "$1" 2>/dev/null | head -1
}
rec="$(find "$WORK/rec" -type f | head -1)"
if [[ -n "$rec" ]]; then
    frames="$(probe_frames "$rec")"
    ((${frames:-0} >= expect)) && pass "recording: ${frames} frames for ${ELAPSED} s ($(du -h "$rec" | cut -f1))" \
        || fail "recording short: ${frames:-0} frames, expected ≥ ${expect}"
else
    fail "no recording file"
fi
frames="$(probe_frames "$WORK/received.ts")"
((${frames:-0} >= expect)) && pass "SRT received: ${frames} frames for ${ELAPSED} s" \
    || fail "SRT received short: ${frames:-0} frames, expected ≥ ${expect}"

grep -q CRITICAL "$WORK/strom.log" && fail "GStreamer CRITICALs in strom.log" || pass "no GStreamer CRITICALs"
[[ "$FAILED" == 0 ]] && log "soak passed" || log "soak FAILED (work kept: $WORK)"
echo "report: $REPORT"
exit "$FAILED"
