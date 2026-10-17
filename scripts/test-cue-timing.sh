#!/usr/bin/env bash
# Cue timing (show mode RUN.8, SNF.4): waits and follows within 10 ms.
# lmix-core with the null engine runs a chain of 50 waits of 20 ms that
# starts itself again (about 5000 steps a minute), then /metrics must
# show no step later than 10 ms after its due time.
#
#   scripts/test-cue-timing.sh                  # 1 minute
#   DURATION_S=14400 scripts/test-cue-timing.sh # the 4 hours of RUN.8
#
# Stays local: a busy shared CI runner is not the machine of a show.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PORT=8895
CORE="http://127.0.0.1:${PORT}"
DURATION_S="${DURATION_S:-60}"
WAITS=50
WAIT_MS=20
WORK="$(mktemp -d)"
PID=""
cleanup() {
    [[ -n "$PID" ]] && kill "$PID" 2>/dev/null || true
    wait 2>/dev/null || true
    [[ "${KEEP_WORK:-}" == 1 ]] && echo "work: $WORK" || rm -rf "$WORK"
}
trap cleanup EXIT
fail() {
    echo "FAIL: $*"
    KEEP_WORK=1
    exit 1
}
act() { curl -sf -X POST -H 'content-type: application/json' -d "$1" "$CORE/api/actions" >/dev/null; }
metric() { curl -sf "$CORE/metrics" | awk -v m="$1" '$1 == m { print $2 }'; }

(cd "$ROOT/workspace/lmix-core" && cargo build --release >/dev/null 2>&1)
CORE_BIN="${CARGO_TARGET_DIR:-$ROOT/workspace/lmix-core/target}/release/lmix-core"
"$CORE_BIN" --engine none --listen "127.0.0.1:${PORT}" --autosave "$WORK/autosave.lmix" \
    --shortcuts "$WORK/shortcuts.json" --layout-templates "$WORK/layout-templates.json" \
    --targets "$WORK/targets.json" >"$WORK/core.log" 2>&1 &
PID=$!
for _ in $(seq 1 60); do curl -sf "$CORE/health" >/dev/null && break; sleep 0.5; done

# 1 … 50: waits that follow each other; the last starts cue 1 again.
cues="$(jq -cn --argjson n "$WAITS" --argjson ms "$WAIT_MS" '
    [range(1; $n + 1) | {number: ("\(.)"), kind: "wait", duration_ms: $ms, follow: "after_end"}]
    + [{number: "loop", kind: "start", target: {cue: "1"}}]')"
act "{\"action\":\"add_cue_list\",\"name\":\"Timing\",\"cues\":$cues}" || fail "cue list refused"
act '{"action":"cue_go"}' || fail "GO refused"

echo "running $DURATION_S s …"
sleep "$DURATION_S"
steps="$(metric lmix_cue_steps_total)"
late="$(metric lmix_cue_late_max_seconds)"
over="$(metric lmix_cue_late_over_budget_total)"
echo "steps $steps, latest $(awk -v s="$late" 'BEGIN { printf "%.1f", s * 1000 }') ms, over 10 ms: $over"
(( steps > DURATION_S * 1000 / WAIT_MS )) || fail "too few steps: $steps (the chain stopped?)"
[[ "$over" == 0 ]] || fail "$over steps more than 10 ms late (RUN.8)"
echo "ok   cue timing: $steps steps within 10 ms"
