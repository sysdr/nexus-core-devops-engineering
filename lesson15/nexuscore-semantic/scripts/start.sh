#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [ -f /tmp/nexuscore-host.pid ] || [ -f /tmp/nexuscore-ingester.pid ] || [ -f /tmp/nexuscore-sink.pid ]; then
  echo "[start] Existing PID files found; attempting cleanup first..."
  bash "$ROOT/scripts/cleanup.sh" || true
fi

echo "[start] Generating 100K tweet dataset..."
python3 "$ROOT/data/gen_tweets.py" 100000 "$ROOT/data/tweets.jsonl"

echo "[start] Building WASI component (best-effort)..."
if command -v cargo >/dev/null 2>&1 && cargo component --version >/dev/null 2>&1; then
  (cd "$ROOT/semantic-index" && CARGO_TARGET_DIR="$ROOT/.cargo-target" timeout 90s cargo component build --release --target wasm32-wasip2) || \
    echo "[start][warn] WASI component build failed; continuing (metrics/demo still works)"
else
  echo "[start][warn] 'cargo component' not available; skipping WASI component build"
fi

echo "[start] Building eBPF ingester (requires clang + bpf headers)..."
cd "$ROOT/ebpf-ingester" && mkdir -p bin && go build -o bin/ingester ./src/

echo "[start] Building host runtime (best-effort)..."
HOST_BUILT=0
if command -v cargo >/dev/null 2>&1; then
  if [ "${NEXUS_SKIP_HOST_BUILD:-0}" = "1" ]; then
    echo "[start][warn] NEXUS_SKIP_HOST_BUILD=1; skipping host runtime build"
  else
  (cd "$ROOT/host-runtime" && CARGO_TARGET_DIR="$ROOT/.cargo-target" timeout 120s cargo build --release) && HOST_BUILT=1 || \
    echo "[start][warn] host runtime build failed; continuing (metrics/demo still works)"
  fi
else
  echo "[start][warn] cargo not found; skipping host runtime build"
fi

if [ "$HOST_BUILT" -eq 1 ] && [ -x "$ROOT/host-runtime/target/release/host-runtime" ]; then
  echo "[start] Launching host runtime..."
  NEXUS_WASM="$ROOT/semantic-index/target/wasm32-wasip2/release/semantic_index.wasm" \
  NEXUS_HOST_SOCK="/tmp/nexuscore-host.sock" \
  "$ROOT/host-runtime/target/release/host-runtime" &
  echo $! > /tmp/nexuscore-host.pid
  sleep 1
else
  echo "[start][warn] host runtime not available; ingester will still expose metrics"
  echo "[start] Launching mock host (for forwarded metrics)..."
  NEXUS_HOST_SOCK="/tmp/nexuscore-host.sock" python3 "$ROOT/scripts/mock_host.py" >/tmp/nexuscore-mockhost.log 2>&1 &
  echo $! > /tmp/nexuscore-mockhost.pid
  sleep 0.2
fi

echo "[start] Starting ingester (tries XDP; falls back to TCP mode if needed)..."
NEXUS_MODE=tcp NEXUS_IFACE=lo NEXUS_HOST_SOCK=/tmp/nexuscore-host.sock \
  "$ROOT/ebpf-ingester/bin/ingester" &
echo $! > /tmp/nexuscore-ingester.pid

chmod +x "$ROOT/scripts/dashboard.sh" >/dev/null 2>&1 || true
echo "[start] Dashboard:"
echo "        bash \"$ROOT/scripts/dashboard.sh\"   # then open http://localhost:8080/"
