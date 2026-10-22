#!/usr/bin/env bash
# Sound cues on a real sound device (HARDWARE-TODO H.37): strom and
# lmix-core in the desktop session (PipeWire), then check.py. Needs real
# hardware, so it stays out of CI.
#
#   scripts/hw-sound-cues/run.sh                        # the default output
#   DEVICE=alsa_output.usb-… DEVICE2=alsa_output.pci-… scripts/hw-sound-cues/run.sh
#   QUIET=1 …        # device at 1 % (-120 dB) while it runs: the monitor
#                    # source still carries the signal (checked on a
#                    # Scarlett Solo); the volume is restored at the end
#   KEEP_WORK=1 …    # keep logs and recordings
#
# Needs strom (scripts/fetch-strom.sh or STROM_BIN), pactl, parec, python3.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
DEVICE="${DEVICE:-$(pactl get-default-sink)}"
export DEVICE
WORK="$(mktemp -d)"
STROM_PORT=8399
CORE_PORT=8398
PIDS=()
VOLUMES=()
cleanup() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    for v in "${VOLUMES[@]}"; do pactl set-sink-volume "${v%%=*}" "${v#*=}"; done
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    echo "FAIL $2 did not come up"
    exit 1
}
if [[ "${QUIET:-}" == 1 ]]; then
    for sink in "$DEVICE" ${DEVICE2:+"$DEVICE2"}; do
        VOLUMES+=("$sink=$(pactl get-sink-volume "$sink" | grep -oE '[0-9]+%' | head -1)")
        pactl set-sink-volume "$sink" 1%
    done
fi

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
(cd "$WORK" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
    --data-dir "$WORK/strom-data" >"$WORK/strom.log" 2>&1) &
PIDS+=($!)
wait_up "http://127.0.0.1:$STROM_PORT/health" strom
"$CORE_BIN" --strom "http://127.0.0.1:$STROM_PORT" --listen "127.0.0.1:$CORE_PORT" --mixer cpu --slots 1 \
    --autosave "$WORK/autosave.lmix" --shortcuts "$WORK/shortcuts.json" \
    --layout-templates "$WORK/layout-templates.json" --targets "$WORK/targets.json" \
    >"$WORK/core.log" 2>&1 &
PIDS+=($!)
wait_up "http://127.0.0.1:$CORE_PORT/health" lmix-core
sleep 2
echo "device: $DEVICE${DEVICE2:+, $DEVICE2}"
python3 "$ROOT/scripts/hw-sound-cues/check.py" "http://127.0.0.1:$CORE_PORT" "$WORK"
