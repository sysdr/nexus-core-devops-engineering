#!/usr/bin/env bash
set -euo pipefail
need() { command -v "$1" &>/dev/null || { echo "Missing: $1 — $2"; exit 1; }; }
need curl  "install curl"

SOCK=/tmp/nexuscore-host.sock

if command -v socat &>/dev/null; then
  echo "[verify] Semantic query test..."
  printf '{"op":"query","tenant_id":0,"text":"supply chain disruption Southeast Asia","top_k":5}\n' \
    | socat - UNIX-CONNECT:$SOCK
  echo ""
  echo "[verify] Index stats for tenant 0..."
  printf '{"op":"stats","tenant_id":0}\n' | socat - UNIX-CONNECT:$SOCK
  echo ""
else
  echo "[verify] socat not found; skipping Unix socket request tests"
fi

curl -s http://localhost:9091/metrics | python3 -c "import json,sys;d=json.load(sys.stdin);print('[verify] metrics:', d); assert d['received']>0, 'expected received>0 after demo'"
if command -v perf &>/dev/null && [ -f /tmp/nexuscore-host.pid ]; then
  perf stat -e cycles,iTLB-load-misses -p "$(cat /tmp/nexuscore-host.pid)" -- sleep 2 2>&1 \
    | grep -E "iTLB|cycles" || echo "[info] perf not available; run manually"
else
  echo "[verify] perf not available; skipping iTLB probe"
fi
