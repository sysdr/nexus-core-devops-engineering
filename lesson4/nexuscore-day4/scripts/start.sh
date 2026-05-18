#!/usr/bin/env bash
# NexusCore Day 4 — Start Script (build all; eBPF optional if clang missing)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

if [[ -x "${HOME}/.local/go/bin/go" ]]; then
  export PATH="${HOME}/.local/go/bin:${PATH}"
fi

echo "[nexuscore] Building eBPF program..."
if ! make -C "$ROOT/ebpf" all; then
  echo "[warn] eBPF build failed (install clang, kernel headers). Continuing with WASI + Go."
fi

echo "[nexuscore] Building WASI component..."
cd "$ROOT/wasi-component"
command -v rustup >/dev/null 2>&1 && rustup target add wasm32-wasip2 2>/dev/null || true
cargo build --release --target wasm32-wasip2
WASM="target/wasm32-wasip2/release/nexuscore_tenant_component.wasm"
if [[ -f "$WASM" ]]; then
  echo "[nexuscore] Wasm component: $(du -sh "$WASM" | cut -f1)"
else
  echo "[warn] Expected $WASM missing after build"
fi

echo "[nexuscore] Building Go control plane..."
cd "$ROOT/control-plane"
go mod tidy
go build -o nexuscore-ctrl .

echo ""
echo "[nexuscore] All build steps finished."
echo "[nexuscore] Visualizer: file://$ROOT/visualizer/index.html"
