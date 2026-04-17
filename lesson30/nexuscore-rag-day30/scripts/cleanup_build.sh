#!/usr/bin/env bash
set -euo pipefail
echo "[NexusCore] Cleaning build artifacts, caches, and temp files..."

# Rust build artifacts (workspace: ./target; legacy per-crate dirs if any)
rm -rf target rag-component/target nexuscore-host/target

# eBPF build artifacts
rm -f ebpf/rag_probe.bpf.o ebpf/rag_probe.skel.h ebpf/rag_loader

# Common caches / temp
rm -rf .pytest_cache .mypy_cache .ruff_cache .cache __pycache__
find . -name "*.pyc" -delete 2>/dev/null || true
find . -name "*.log" -delete 2>/dev/null || true

# Node / Python envs (requested)
rm -rf node_modules venv .venv

echo "[NexusCore] Clean complete."
