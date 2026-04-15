#!/usr/bin/env bash
set -euo pipefail

echo "=== NexusCore Day 25 — Verification ==="
echo

echo "1. Rust compilation..."
cargo check --all 2>&1 | tail -3 && echo "   OK"

echo "2. Wasm target available..."
rustup target list --installed | grep wasm32-wasip2 && echo "   OK" || echo "   MISSING — run: rustup target add wasm32-wasip2"

echo "3. Redpanda reachable..."
docker exec nexuscore-redpanda rpk cluster health 2>/dev/null && echo "   OK" || echo "   NOT RUNNING — run: scripts/start.sh"

echo "4. Metrics endpoint..."
curl -s --max-time 2 http://localhost:9090/metrics | grep nexuscore | head -5 || echo "   Host not running — start it first"

echo "5. BPF probe (requires root)..."
if [[ $(id -u) -eq 0 ]]; then
    bpftool prog show | grep nexuscore && echo "   Probe loaded" || echo "   Not loaded (run: make -C ebpf && sudo nexuscore)"
else
    echo "   Skip (not root) — run verify.sh as root for eBPF check"
fi

echo
echo "=== Verification complete ==="
