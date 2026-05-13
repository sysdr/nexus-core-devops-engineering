#!/usr/bin/env bash
# NexusCore Day 35 — stop mock services, clear local junk under this repo, optional Docker prune.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "[nexuscore] Stopping mock services..."

stop_pidfile() {
  local f="$1"
  local label="$2"
  if [ -f "$f" ]; then
    local pid
    pid="$(cat "$f" || true)"
    if [ -n "${pid:-}" ] && kill "$pid" 2>/dev/null; then
      echo "  ✓ $label stopped (pid $pid)"
    fi
    rm -f "$f"
  fi
}

stop_pidfile "$ROOT/.mock_llm.pid" "Mock LLM"
stop_pidfile "$ROOT/.search_server.pid" "Search server"
stop_pidfile "$ROOT/.viz_server.pid" "Visualizer"
stop_pidfile "$ROOT/.ebpf.pid" "eBPF loader"

if command -v fuser >/dev/null 2>&1; then
  echo "[nexuscore] Clearing listeners on 8765/9090/9876 (best effort)..."
  fuser -k 8765/tcp >/dev/null 2>&1 || true
  fuser -k 9090/tcp >/dev/null 2>&1 || true
  fuser -k 9876/tcp >/dev/null 2>&1 || true
fi

echo ""
echo "[nexuscore] Removing node_modules, venv, Python caches, *.pyc (under $ROOT)..."
while IFS= read -r -d '' d; do
  echo "  rm -rf $d"
  rm -rf "$d"
done < <(find "$ROOT" \( -type d -name node_modules -o -type d -name venv -o -type d -name .venv \
  -o -type d -name __pycache__ -o -type d -name .pytest_cache \) -print0 2>/dev/null || true)

while IFS= read -r -d '' f; do
  echo "  rm $f"
  rm -f "$f"
done < <(find "$ROOT" -type f \( -name '*.pyc' -o -name '*.pyo' \) -print0 2>/dev/null || true)

echo ""
echo "[nexuscore] Removing paths matching *istio* (name pattern)..."
while IFS= read -r -d '' p; do
  echo "  rm -rf $p"
  rm -rf "$p"
done < <(find "$ROOT" \( -iname '*istio*' \) -print0 2>/dev/null || true)

echo ""
echo "[nexuscore] Docker: stop running containers..."
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  ids="$(docker ps -q 2>/dev/null || true)"
  if [[ -n "${ids:-}" ]]; then
    docker stop $ids || true
  else
    echo "  (no running containers)"
  fi
else
  echo "  (skip: docker not installed or daemon unreachable)"
fi

echo ""
echo "[nexuscore] Docker: prune unused (containers, dangling images, networks, build cache)..."
if command -v docker >/dev/null 2>&1 && docker info >/dev/null 2>&1; then
  docker container prune -f || true
  docker image prune -f || true
  if [[ "${NEXUSCORE_IMAGE_PRUNE_ALL:-}" == "1" ]]; then
    echo "  NEXUSCORE_IMAGE_PRUNE_ALL=1 → docker image prune -a"
    docker image prune -a -f || true
  fi
  docker network prune -f || true
  docker volume prune -f || true
  docker builder prune -f || true
  echo "  Done."
else
  echo "  (skip)"
fi

echo ""
echo "[nexuscore] Removing empty files and empty directories (excluding target/, .git/)..."
find "$ROOT" \( -path '*/target/*' -o -path '*/target' -o -path '*/.git/*' -o -name .git \) -prune -o -type f -empty -delete 2>/dev/null || true
i=0
while [[ $i -lt 8 ]]; do
  n="$(find "$ROOT" \( -path '*/target/*' -o -path '*/target' -o -path '*/.git/*' -o -name .git \) -prune -o -depth -type d -empty -print 2>/dev/null | wc -l)"
  [[ "${n:-0}" -eq 0 ]] && break
  find "$ROOT" \( -path '*/target/*' -o -path '*/target' -o -path '*/.git/*' -o -name .git \) -prune -o -depth -type d -empty -delete 2>/dev/null || true
  i=$((i + 1))
done

echo ""
echo "[nexuscore] Cleanup complete."
