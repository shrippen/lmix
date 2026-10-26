#!/usr/bin/env bash
# Sound of a running output while it changes source, measured instead of
# heard (todo „Hörtest: Abgriff-Wechsel“): a pattern input's steady tone is
# on program and on the bus "Stream"; a recording takes program, then
# switches to the bus and back (set_output_bus), then the bus switches its
# tap before and after the faders (set_bus_tap) while the recording takes
# it. scripts/switch-gaps.py finds gaps and clicks in the recording and
# names the switch before each. A mute of one second before the switches
# marks the recording's clock. No GPU needed; stays out of CI (measures,
# does not judge).
#
#   scripts/measure-switch-gaps.sh         # CPU mixer, 5 cycles each (10 bus
#                                          # switches, 10 tap switches)
#   MIXER=cuda CYCLES=10 scripts/measure-switch-gaps.sh
#   KEEP_WORK=1 …                          # keep logs and the recording
#   STROM_BIN=…/strom CORE_BIN=…/lmix-core …   # skip the builds
#
# Needs ffmpeg, jq and Python 3 with numpy.
set -euo pipefail

MIXER="${MIXER:-cpu}"
CYCLES="${CYCLES:-5}"
STROM_PORT="${STROM_PORT:-8721}"
CORE_PORT="${CORE_PORT:-8722}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CORE="http://127.0.0.1:${CORE_PORT}/api"
# The first pattern input's tone (lmix-core engine/flows.rs, TONE_BASE_HZ).
TONE_HZ=440
mkdir -p "$HOME/.cache/lmix-switch-gaps"
WORK="$(mktemp -d -p "$HOME/.cache/lmix-switch-gaps")"
PIDS=()

cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT

act() {
    curl -sS -f -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null && return 0
    echo "action failed: $1 (logs: $WORK)" >&2
    KEEP_WORK=1
    exit 1
}
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    echo "$2 did not come up (logs: $WORK)" >&2
    KEEP_WORK=1
    exit 1
}
now() { date +%s.%N; }

python3 -c 'import numpy' 2>/dev/null || { echo "needs numpy for Python 3" >&2; exit 1; }
for tool in ffmpeg jq; do
    command -v "$tool" >/dev/null || { echo "needs $tool" >&2; exit 1; }
done

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
if [[ -z "${CORE_BIN:-}" ]]; then
    (cd "$ROOT/workspace/lmix-core" && cargo build --release -q)
    CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
fi

mkdir -p "$WORK/rec"
case "$MIXER" in
    cpu) ENGINE_ENV=(LIBGL_ALWAYS_SOFTWARE=1) ;;
    *) ENGINE_ENV=() ;;
esac
(cd "$WORK" && exec env "${ENGINE_ENV[@]}" "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
    --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "http://127.0.0.1:${STROM_PORT}/health" strom
"$CORE_BIN" --strom "http://127.0.0.1:${STROM_PORT}" --listen "127.0.0.1:${CORE_PORT}" \
    --mixer "$MIXER" --slots 1 --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" \
    --layout-templates "$WORK/layout-templates.json" --live-dir "$WORK/live" >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "$CORE/state" lmix-core

act '{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Tone"}'
act '{"action":"cut_direct","input":1}'
act '{"action":"add_bus","name":"Stream"}'
act '{"action":"set_send","input":1,"bus":"Stream","level":100}'
sleep 4
act "{\"action\":\"add_output\",\"spec\":{\"kind\":\"record\",\"folder\":\"$WORK/rec\",\"container\":\"mkv\"},\"name\":\"Rec\"}"
act '{"action":"start_output","output":"Rec"}'
sleep 4

# The mark: one second without the tone.
mark="$(now)"
act '{"action":"set_audio","input":1,"on":false}'
sleep 1
act '{"action":"set_audio","input":1,"on":true}'
sleep 3
switches=()
for _ in $(seq 1 "$CYCLES"); do
    switches+=("$(now) bus")
    act '{"action":"set_output_bus","output":"Rec","bus":"Stream"}'
    sleep 2
    switches+=("$(now) program")
    act '{"action":"set_output_bus","output":"Rec","bus":null}'
    sleep 2
done
switches+=("$(now) bus")
act '{"action":"set_output_bus","output":"Rec","bus":"Stream"}'
sleep 2
for _ in $(seq 1 "$CYCLES"); do
    switches+=("$(now) pre")
    act '{"action":"set_bus_tap","bus":"Stream","pre_fader":true}'
    sleep 2
    switches+=("$(now) post")
    act '{"action":"set_bus_tap","bus":"Stream","pre_fader":false}'
    sleep 2
done
act '{"action":"stop_output","output":"Rec"}'
sleep 3

file="$(find "$WORK/rec" -name '*.mkv' | head -1)"
[[ -n "$file" ]] || { echo "no recording (logs: $WORK)" >&2; KEEP_WORK=1; exit 1; }
# The mark is the first gap of half a second or more.
mark_at="$("$ROOT/scripts/switch-gaps.py" "$file" "$TONE_HZ" | awk '$1 == "gap" && $3 >= 500 { print $2; exit }')"
[[ -n "$mark_at" ]] || { echo "the mute is not in the recording ($file)" >&2; KEEP_WORK=1; exit 1; }
offsets=()
for s in "${switches[@]}"; do
    offsets+=("$(awk -v t="${s% *}" -v m="$mark" -v a="$mark_at" 'BEGIN { printf "%.3f", t - m + a }')")
done
echo "recording: $file, mute at ${mark_at} s"
echo "switches (s in the recording):"
for i in "${!switches[@]}"; do echo "  ${offsets[$i]} ${switches[$i]#* }"; done
# Everything but the mark, which is meant.
"$ROOT/scripts/switch-gaps.py" "$file" "$TONE_HZ" "${offsets[@]}" | awk -v a="$mark_at" -v n="${#switches[@]}" '
    $1 ~ /^(gap|click)$/ && $2 >= a - 0.05 && $2 <= a + 1.2 { next }
    $1 == "gap" { gaps++ } $1 == "click" { clicks++ }
    $1 == "summary" { printf "%d switches: %d gaps, %d clicks in %s s %s %s %s %s\n", n, gaps, clicks, $4, $5, $6, $7, $8; next }
    { print }'
