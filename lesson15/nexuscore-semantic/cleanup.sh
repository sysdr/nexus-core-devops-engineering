#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"

echo "[cleanup] Stopping lesson services (ports 8080/9090/9091)..."
for p in 8080 9090 9091; do
  pid="$(ss -ltnp 2>/dev/null | awk -v port=":$p" '$4 ~ port {print $0}' | sed -n 's/.*pid=\\([0-9][0-9]*\\).*/\\1/p' | head -n 1)"
  if [ -n "${pid:-}" ] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    sleep 0.2
    kill -9 "$pid" 2>/dev/null || true
  fi
done

if [ -x "$ROOT/nexuscore-semantic/scripts/cleanup.sh" ]; then
  bash "$ROOT/nexuscore-semantic/scripts/cleanup.sh" || true
fi

echo "[cleanup] Stopping all Docker containers..."
if command -v docker >/dev/null 2>&1; then
  docker ps -q | xargs -r docker stop || true
  docker ps -aq | xargs -r docker rm -f || true

  echo "[cleanup] Pruning unused Docker resources (images/containers/networks/build cache)..."
  docker system prune -af || true
  docker volume prune -f || true
else
  echo "[cleanup] docker not found; skipping container cleanup"
fi

echo "[cleanup] Removing caches/artifacts..."
rm -rf \
  "$ROOT/nexuscore-semantic/.cargo-target" \
  "$ROOT/nexuscore-semantic/target" \
  "$ROOT/nexuscore-semantic/ebpf-ingester/bin" \
  "$ROOT/nexuscore-semantic/data/tweets.jsonl" \
  2>/dev/null || true

echo "[cleanup] Done."

