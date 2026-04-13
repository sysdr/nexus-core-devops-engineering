#!/usr/bin/env bash
set -euo pipefail
BOLD='\033[1m'; GREEN='\033[0;32m'; BLUE='\033[0;34m'; RED='\033[0;31m'; ORANGE='\033[0;33m'; RESET='\033[0m'
QDRANT_IMAGE="${QDRANT_IMAGE:-qdrant/qdrant:v1.12.6}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
echo -e "${BOLD}${BLUE}NexusCore Day 20 — Starting CDC Pipeline${RESET}\n"

ulimit -l unlimited 2>/dev/null || true

if command -v fuser &>/dev/null; then
  fuser -k 9090/tcp 2>/dev/null || true
fi
if [[ -f .loader.pid ]]; then
  kill "$(cat .loader.pid)" 2>/dev/null || true
  rm -f .loader.pid
fi
pkill -f '[.]?/nexuscore-loader' 2>/dev/null || true
sleep 1

if command -v clang &>/dev/null; then
  echo -e "  ${GREEN}→${RESET} Compiling eBPF probe..."
  clang -g -O2 -target bpf -D__TARGET_ARCH_x86 \
    -I/usr/include/$(uname -m)-linux-gnu \
    -I/usr/local/include \
    -c ebpf/src/cdc_probe.bpf.c \
    -o ebpf/cdc_probe.bpf.o 2>&1 || echo "  (eBPF compile skipped — synthetic mode)"
else
  echo "  (clang not found — synthetic mode)"
fi

if command -v cargo-component &>/dev/null; then
  echo -e "  ${GREEN}→${RESET} Building WASI component..."
  (cd cdc-component && cargo component build --release --target wasm32-wasip2) || true
  cp -f cdc-component/target/wasm32-wasip2/release/nexuscore_cdc_component.wasm ./cdc_component.wasm 2>/dev/null || true
fi
touch cdc_component.wasm

if ! mount | grep -q ' /sys/fs/bpf '; then
  sudo mount -t bpf bpf /sys/fs/bpf 2>/dev/null || true
fi
sudo mkdir -p /sys/fs/bpf/nexuscore 2>/dev/null || true

if docker ps -a --format '{{.Names}}' | grep -qx nexuscore-qdrant; then
  if ! docker start nexuscore-qdrant >/dev/null 2>&1; then
    echo -e "  ${ORANGE}⚠${RESET} Could not start existing container nexuscore-qdrant — try: docker rm -f nexuscore-qdrant && ${ROOT}/scripts/start.sh"
  fi
else
  echo -e "  ${GREEN}→${RESET} Starting Qdrant (${QDRANT_IMAGE})..."
  if ! docker run -d --name nexuscore-qdrant -p 6333:6333 -p 6334:6334 "${QDRANT_IMAGE}"; then
    echo -e "  ${RED}✗${RESET} Qdrant container failed (is Docker running? can you pull images?). Dashboard: http://localhost:6333/dashboard will not load until Qdrant runs."
  fi
fi
QDRANT_OK=0
for i in $(seq 1 30); do
  if curl -sf http://localhost:6333/healthz >/dev/null; then QDRANT_OK=1; break; fi
  sleep 1
done
if [[ "${QDRANT_OK}" -eq 0 ]]; then
  echo -e "  ${ORANGE}⚠${RESET} Qdrant not reachable on :6333 after 30s — check: docker ps, docker logs nexuscore-qdrant"
fi

echo -e "  ${GREEN}→${RESET} Building Go loader..."
export GOTOOLCHAIN=local
(cd loader && go mod tidy && go build -o ../nexuscore-loader ./cmd/nexuscore-loader/)

POOL_SIZE=$(( $(nproc) * 4 ))
SURREAL_PID=$(pgrep -f surrealdb 2>/dev/null | head -1 || echo 0)
echo -e "  ${GREEN}→${RESET} Starting loader (SurrealDB PID: ${SURREAL_PID}, pool: ${POOL_SIZE})..."
nohup ./nexuscore-loader \
  -demo=true \
  -pid "${SURREAL_PID}" \
  -qdrant "http://localhost:6334" \
  -metrics ":9090" \
  -pool "${POOL_SIZE}" \
  -wasm ./cdc_component.wasm \
  > loader.log 2>&1 &
echo $! > .loader.pid
sleep 1
if ! curl -sf http://127.0.0.1:9090/metrics >/dev/null; then
  echo -e "  ${RED}✗${RESET} Loader did not expose metrics on :9090 — see loader.log:"
  tail -50 loader.log 2>/dev/null || true
  exit 1
fi

echo -e "\n${GREEN}Pipeline active.${RESET}"
echo -e "  Dashboard: http://localhost:9090/dashboard  (Start / Stop / Run pulse)"
echo -e "  Metrics:   http://localhost:9090/metrics"
echo -e "  Demo:      curl -sS -X POST http://localhost:9090/demo/pulse -H 'Content-Type: application/json' -d '{\"events\":500,\"upserts\":400}'"
echo -e "  Qdrant:    http://localhost:6333/dashboard"
echo -e "  Stop:      ${ROOT}/scripts/stop.sh"
