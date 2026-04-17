#!/usr/bin/env bash
set -euo pipefail
echo "[NexusCore] Running load test (Go goroutine swarm)..."
cd loadtest
go run main.go --concurrency "${1:-200}" --duration "${2:-30s}"
cd ..
