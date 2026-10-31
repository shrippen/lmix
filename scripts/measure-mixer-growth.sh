#!/usr/bin/env bash
# Program while the mixer grows and shrinks (todo „Mischer wächst mit“,
# DECISIONS 31.10.2026): one pattern input (moving ball, steady tone) on
# program and recorded, then COUNT more inputs added one after another
# (each gets its mixer input and audio channel while the mixer runs),
# program latency measured with all of them, then all removed again.
# The recording is checked for gaps: frame times (no frame missing),
# repeated frames (scripts/frozen-frames.py: the ball stops when program
# stalls) and the tone (scripts/switch-gaps.py). Each new input is heard
# until its sound is switched off right after the add: those clicks are
# its own tone, counted but no failure. Prints strom's CPU and the
# latency before and with all inputs, and when each phase began in the
# recording; fails on a gap or a failed add.
#
#   scripts/measure-mixer-growth.sh             # CUDA mixer, 40 inputs
#   MIXER=cpu COUNT=20 scripts/measure-mixer-growth.sh
#   KEEP_WORK=1 …                               # keep logs and the recording
#   STROM_BIN=…/strom CORE_BIN=…/lmix-core …    # skip the builds
#
# Needs ffmpeg, ffprobe, jq, Python 3 with numpy; for CUDA an NVIDIA GPU.
# Runs on real hardware only (time budget, not for CI).
set -euo pipefail

MIXER="${MIXER:-cuda}"
COUNT="${COUNT:-40}"
STROM_PORT="${STROM_PORT:-8731}"
CORE_PORT="${CORE_PORT:-8732}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STROM="http://127.0.0.1:${STROM_PORT}"
CORE="http://127.0.0.1:${CORE_PORT}/api"
# The first slot's tone (lmix-core engine/flows.rs, TONE_BASE_HZ).
TONE_HZ=440
mkdir -p "$HOME/.cache/claude"
WORK="${WORK:-$(mktemp -d -p "$HOME/.cache/claude" mixer-growth.XXXX)}"
PIDS=()

cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT

fail() {
    echo "FAIL $* (logs: $WORK)" >&2
    KEEP_WORK=1
    exit 1
}
act() { curl -sS -f -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null || fail "action $1"; }
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    fail "$2 did not come up"
}
online() { curl -sf "$CORE/state" | jq '[.inputs[] | select(.status=="online")] | length'; }
cpu_ticks() { awk '{print $14+$15}' "/proc/$1/stat"; }
TK=$(getconf CLK_TCK)
# strom's CPU (% of one core) over `$1` seconds.
strom_cpu() {
    local before t0
    before=$(cpu_ticks "$STROM_PID")
    t0=$(date +%s%N)
    sleep "$1"
    echo $((($(cpu_ticks "$STROM_PID") - before) * 100 * 1000000000 / TK / ($(date +%s%N) - t0)))
}
latency() {
    curl -sf -X POST "$CORE/latency/measure?seconds=4" >/dev/null
    sleep 5
    curl -sf "$CORE/latency" | jq -r '.median_ms // "none"'
}
# Seconds since the recording began, roughly (its first frame comes a
# little after the start).
T_REC=0
at() { awk -v a="$T_REC" -v b="$(date +%s.%N)" 'BEGIN { printf "%.1f", b - a }'; }

python3 -c 'import numpy' 2>/dev/null || { echo "needs numpy for Python 3" >&2; exit 1; }
for tool in ffmpeg ffprobe jq; do
    command -v "$tool" >/dev/null || { echo "needs $tool" >&2; exit 1; }
done

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
if [[ -z "${CORE_BIN:-}" ]]; then
    (cd "$ROOT/workspace/lmix-core" && cargo build --release -q)
    CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
fi

mkdir -p "$WORK/rec"
(cd "$WORK" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
    --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
STROM_PID=$!
PIDS+=("$STROM_PID")
wait_up "$STROM/health" strom
"$CORE_BIN" --strom "$STROM" --listen "127.0.0.1:${CORE_PORT}" --mixer "$MIXER" \
    --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" \
    --layout-templates "$WORK/layout-templates.json" --live-dir "$WORK/live" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core

act '{"action":"add_input","source":{"kind":"pattern","pattern":"ball"},"name":"Program"}'
for _ in $(seq 1 60); do [[ "$(online)" == 1 ]] && break; sleep 0.5; done
act '{"action":"cut_direct","input":1}'
sleep 3
echo "mixer ${MIXER}, ${COUNT} inputs added one after another"
echo "1 input: strom $(strom_cpu 5) %, latency $(latency) ms"

act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"record\",\"folder\":\"$WORK/rec\",\"container\":\"mkv\"},\"name\":\"Rec\"}"
act '{"action":"start_output","output":"Rec"}'
T_REC=$(date +%s.%N)
sleep 4

patterns=(bars snow checkers ball)
echo "$(at) s: adding"
t_add=$(date +%s.%N)
for i in $(seq 1 "$COUNT"); do
    pattern="${patterns[$((i % 4))]}"
    # The last one goes on program: it moves.
    ((i == COUNT)) && pattern=ball
    act "{\"action\":\"add_input\",\"source\":{\"kind\":\"pattern\",\"pattern\":\"$pattern\"},\"name\":\"In $i\"}"
    # Only the program input's tone in the recording.
    act "{\"action\":\"set_audio\",\"input\":\"In $i\",\"on\":false}"
    sleep 0.5
done
for _ in $(seq 1 120); do [[ "$(online)" == $((COUNT + 1)) ]] && break; sleep 0.5; done
[[ "$(online)" == $((COUNT + 1)) ]] || fail "only $(online) of $((COUNT + 1)) inputs online"
t_added=$(date +%s.%N)
echo "$(at) s: all online"
sleep 3
echo "$((COUNT + 1)) inputs: strom $(strom_cpu 5) %, latency $(latency) ms," \
    "gpu $(nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits 2>/dev/null || echo -) MiB"
echo "$(at) s: In $COUNT on program"
act "{\"action\":\"cut_direct\",\"input\":\"In $COUNT\"}"
sleep 1
echo "In $COUNT on program: latency $(latency) ms"
act '{"action":"cut_direct","input":"Program"}'
sleep 2

echo "$(at) s: removing"
for i in $(seq 1 "$COUNT"); do
    act "{\"action\":\"remove_input\",\"input\":\"In $i\"}"
    sleep 0.3
done
echo "$(at) s: removed"
sleep 3
echo "removed again: strom $(strom_cpu 5) %, latency $(latency) ms"
act '{"action":"stop_output","output":"Rec"}'
sleep 3

file="$(find "$WORK/rec" -name '*.mkv' | head -1)"
[[ -n "$file" ]] || fail "no recording"
# Frames: none missing (a step over 1.5 frame times), none repeated.
ffprobe -v error -select_streams v:0 -show_entries frame=pts_time -of csv=p=0 "$file" >"$WORK/pts.txt"
read -r frames steps <<<"$(awk 'NR > 1 { d = $1 - p; if (d > 0.05) s++ } { p = $1 } END { print NR, s + 0 }' "$WORK/pts.txt")"
"$ROOT/scripts/frozen-frames.py" "$file" >"$WORK/frozen.txt"
"$ROOT/scripts/switch-gaps.py" "$file" "$TONE_HZ" >"$WORK/sound.txt"
grep -h -E '^(frozen|gap) ' "$WORK/frozen.txt" "$WORK/sound.txt" || true
freezes="$(awk '$1 == "summary" { print $3 }' "$WORK/frozen.txt")"
sound="$(awk '$1 == "gap" { g++ } $1 == "click" { c++ } END { printf "%d gaps, %d clicks", g, c }' "$WORK/sound.txt")"
echo "recording: $frames frames, $steps missing, $freezes repeated, sound $sound" \
    "(adds took $(awk -v a="$t_add" -v b="$t_added" 'BEGIN { printf "%.0f", b - a }') s)"
((steps == 0 && freezes == 0)) || fail "program had a gap"
[[ "$sound" == "0 gaps, "* ]] || fail "the tone had a gap"
