#!/usr/bin/env bash
# Latency source → program against the number of mixer inputs (NFA.3,
# DECISIONS 31.10.2026 "Mischer wächst mit"): for each count in COUNTS
# a fresh strom and lmix-core, that many moving pattern inputs, then
# each of up to PROBES inputs on program, measured SECONDS each. Prints
# one row per count (median and p95 over all probes, strom's CPU, GPU
# memory) and fails when a median is over BUDGET_MS.
#
#   scripts/measure-input-latency.sh                   # CUDA, 4 8 16 24 40
#   MIXER=cpu COUNTS="4 8 16" scripts/measure-input-latency.sh
#   COUNTS="16 16 16" …                                # one count, three runs
#   STROM_BIN=…/strom CORE_BIN=…/lmix-core …           # skip the builds
#
# Needs: strom (scripts/fetch-strom.sh), jq; for CUDA an NVIDIA GPU with
# nvcodec. Runs on real hardware only (time budget, not for CI). Work
# files go to WORK (default under ~/.cache/claude), kept with KEEP_WORK=1.
set -euo pipefail

MIXER="${MIXER:-cuda}"
COUNTS="${COUNTS:-4 8 16 24 40}"
PROBES="${PROBES:-4}"
SECONDS_EACH="${SECONDS_EACH:-4}"
# NFA.3: two frames at 30 fps.
BUDGET_MS="${BUDGET_MS:-67}"
STROM_PORT="${STROM_PORT:-8597}"
CORE_PORT="${CORE_PORT:-8697}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
STROM="http://127.0.0.1:${STROM_PORT}"
CORE="http://127.0.0.1:${CORE_PORT}/api"
mkdir -p "$HOME/.cache/claude"
WORK="${WORK:-$(mktemp -d -p "$HOME/.cache/claude" input-latency.XXXX)}"
PIDS=()
FAILED=0
RUN=0

stop_all() {
    for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
    wait 2>/dev/null || true
    PIDS=()
}
cleanup() {
    stop_all
    if [[ "${KEEP_WORK:-}" != 1 ]]; then rm -rf "$WORK"; fi
}
trap cleanup EXIT

act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/actions" >/dev/null; }
wait_up() {
    for _ in $(seq 1 120); do curl -sf "$1" >/dev/null 2>&1 && return 0; sleep 0.5; done
    echo "$2 did not come up" >&2
    exit 1
}
cpu_ticks() { awk '{print $14+$15}' "/proc/$1/stat"; }
TK=$(getconf CLK_TCK)

STROM_BIN="${STROM_BIN:-$("$ROOT/scripts/fetch-strom.sh")}"
if [[ -z "${CORE_BIN:-}" ]]; then
    (cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
    CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
fi

# Starts strom and lmix-core for `$1` inputs; extra lmix-core flags
# come from CORE_ARGS.
start() {
    RUN=$((RUN + 1))
    local n="$1" dir="$WORK/run$RUN-$1"
    mkdir -p "$dir"
    (cd "$dir" && exec "$STROM_BIN" --host 127.0.0.1 --port "$STROM_PORT" --no-auto-restart \
        --data-dir "$dir/strom-data" >"$dir/strom.log" 2>&1) &
    STROM_PID=$!
    PIDS+=("$STROM_PID")
    wait_up "$STROM/health" strom
    local slots=()
    # A core from before 31.10.2026 (fixed slots) gets exactly the inputs.
    if "$CORE_BIN" --help | grep -q -- --slots; then slots=(--slots "$n"); fi
    # shellcheck disable=SC2086
    "$CORE_BIN" --strom "$STROM" --listen "127.0.0.1:${CORE_PORT}" --mixer "$MIXER" \
        "${slots[@]}" ${CORE_ARGS:-} --autosave "$dir/autosave.lmix" --shortcuts "$dir/shortcuts.json" \
        --layout-templates "$dir/layout-templates.json" >"$dir/core.log" 2>&1 &
    PIDS+=($!)
    wait_up "$CORE/state" lmix-core
}

patterns=(ball bars snow checkers)
echo "mixer ${MIXER}, ${PROBES} inputs on program per count, ${SECONDS_EACH} s each"
printf '%6s %8s %8s %8s %10s %10s\n' inputs median p95 max "strom cpu" "gpu MiB"
for n in $COUNTS; do
    start "$n"
    for i in $(seq 1 "$n"); do
        act "{\"action\":\"add_input\",\"source\":{\"kind\":\"pattern\",\"pattern\":\"${patterns[$((i % 4))]}\"},\"name\":\"In $i\"}"
    done
    for _ in $(seq 1 60); do
        online="$(curl -sf "$CORE/state" | jq '[.inputs[] | select(.status=="online")] | length')"
        [[ "$online" == "$n" ]] && break
        sleep 0.5
    done
    sleep 3

    # Inputs spread over the count: first, last and between.
    probes=()
    for p in $(seq 0 $((PROBES - 1))); do
        probes+=($((1 + p * (n - 1) / (PROBES > 1 ? PROBES - 1 : 1))))
    done
    samples=()
    before=$(cpu_ticks "$STROM_PID")
    t0=$(date +%s%N)
    for input in "${probes[@]}"; do
        act "{\"action\":\"cut_direct\",\"input\":$input}"
        sleep 1
        curl -sf -X POST "$CORE/latency/measure?seconds=$SECONDS_EACH" >/dev/null
        sleep $((SECONDS_EACH + 1))
        median="$(curl -sf "$CORE/latency" | jq -r '.median_ms // empty')"
        [[ -n "$median" ]] && samples+=("$median")
        echo "  $n inputs, input $input on program: ${median:-none} ms" >>"$WORK/probes.txt"
    done
    cpu=$((($(cpu_ticks "$STROM_PID") - before) * 100 * 1000000000 / TK / ($(date +%s%N) - t0)))
    gpu="$(nvidia-smi --query-gpu=memory.used --format=csv,noheader,nounits 2>/dev/null || echo -)"
    stop_all

    if ((${#samples[@]} == 0)); then
        echo "FAIL $n inputs: no latency samples (logs in $WORK/run$RUN-$n)" >&2
        FAILED=1
        KEEP_WORK=1
        continue
    fi
    sorted=($(printf '%s\n' "${samples[@]}" | sort -n))
    med=${sorted[$((${#sorted[@]} / 2))]}
    p95=${sorted[$(((${#sorted[@]} * 95 + 99) / 100 - 1))]}
    max=${sorted[-1]}
    printf '%6s %8s %8s %8s %9s%% %10s\n' "$n" "$med" "$p95" "$max" "$cpu" "$gpu"
    if (($(printf '%.0f' "$med") > BUDGET_MS)); then
        echo "FAIL $n inputs: median ${med} ms over ${BUDGET_MS} ms" >&2
        FAILED=1
    fi
done
cat "$WORK/probes.txt"
exit "$FAILED"
