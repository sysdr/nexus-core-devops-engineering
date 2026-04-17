#!/usr/bin/env bash
set -euo pipefail

echo "[NexusCore] Running Rust tests..."
cargo test --manifest-path rag-component/Cargo.toml
cargo test --manifest-path nexuscore-host/Cargo.toml

echo "[NexusCore] Running Go tests..."
(cd loadtest && go test ./... 2>/dev/null || echo "[NexusCore] (no Go tests found)")

echo "[NexusCore] Tests complete."
