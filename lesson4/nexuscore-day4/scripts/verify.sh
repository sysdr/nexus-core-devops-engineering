#!/usr/bin/env bash
set -euo pipefail

pass() { echo -e "\033[0;32m  ✓ PASS\033[0m $1"; }
fail() { echo -e "\033[0;31m  ✗ FAIL\033[0m $1"; }
skip() { echo -e "\033[0;33m  - SKIP\033[0m $1"; }

echo "NexusCore Day 4 — Verification"
echo "─────────────────────────────────────"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if [[ -f "$ROOT/ebpf/nexuscore_xdp.bpf.o" ]]; then
  pass "eBPF object exists: nexuscore_xdp.bpf.o"
else
  skip "eBPF object not built (install libbpf-dev / linux-headers; make -C ebpf)"
fi

WASM_PATH="$ROOT/wasi-component/target/wasm32-wasip2/release/nexuscore_tenant_component.wasm"
if [[ -f "$WASM_PATH" ]]; then
  pass "WASM component exists: nexuscore_tenant_component.wasm ($(du -sh "$WASM_PATH" | cut -f1))"
else
  fail "WASM component not built"
fi

if [[ -f "$ROOT/control-plane/nexuscore-ctrl" ]]; then
  pass "Control plane binary: nexuscore-ctrl"
else
  fail "Control plane not built"
fi

if (cd "$ROOT/control-plane" && go test ./...); then
  pass "go test ./... in control-plane"
else
  fail "go test failed"
fi

if [[ -f "/sys/fs/bpf/nexuscore/schemas" ]]; then
  pass "BPF schema map pinned"
else
  skip "BPF schema map not pinned"
fi

if ip link show 2>/dev/null | grep -q "xdp"; then
  pass "XDP program attached"
else
  skip "XDP not attached"
fi

echo ""
echo "Visualizer: $ROOT/visualizer/index.html"
