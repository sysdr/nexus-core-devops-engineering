#!/usr/bin/env bash
set -euo pipefail
RATE=${1:-200}
DURATION=${2:-5}
TENANTS=${3:-10}
BOLD='\033[1m'; GREEN='\033[0;32m'; BLUE='\033[0;34m'; RED='\033[0;31m'; ORANGE='\033[0;33m'; RESET='\033[0m'
echo -e "\n${BOLD}${BLUE}NexusCore demo load (HTTP /demo/pulse)${RESET}"
if ! curl -sf http://localhost:9090/metrics >/dev/null; then
  echo -e "${RED}metrics not reachable — start the loader first${RESET}"; exit 1
fi
if ! curl -sf --max-time 2 http://127.0.0.1:6333/healthz >/dev/null && ! curl -sf --max-time 2 http://localhost:6333/healthz >/dev/null; then
  echo -e "${ORANGE}⚠ Qdrant not reachable — continuing (metrics-only demo)${RESET}"
fi
TOTAL=$((RATE * DURATION))
for ((i=1; i<=DURATION; i++)); do
  curl -sf -X POST http://localhost:9090/demo/pulse \
    -H 'Content-Type: application/json' \
    -d "{\"events\":${RATE},\"upserts\":$((RATE*4/5))}" >/dev/null
  echo -e "  ${GREEN}✓${RESET} pulse $i/${DURATION}"
  sleep 1
done
echo -e "\n${GREEN}Done.${RESET} (requested ~${TOTAL} events)"
