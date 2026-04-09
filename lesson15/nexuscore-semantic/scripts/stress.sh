#!/usr/bin/env bash
set -euo pipefail
if ! command -v socat &>/dev/null; then
  echo "[stress] socat not found; install socat to run stress"
  exit 1
fi
QUERIES=("supply chain disruption" "carbon emissions market" "DeFi protocol exploit"
         "mRNA vaccine trial" "diplomatic sanctions energy")
run_worker(){
  local tid=$1; local q="${QUERIES[$((tid % 5))]}"
  for _ in $(seq 1 1000); do
    printf '{"op":"query","tenant_id":%d,"text":"%s","top_k":5}\n' "$tid" "$q" \
      | socat - UNIX-CONNECT:/tmp/nexuscore-host.sock >/dev/null 2>&1
  done; echo "  worker $tid finished"
}
export -f run_worker; export QUERIES
T0=$(date +%s%N)
for i in $(seq 0 9); do run_worker "$i" & done; wait
T1=$(date +%s%N); E=$(( (T1-T0)/1000000 ))
echo "[stress] 10,000 queries across 10 tenants in ${E}ms ($((10000000/E)) qps)"
