#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
METRICS_PORT="${METRICS_PORT:-8080}"
PIDFILE="${PIDFILE:-$ROOT/.demo.pid}"

echo "Stopping demo services (metrics port ${METRICS_PORT})..."
pkill -f "nexus-loader" 2>/dev/null || true

if [ -f "$PIDFILE" ]; then
  pid="$(cat "$PIDFILE" 2>/dev/null || true)"
  if [ -n "${pid:-}" ]; then
    kill "$pid" 2>/dev/null || true
  fi
  rm -f "$PIDFILE" 2>/dev/null || true
fi

for _ in 1 2 3 4 5; do
  if command -v ss >/dev/null 2>&1; then
    ss -tlnH 2>/dev/null | grep -qE ":${METRICS_PORT}\\s" || break
  elif command -v lsof >/dev/null 2>&1; then
    lsof -iTCP:"${METRICS_PORT}" -sTCP:LISTEN -t >/dev/null 2>&1 || break
  else
    break
  fi
  sleep 0.3
done
echo "Optional eBPF cleanup (requires sudo): sudo make -C \"$ROOT\" cleanup"
