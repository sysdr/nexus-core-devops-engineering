#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

echo "== Stop Day10 demo services =="
if [ -x "$ROOT/scripts/stop.sh" ]; then
  bash "$ROOT/scripts/stop.sh" || true
fi

echo ""
echo "== Stop all Docker containers =="
if command -v docker >/dev/null 2>&1; then
  docker ps -q | xargs -r docker stop || true

  echo ""
  echo "== Remove stopped containers =="
  docker ps -aq | xargs -r docker rm -f || true

  echo ""
  echo "== Prune unused Docker resources (images/networks/build cache) =="
  docker system prune -af --volumes || true
else
  echo "docker not found; skipping Docker cleanup"
fi

echo ""
echo "== Remove common build/cache artifacts in nexuscore-day10 =="
rm -rf \
  node_modules \
  .venv \
  venv \
  .pytest_cache \
  __pycache__ \
  target \
  .demo.pid \
  2>/dev/null || true

echo ""
echo "== Remove *.pyc files =="
python3 - <<'PY' || true
import os

root = os.path.abspath(os.getcwd())
removed = 0
for dirpath, _dirnames, filenames in os.walk(root):
    for fn in filenames:
        if fn.endswith(".pyc"):
            p = os.path.join(dirpath, fn)
            try:
                os.remove(p)
                removed += 1
            except OSError:
                pass
print(f"Removed {removed} .pyc files")
PY

echo ""
echo "Cleanup complete."
