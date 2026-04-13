#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
echo "Stopping NexusCore CDC pipeline..."
if [[ -f .loader.pid ]]; then
  kill "$(cat .loader.pid)" 2>/dev/null || true
  rm -f .loader.pid
fi
pkill -f '[.]?/nexuscore-loader' 2>/dev/null || true
if command -v fuser &>/dev/null; then
  fuser -k 9090/tcp 2>/dev/null || true
fi
docker stop nexuscore-qdrant 2>/dev/null || true
sudo rm -rf /sys/fs/bpf/nexuscore 2>/dev/null || true
echo "Done."
