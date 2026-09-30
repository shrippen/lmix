#!/usr/bin/env bash
# Starts the lmix Console: the strom backend (headless, no-gui feature,
# --host 127.0.0.1) as a separate background process, waits for it to
# become healthy, then runs the app-desktop Console client in the
# foreground. Closing the Console window stops the backend too.
# Release builds: debug GStreamer glue is too slow for live video.
#
# Port defaults to 8099, not strom's own 8080 default: on this machine
# 8080 is already bound by an unrelated docker-proxy container. Override
# with LMIX_PORT if that's not the case for you.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HOST="127.0.0.1"
PORT="${LMIX_PORT:-8099}"
# Covers a first release build of the backend.
HEALTH_TIMEOUT_S=300
BASE_URL="http://${HOST}:${PORT}"

BACKEND_PID=""
cleanup() {
    if [[ -n "$BACKEND_PID" ]] && kill -0 "$BACKEND_PID" 2>/dev/null; then
        echo "Stopping strom backend (pid $BACKEND_PID)..."
        kill "$BACKEND_PID" 2>/dev/null || true
        wait "$BACKEND_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

echo "Starting strom backend on ${BASE_URL} ..."
(
    cd "$ROOT/workspace/strom/backend"
    exec cargo run --release --features no-gui -- --host "$HOST" --port "$PORT" --no-auto-restart
) &
BACKEND_PID=$!

echo -n "Waiting for backend to become healthy"
up=false
for _ in $(seq 1 "$HEALTH_TIMEOUT_S"); do
    if [[ "$(curl -s -o /dev/null -w '%{http_code}' "${BASE_URL}/health" 2>/dev/null)" == "200" ]]; then
        up=true
        break
    fi
    echo -n "."
    sleep 1
done
echo

if [[ "$up" != "true" ]]; then
    echo "Backend did not become healthy within ${HEALTH_TIMEOUT_S}s - check its output above." >&2
    exit 1
fi
echo "Backend is up (${BASE_URL}, swagger-ui at ${BASE_URL}/swagger-ui)."

echo "Starting app-desktop Console ..."
(
    cd "$ROOT/workspace/app-desktop"
    exec cargo run --release -- --server "$BASE_URL"
)
