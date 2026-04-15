#!/usr/bin/env bash
# NexusCore lesson25 — stop local services and prune Docker / project caches.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# This script lives in lesson25/nexuscore-day25/. Repo root is three levels up.
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

info()  { echo "[cleanup] $*"; }
warn()  { echo "[cleanup] WARN: $*" >&2; }

info "Repository root: $REPO_ROOT"

# --- Stop host processes (metrics / simulator) ---
info "Stopping NexusCore host processes (best effort)…"
fuser -k 9090/tcp 2>/dev/null || true
pkill -f 'target/release/nexuscore' 2>/dev/null || true
pkill -f 'target/debug/nexuscore' 2>/dev/null || true
pkill -f 'cargo run.*nexuscore-host' 2>/dev/null || true

# --- Docker: stop all running containers ---
if command -v docker >/dev/null 2>&1; then
  if docker info >/dev/null 2>&1; then
    RUNNING="$(docker ps -q 2>/dev/null || true)"
    if [[ -n "${RUNNING:-}" ]]; then
      info "Stopping running Docker containers…"
      docker stop $RUNNING
    else
      info "No running Docker containers."
    fi

    info "Removing stopped containers…"
    docker container prune -f

    info "Pruning unused Docker images, networks, build cache…"
    docker image prune -af
    docker network prune -f
    docker builder prune -af

    info "Pruning unused Docker volumes (not used by any container)…"
    docker volume prune -f

    info "Docker disk usage after prune:"
    docker system df || true
  else
    warn "Docker daemon not reachable; skipping Docker steps."
  fi
else
  warn "docker not installed; skipping Docker steps."
fi

# --- Project caches: Python / Node / Istio-like paths ---
info "Removing Python/Node caches and Istio-like paths under $REPO_ROOT…"

while IFS= read -r -d '' dir; do
  info "  rmdir: $dir"
  rm -rf "$dir"
done < <(find "$REPO_ROOT" \( \
    -type d -name node_modules \
    -o -type d -name venv \
    -o -type d -name .venv \
    -o -type d -name .pytest_cache \
    -o -type d -name __pycache__ \
    -o -type d -iname '*istio*' \
  \) -print0 2>/dev/null || true)

while IFS= read -r -d '' f; do
  info "  rm: $f"
  rm -f "$f"
done < <(find "$REPO_ROOT" -type f \( -name '*.pyc' -o -name '*.pyo' \) -print0 2>/dev/null || true)

while IFS= read -r -d '' f; do
  info "  rm istio file: $f"
  rm -f "$f"
done < <(find "$REPO_ROOT" -type f \( -iname '*istio*.yaml' -o -iname '*istio*.yml' \) -print0 2>/dev/null || true)

# Optional: drop Rust build tree for this workspace (large)
if [[ -d "$SCRIPT_DIR/target" ]]; then
  info "Removing $SCRIPT_DIR/target …"
  rm -rf "$SCRIPT_DIR/target"
fi

info "Done."
