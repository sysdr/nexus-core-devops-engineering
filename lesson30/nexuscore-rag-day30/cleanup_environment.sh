#!/usr/bin/env bash
# Full cleanup: stop NexusCore processes, Docker prune, remove caches / target / junk
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

log() { echo "[cleanup_environment] $*"; }

stop_project_services() {
  log "Stopping NexusCore / related processes..."
  pkill -f "target/release/nexuscore-host" 2>/dev/null || true
  pkill -f "nexuscore-host/target/release/nexuscore-host" 2>/dev/null || true
  pkill -f "cargo run --manifest-path nexuscore-host" 2>/dev/null || true
  pkill -f "ebpf/rag_loader" 2>/dev/null || true
  pkill -f "./rag_loader" 2>/dev/null || true
  if [[ -x "$SCRIPT_DIR/scripts/stop.sh" ]]; then
    "$SCRIPT_DIR/scripts/stop.sh" || true
  fi
  log "Process cleanup done."
}

docker_cleanup() {
  if ! command -v docker &>/dev/null; then
    log "docker not installed; skipping Docker steps."
    return 0
  fi
  log "Stopping running Docker containers..."
  if docker info &>/dev/null; then
    mapfile -t running < <(docker ps -q 2>/dev/null || true)
    if ((${#running[@]})); then
      docker stop "${running[@]}" 2>/dev/null || true
    else
      log "No running containers."
    fi
    log "Pruning unused Docker data..."
    docker container prune -f 2>/dev/null || true
    docker network prune -f 2>/dev/null || true
    docker volume prune -f 2>/dev/null || true
    docker image prune -af 2>/dev/null || true
    docker builder prune -af 2>/dev/null || true
    docker system prune -af --volumes 2>/dev/null || true
    log "Docker cleanup done."
  else
    log "Docker daemon not reachable; skipping Docker steps."
  fi
}

remove_project_artifacts() {
  log "Removing node_modules, Python envs, caches, .pyc, Istio-related files..."
  while IFS= read -r -d '' d; do
    rm -rf "$d"
  done < <(
    find "$SCRIPT_DIR" -depth -type d \( \
      -name node_modules -o -name venv -o -name .venv -o \
      -name .pytest_cache -o -name __pycache__ -o -name istio \
    \) -print0 2>/dev/null
  )
  find "$SCRIPT_DIR" -type f \( -name '*istio*.yaml' -o -name '*istio*.yml' \) -delete 2>/dev/null || true
  find "$SCRIPT_DIR" -name '*.pyc' -delete 2>/dev/null || true
  find "$SCRIPT_DIR" -name '*.pyo' -delete 2>/dev/null || true
  if [[ -d "$SCRIPT_DIR/target" ]]; then
    log "Removing Rust workspace target/..."
    rm -rf "$SCRIPT_DIR/target"
  fi
  log "Project artifact removal done."
}

main() {
  stop_project_services
  docker_cleanup
  remove_project_artifacts
  log "All cleanup steps finished."
}

main "$@"
