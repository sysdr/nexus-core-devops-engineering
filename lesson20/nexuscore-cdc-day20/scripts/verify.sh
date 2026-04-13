#!/usr/bin/env bash
set -uo pipefail
BOLD='\033[1m'; GREEN='\033[0;32m'; RED='\033[0;31m'; BLUE='\033[0;34m'; ORANGE='\033[0;33m'; RESET='\033[0m'
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
PASS=0; FAIL=0
check() {
  local name=$1; shift
  if "$@" &>/dev/null; then
    echo -e "  ${GREEN}✓${RESET} $name"; ((PASS++))
  else
    echo -e "  ${RED}✗${RESET} $name"; ((FAIL++))
  fi
}

echo -e "\n${BOLD}${BLUE}NexusCore Day 20 — Verification${RESET}\n"

if curl -sf --max-time 2 http://127.0.0.1:6333/healthz >/dev/null || curl -sf --max-time 2 http://localhost:6333/healthz >/dev/null; then
  echo -e "  ${GREEN}✓${RESET} Qdrant reachable"
  ((PASS++))
else
  echo -e "  ${ORANGE}⚠${RESET} Qdrant not reachable (optional for metrics-only demo; start Docker if needed)"
fi

check "Metrics endpoint up" curl -sf http://localhost:9090/metrics
check "CDC events counter exists" bash -c 'curl -sf http://localhost:9090/metrics | grep -q cdc_events_total'
check "CDC upserts counter exists" bash -c 'curl -sf http://localhost:9090/metrics | grep -q cdc_qdrant_upserts_total'
check "Non-zero CDC traffic" bash -c 'python3 - <<PY
import re, sys, urllib.request
u = urllib.request.urlopen("http://127.0.0.1:9090/metrics")
text = u.read().decode()
ev = sum(float(m.group(1)) for m in re.finditer(r"^cdc_events_total\{[^}]*\}\s+(\d+(?:\.\d+)?(?:e\+\d+)?)", text, re.M))
up = sum(float(m.group(1)) for m in re.finditer(r"^cdc_qdrant_upserts_total\{[^}]*\}\s+(\d+(?:\.\d+)?(?:e\+\d+)?)", text, re.M))
sys.exit(0 if ev > 0 and up > 0 else 1)
PY'
check "Loader process running" bash -c 'test -f .loader.pid && kill -0 "$(cat .loader.pid)"'

echo -e "\n  ${BOLD}${PASS} passed, ${FAIL} failed${RESET}"
[[ $FAIL -eq 0 ]]
