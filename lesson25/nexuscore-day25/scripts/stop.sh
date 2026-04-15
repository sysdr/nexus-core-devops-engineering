#!/usr/bin/env bash
set -euo pipefail

echo "Stopping Redpanda..."
docker rm -f nexuscore-redpanda 2>/dev/null || true

echo "Killing anything on port 9090..."
fuser -k 9090/tcp 2>/dev/null || true

echo "Stopping NexusCore host (best effort)..."
pkill -f 'nexuscore-host' 2>/dev/null || true
pkill -f 'target/release/nexuscore' 2>/dev/null || true

echo "Done."
