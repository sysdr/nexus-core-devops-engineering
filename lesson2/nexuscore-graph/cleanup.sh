#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

echo "[cleanup] stopping nexuscore-host processes (best-effort)..."
pkill -f "target/release/nexuscore-host" 2>/dev/null || true
pkill -x "nexuscore-host" 2>/dev/null || true

echo "[cleanup] stopping local dashboard servers (best-effort)..."
for pidfile in /tmp/nexuscore_dashboard_*.pid; do
  [[ -f "$pidfile" ]] || continue
  pid="$(cat "$pidfile" 2>/dev/null || true)"
  if [[ -n "${pid:-}" ]]; then
    kill "$pid" 2>/dev/null || true
  fi
  rm -f "$pidfile" 2>/dev/null || true
done
pkill -f "python3 -m http.server" 2>/dev/null || true

echo "[cleanup] stopping docker containers (best-effort)..."
if command -v docker >/dev/null 2>&1; then
  docker ps -q | xargs -r docker stop >/dev/null 2>&1 || true
  docker ps -aq | xargs -r docker rm -f >/dev/null 2>&1 || true

  echo "[cleanup] pruning unused docker resources (best-effort)..."
  docker system prune -af --volumes >/dev/null 2>&1 || true
  docker image prune -af >/dev/null 2>&1 || true
else
  echo "[cleanup] docker not found; skipping docker cleanup"
fi

echo "[cleanup] removing local junk (node_modules/venv/caches/targets)..."
shopt -s globstar nullglob

rm -rf **/node_modules **/venv **/.venv **/.pytest_cache **/__pycache__ **/*.pyc **/*.pyo 2>/dev/null || true
rm -rf **/target 2>/dev/null || true

echo "[cleanup] removing istio artifacts (best-effort)..."
rm -rf **/istio **/istio-* **/*istio* 2>/dev/null || true

echo "[cleanup] done"

