#!/usr/bin/env bash
set -euo pipefail
echo "[NexusCore] Stopping NexusCore processes (if any)..."

# Host runtime (workspace build: ./target/release/nexuscore-host)
pkill -f "target/release/nexuscore-host" 2>/dev/null || true
pkill -f "nexuscore-host/target/release/nexuscore-host" 2>/dev/null || true
pkill -f "cargo run --release --" 2>/dev/null || true

# eBPF loader (if user ran it)
pkill -f "ebpf/rag_loader" 2>/dev/null || true
pkill -f "./rag_loader" 2>/dev/null || true

echo "[NexusCore] Stop complete."
