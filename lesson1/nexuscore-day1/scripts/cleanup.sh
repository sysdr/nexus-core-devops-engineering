#!/usr/bin/env bash
echo "[nexuscore] Cleaning up..."
docker stop nexuscore-surreal 2>/dev/null || true
docker rm   nexuscore-surreal 2>/dev/null || true
sudo rm -f /sys/fs/bpf/nexuscore_* 2>/dev/null || true
cargo clean 2>/dev/null || true
echo "[ok ] Cleanup complete"
