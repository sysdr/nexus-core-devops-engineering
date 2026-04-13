#!/usr/bin/env bash
# NexusCore lesson 20 — stop local services and prune Docker + Python/Node junk under this tree.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$ROOT"

RED='\033[0;31m'; GREEN='\033[0;32m'; BLUE='\033[0;34m'; YELLOW='\033[0;33m'; RESET='\033[0m'
info() { echo -e "${BLUE}→${RESET} $*"; }
ok() { echo -e "${GREEN}✓${RESET} $*"; }
warn() { echo -e "${YELLOW}!${RESET} $*"; }

info "Stopping NexusCore CDC pipeline (loader, ports)…"
if [[ -f "$ROOT/scripts/stop.sh" ]]; then
  (cd "$ROOT" && bash scripts/stop.sh) || true
else
  warn "scripts/stop.sh not found — killing loader by pattern"
  pkill -f '[.]?/nexuscore-loader' 2>/dev/null || true
  if command -v fuser &>/dev/null; then
    fuser -k 9090/tcp 2>/dev/null || true
  fi
fi

# Optional: stop every running Docker container (set STOP_ALL_RUNNING_CONTAINERS=1)
if [[ "${STOP_ALL_RUNNING_CONTAINERS:-0}" == "1" ]]; then
  warn "STOP_ALL_RUNNING_CONTAINERS=1 — stopping all running Docker containers"
  if command -v docker &>/dev/null; then
    mapfile -t _ids < <(docker ps -q 2>/dev/null) || true
    if ((${#_ids[@]})); then
      docker stop "${_ids[@]}" 2>/dev/null || true
    fi
  fi
else
  info "Stopping lesson-related Docker containers (names matching nexuscore / nexuscore-qdrant)…"
  if command -v docker &>/dev/null; then
    while IFS= read -r name; do
      [[ -z "$name" ]] && continue
      docker stop "$name" 2>/dev/null || true
    done < <(docker ps --format '{{.Names}}' 2>/dev/null | grep -iE 'nexuscore' || true)
    docker stop nexuscore-qdrant 2>/dev/null || true
  fi
fi

info "Removing Python/Node caches and Istio-like paths under ${ROOT}…"
# shellcheck disable=SC2038
while IFS= read -r -d '' dir; do
  rm -rf "$dir"
  ok "removed dir: $dir"
done < <(find "$ROOT" \( \
  -type d -name node_modules -o \
  -type d -name venv -o \
  -type d -name .venv -o \
  -type d -name __pycache__ -o \
  -type d -name .pytest_cache -o \
  -type d -iname '*istio*' \
\) -print0 2>/dev/null || true)

while IFS= read -r -d '' f; do
  rm -f "$f"
done < <(find "$ROOT" -type f \( -name '*.pyc' -o -name '*.pyo' \) -print0 2>/dev/null || true)
ok "removed *.pyc / *.pyo files (if any)"

# Files with istio in the name (not already removed with dirs)
while IFS= read -r -d '' p; do
  rm -f "$p"
  ok "removed: $p"
done < <(find "$ROOT" -type f \( -iname '*istio*' \) -print0 2>/dev/null || true)

if command -v docker &>/dev/null; then
  info "Docker: pruning stopped containers, unused networks, dangling images…"
  docker container prune -f 2>/dev/null || true
  docker network prune -f 2>/dev/null || true
  docker image prune -a -f 2>/dev/null || true
  docker builder prune -af 2>/dev/null || true
  ok "docker prune complete (builder + images may free significant disk)"
else
  warn "docker not in PATH — skipped Docker prune"
fi

ok "Cleanup finished."
