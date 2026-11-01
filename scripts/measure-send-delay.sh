#!/usr/bin/env bash
# Send delay of a stream (STR.5), measured as a receiver gets it: a pattern
# input's tone is on program, an SRT output sends it (listener) or an RTMP
# output publishes it to ffmpeg as the server; ffmpeg decodes the sound as
# it arrives; the tone is muted once and
# the time until the receiver hears the silence is the delay through the
# whole chain. Run once live and once delayed: the difference is the send
# delay. No GPU needed; stays out of CI (measures, does not judge).
#
#   scripts/measure-send-delay.sh            # live, then 5 s late
#   DELAYS="0 10" scripts/measure-send-delay.sh
#   OUTPUT=rtmp …                            # RTMP instead of SRT
#   KEEP_WORK=1 …                            # keep the logs
#   STROM_BIN=…/strom CORE_BIN=…/lmix-core … # skip the builds
#
# Needs ffmpeg, curl and Python 3.
set -euo pipefail

DELAYS="${DELAYS:-0 5}"
OUTPUT="${OUTPUT:-srt}"
PORT="${PORT:-8733}"
STROM_PORT="${STROM_PORT:-8731}"
CORE_PORT="${CORE_PORT:-8732}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="http://127.0.0.1:${CORE_PORT}/api"
mkdir -p "$HOME/.cache/lmix-send-delay"
WORK="$(mktemp -d -p "$HOME/.cache/lmix-send-delay")"
PIDS=()

cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT

fail() {
    echo "FAIL: $* (logs: $WORK)" >&2
    KEEP_WORK=1
    exit 1
}
act() {
    curl -sS -f -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null \
        || fail "action failed: $1"
}
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    fail "$2 did not come up"
}

for tool in ffmpeg curl python3; do
    command -v "$tool" >/dev/null || { echo "needs $tool" >&2; exit 1; }
done

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
if [[ -z "${CORE_BIN:-}" ]]; then
    (cd "$ROOT/workspace/lmix-core" && cargo build --release -q)
    CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
fi

(cd "$WORK" && exec env LIBGL_ALWAYS_SOFTWARE=1 "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" \
    --no-auto-restart --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
"$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" \
    --mixer cpu --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" \
    --layout-templates "$WORK/layout-templates.json" --live-dir "$WORK/live" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core

act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Tone"}'
act '{"action":"cut_direct","input":1}'
sleep 4

# The stream's sound as it arrives, into `$marks`; `$receiver` is its pid.
receive() {
    local url="$1"
    shift
    ffmpeg -v error "$@" -fflags nobuffer -probesize 64k -analyzeduration 500k -i "$url" \
        -vn -ac 1 -ar 8000 -f s16le - 2>"$WORK/ffmpeg-$delay.log" \
        | "$ROOT/scripts/send-delay.py" >"$marks" &
    receiver=$!
    PIDS+=("$receiver")
}

results=()
for delay in $DELAYS; do
    marks="$WORK/marks-$delay.txt"
    case "$OUTPUT" in
        srt)
            act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"srt\",\"uri\":\"srt://:$PORT?mode=listener\",\"delay_s\":$delay},\"name\":\"Late\"}"
            act '{"action":"start_output","output":"Late"}'
            sleep 1
            receive "srt://127.0.0.1:$PORT"
            ;;
        rtmp)
            # ffmpeg is the server here: it listens before the output starts.
            receive "rtmp://127.0.0.1:$PORT/live/delay" -listen 1
            sleep 1
            act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"rtmp\",\"url\":\"rtmp://127.0.0.1:$PORT/live\",\"key\":\"delay\",\"delay_s\":$delay},\"name\":\"Late\"}"
            act '{"action":"start_output","output":"Late"}'
            ;;
        *) echo "OUTPUT is srt or rtmp, not $OUTPUT" >&2; exit 2 ;;
    esac
    # Until the delayed stream flows and the receiver hears the tone.
    for _ in $(seq 1 $(((delay + 20) * 2))); do
        grep -q '^tone' "$marks" && break
        sleep 0.5
    done
    grep -q '^tone' "$marks" || fail "delay $delay s: the receiver never heard the tone"
    sleep 2
    muted="$(date +%s.%N)"
    act '{"action":"set_audio","input":1,"on":false}'
    sleep $((delay + 3))
    act '{"action":"set_audio","input":1,"on":true}'
    act '{"action":"stop_output","output":"Late"}'
    sleep 2
    kill "$receiver" 2>/dev/null || true
    pkill -f "ffmpeg .*127.0.0.1:$PORT" 2>/dev/null || true
    act '{"action":"remove_output","output":"Late"}'
    heard="$(awk -v m="$muted" '$1 == "silence" && $2 > m { print $2; exit }' "$marks")"
    [[ -n "$heard" ]] || fail "delay $delay s: the receiver never heard the mute"
    took="$(awk -v h="$heard" -v m="$muted" 'BEGIN { printf "%.2f", h - m }')"
    echo "delay ${delay} s: mute heard after ${took} s"
    results+=("$delay $took")
    sleep 2
done

# Each delay against the first (live) run.
read -r base_delay base_took <<<"${results[0]}"
for r in "${results[@]:1}"; do
    read -r delay took <<<"$r"
    awk -v d="$delay" -v t="$took" -v bd="$base_delay" -v bt="$base_took" \
        'BEGIN { printf "send delay %d s: %.2f s later than %d s\n", d, t - bt, bd }'
done
