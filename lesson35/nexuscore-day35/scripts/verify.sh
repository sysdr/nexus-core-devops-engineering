#!/usr/bin/env bash
set -euo pipefail
echo "── NexusCore Day 35: Verification ──────────────────────"
echo ""

echo -n "[1] wasm32-wasip2 target installed: "
rustup target list --installed | grep -q wasm32-wasip2 && echo "✓" || echo "✗ (run: rustup target add wasm32-wasip2)"

echo -n "[2] cargo-component available: "
if cargo component --version >/dev/null 2>&1; then
  echo "✓ ($(cargo component --version))"
else
  echo "✗ (run: cargo install cargo-component)"
fi

echo -n "[3] clang for eBPF: "
command -v clang >/dev/null 2>&1 && echo "✓ ($(clang --version | head -1))" || echo "✗ (optional; install: sudo apt install clang)"

echo -n "[4] BTF vmlinux present: "
[ -f /sys/kernel/btf/vmlinux ] && echo "✓" || echo "✗ (optional; need kernel with CONFIG_DEBUG_INFO_BTF=y)"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
echo -n "[5] Wasm component builds: "
if (cd "$ROOT/agent-component" && cargo component build --release >/tmp/nx35-wasm-build.log 2>&1); then
  echo "✓"
else
  echo "✗ (see /tmp/nx35-wasm-build.log)"
fi

echo -n "[6] Host binary builds: "
if (cd "$ROOT/host" && cargo build --release >/tmp/nx35-host-build.log 2>&1); then
  echo "✓"
else
  echo "✗ (see /tmp/nx35-host-build.log)"
fi

echo -n "[7] Mock search server reachable: "
if curl -sf -X POST http://127.0.0.1:8765/search -H "Content-Type: application/json" -d '{"q":"rust"}' >/dev/null 2>&1; then
  echo "✓"
else
  echo "✗ (run: ./scripts/start.sh first)"
fi

echo -n "[8] Mock LLM reachable: "
if curl -sf -X POST http://127.0.0.1:9876/v1/messages \
  -H "Content-Type: application/json" \
  -d '{"model":"mock","max_tokens":64,"messages":[{"role":"user","content":"ping"}]}' >/dev/null 2>&1; then
  echo "✓"
else
  echo "✗ (run: ./scripts/start.sh first)"
fi

echo ""
echo "── Done ────────────────────────────────────────────────"
