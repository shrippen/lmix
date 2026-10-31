#!/usr/bin/env bash
# Long run on real hardware (HARDWARE-TODO H.10, H.18, H.22; LASTENHEFT
# NFA.7): engine with the GPU mixer, inputs that move, program switched
# every 30 s, a recording, SRT streams to local receivers and an ISO
# recording of input 1. Every 5 minutes it measures the latency source →
# program, the link health and strom's memory; at the end it checks that
# latency and memory did not grow, the outputs never failed, and that the
# recordings and the received streams cover the whole run without gaps.
#
#   scripts/soak-hardware.sh                 # 60 min, CUDA mixer, 8 inputs
#   MINUTES=480 INPUTS=16 STREAMS=2 ISO=1 scripts/soak-hardware.sh   # NFA.7
#   MINUTES=10 MIXER=gl scripts/soak-hardware.sh
#   RECORD=0 STREAMS=0 …                     # no outputs: the mixer alone
#   KEEP_WORK=1 …                            # keep logs, recordings, streams
#   MALLOC_ARENA_MAX=2 …                     # passed on to strom
#   WORK=/big/disk/soak …                    # where recordings go (see below)
#   STROM_BIN=…/strom …                      # skip scripts/fetch-strom.sh
#
# Recordings and received streams take ~2.7 GB per file and hour (6 Mbit/s;
# 8 h NFA.7 ~90 GB, 48 h ~520 GB). The run refuses to start when WORK has
# less room, and when WORK is on tmpfs (/tmp here: RAM).
#
# Needs: strom (scripts/fetch-strom.sh), jq, ffprobe, gst-launch-1.0 with
# srtsrc. Report: $WORK/report.txt (path printed at the end); strom's
# memory once a minute in $WORK/rss.csv (seconds, MB).
set -euo pipefail

MINUTES="${MINUTES:-60}"
MIXER="${MIXER:-cuda}"
INPUTS="${INPUTS:-8}"
STREAMS="${STREAMS:-1}"
ISO="${ISO:-0}"
RECORD="${RECORD:-1}"
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
        find "$WORK" -mindepth 1 ! -name report.txt ! -name rss.csv -delete 2>/dev/null || true
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

# Room for the recording, the ISO recording and each received stream.
FILES=$((RECORD + ISO + STREAMS))
NEED_MB=$((FILES * 6000 / 8 * MINUTES * 60 * 12 / 10 / 1000))
if [[ "$(findmnt -no FSTYPE --target "$WORK")" == tmpfs ]]; then
    echo "WORK=$WORK is on tmpfs (RAM); set WORK to a folder on disk" >&2
    exit 1
fi
FREE_MB=$(($(df --output=avail -k "$WORK" | tail -1) / 1024))
if ((FREE_MB < NEED_MB)); then
    echo "WORK=$WORK has ${FREE_MB} MB free, the run needs ~${NEED_MB} MB" >&2
    exit 1
fi

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"

log "soak: ${MINUTES} min, mixer ${MIXER}, ${INPUTS} inputs, ${STREAMS} streams, ISO ${ISO}, work ${WORK}"
(cd "$WORK" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
    --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
STROM_PID=$!
PIDS+=("$STROM_PID")
wait_up "$STROM/health" strom
"$CORE_BIN" --strom "$STROM" --listen "127.0.0.1:${CORE_PORT}" --mixer "$MIXER" \
    --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" --layout-templates "$WORK/layout-templates.json" >"$WORK/core.log" 2>&1 &
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

OUTPUTS=()
if [[ "$RECORD" == 1 ]]; then
    mkdir -p "$WORK/rec"
    act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"record\",\"folder\":\"$WORK/rec\",\"container\":\"mkv\"},\"name\":\"Rec\"}"
    OUTPUTS+=(Rec)
fi
if [[ "$ISO" == 1 ]]; then
    mkdir -p "$WORK/iso"
    first="$(curl -sf "$CORE/state" | jq -r '.inputs[0].id')"
    act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"record\",\"folder\":\"$WORK/iso\",\"container\":\"mkv\"},\"source\":{\"from\":\"input\",\"input\":\"$first\"},\"name\":\"ISO\"}"
    OUTPUTS+=(ISO)
fi
for n in $(seq 1 "$STREAMS"); do
    act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"srt\",\"uri\":\"srt://:$((SRT_PORT + n))?mode=listener\"},\"name\":\"Stream $n\"}"
    OUTPUTS+=("Stream $n")
done
for name in "${OUTPUTS[@]}"; do
    act "{\"action\":\"start_output\",\"output\":\"$name\"}"
done
sleep 2
# Receivers: each stream as it arrives, checked with ffprobe at the end.
RECEIVERS=()
for n in $(seq 1 "$STREAMS"); do
    gst-launch-1.0 -q srtsrc "uri=srt://127.0.0.1:$((SRT_PORT + n))?mode=caller" ! \
        filesink location="$WORK/received-$n.ts" >"$WORK/receiver-$n.log" 2>&1 &
    RECEIVERS+=($!)
    PIDS+=($!)
done
START=$(date +%s)
END=$((START + MINUTES * 60))
(
    while kill -0 "$STROM_PID" 2>/dev/null; do
        awk -v t=$(($(date +%s) - START)) '/VmRSS/ { printf "%d,%d\n", t, $2 / 1024 }' \
            "/proc/$STROM_PID/status" >>"$WORK/rss.csv" 2>/dev/null || true
        sleep 60
    done
) &
PIDS+=($!)

cpu_ticks() { awk '{print $14+$15}' "/proc/$1/stat"; }
TK=$(getconf CLK_TCK)
medians=()
rss_at_10=""
rss_last=""
rss_mb() { awk '/VmRSS/ { printf "%d", $2 / 1024 }' "/proc/$1/status"; }
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
    link="$(jq -r 'map("\(.kbps) kbit/s" + (if .srt then " lost \(.srt.packets_lost) dropped \(.srt.packets_dropped)" else "" end)) | join("; ")' <<<"$stats" 2>/dev/null || echo -)"
    states="$(curl -sf "$CORE/state" | jq -r '[.outputs[] | "\(.name)=\(.status.state)"] | join(" ")')"
    gpu="$(nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv,noheader 2>/dev/null | tr -d ' ' || echo -)"
    rss_last="$(rss_mb "$STROM_PID")"
    if [[ -z "$rss_at_10" ]] && ((now - START >= 600)); then
        rss_at_10="$rss_last"
    fi
    log "t=$(((now - START) / 60))min latency median ${median} ms p95 ${p95} ms; strom cpu ${cpu}% rss ${rss_last} MB; gpu ${gpu}; ${states}; srt ${link}"
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

for name in "${OUTPUTS[@]}"; do
    act "{\"action\":\"stop_output\",\"output\":\"$name\"}"
done
sleep 5
for pid in "${RECEIVERS[@]}"; do kill "$pid" 2>/dev/null || true; done

# strom's memory after warm-up (10 min) against the end.
if [[ -n "$rss_at_10" ]]; then
    if ((rss_last <= rss_at_10 * 125 / 100)); then
        pass "strom memory steady: ${rss_at_10} MB at 10 min, ${rss_last} MB at the end"
    else
        fail "strom memory grew: ${rss_at_10} MB at 10 min, ${rss_last} MB at the end"
    fi
fi

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
check_file() {
    local what="$1" file="$2" frames
    if [[ -z "$file" || ! -s "$file" ]]; then
        fail "$what: no file"
        return
    fi
    frames="$(probe_frames "$file" | tr -cd 0-9)"
    if ((${frames:-0} >= expect)); then
        pass "$what: ${frames} frames for ${ELAPSED} s ($(du -h "$file" | cut -f1))"
    else
        fail "$what short: ${frames:-0} frames, expected ≥ ${expect}"
    fi
}
if [[ "$RECORD" == 1 ]]; then
    check_file recording "$(find "$WORK/rec" -type f | head -1)"
fi
if [[ "$ISO" == 1 ]]; then
    check_file "ISO recording" "$(find "$WORK/iso" -type f | head -1)"
fi
for n in $(seq 1 "$STREAMS"); do
    check_file "SRT $n received" "$WORK/received-$n.ts"
done

# GStreamer's PipeWire device provider (strom's device discovery at start)
# trips over degenerate int ranges on some desktops; not lmix or strom.
PIPEWIRE_RANGES='gst_value_collect_int_range|range start is not smaller than end'
if grep CRITICAL "$WORK/strom.log" | grep -Evq "$PIPEWIRE_RANGES"; then
    fail "GStreamer CRITICALs in strom.log"
else
    pass "no GStreamer CRITICALs"
fi
[[ "$FAILED" == 0 ]] && log "soak passed" || log "soak FAILED (work kept: $WORK)"
echo "report: $REPORT"
exit "$FAILED"
