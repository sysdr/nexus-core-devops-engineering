#!/usr/bin/env bash
# Short synthetic load: host for DURATION seconds (no perf required).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
DURATION="${1:-5}"
RPS="${2:-80000}"

cargo build -p nexuscore-host --release
python3 scripts/gen_graph.py 5000

echo "[stress] Host for ${DURATION}s at NEXUSCORE_RPS=${RPS}"
NEXUSCORE_RPS="$RPS" cargo run -p nexuscore-host --release &
HOST_PID=$!
sleep "$DURATION"
kill "$HOST_PID" 2>/dev/null || true
wait "$HOST_PID" 2>/dev/null || true
echo "[stress] Done."
