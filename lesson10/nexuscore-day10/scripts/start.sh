#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
METRICS_PORT="${METRICS_PORT:-8080}"
PIDFILE="${PIDFILE:-$ROOT/.demo.pid}"
if command -v ss >/dev/null 2>&1; then
  if ss -tlnH 2>/dev/null | grep -qE ":${METRICS_PORT}\\s"; then
    echo "Port ${METRICS_PORT} is already in use. Run: $ROOT/scripts/stop.sh"
    exit 1
  fi
elif command -v lsof >/dev/null 2>&1; then
  if lsof -iTCP:"${METRICS_PORT}" -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo "Port ${METRICS_PORT} is already in use. Run: $ROOT/scripts/stop.sh"
    exit 1
  fi
fi
echo "Starting NexusCore Day 10 demo (cwd: $ROOT)..."

echo "Starting Python demo metrics server on port ${METRICS_PORT}"
nohup python3 "$ROOT/scripts/demo_metrics.py" --port "${METRICS_PORT}" --pidfile "${PIDFILE}" >/dev/null 2>&1 &
sleep 0.2 || true
echo "Dashboard: http://127.0.0.1:${METRICS_PORT}/"
echo "Metrics:   http://127.0.0.1:${METRICS_PORT}/metrics"
