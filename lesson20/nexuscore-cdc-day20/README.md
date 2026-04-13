# NexusCore — Lesson 20 (CDC pipeline)

This folder is the **Day 20** CDC project: eBPF probe, WASI component, Go loader, scripts, dashboard, and housekeeping files live here together.

## Contents

| Path | Purpose |
|------|---------|
| `build_lesson20.py` | Regenerates / refreshes this tree (eBPF, Wasm component, loader, scripts). |
| `cleanup.sh` | Stops services, removes local caches (`venv`, `node_modules`, `.pytest_cache`, `__pycache__`, `*.pyc`), optional Istio-named paths, and prunes unused Docker objects. |
| `requirements.txt` | Python deps (stdlib-only for the scaffold; extend for tests/linters). |

## Quick start

```bash
./scripts/start.sh
```

- **Dashboard:** http://localhost:9090/dashboard  
- **Metrics:** http://localhost:9090/metrics  
- **Stop:** `./scripts/stop.sh`

## Regenerate / refresh files

```bash
python3 build_lesson20.py
```

## Cleanup

```bash
chmod +x cleanup.sh
./cleanup.sh
```

- Stops the Go loader and frees `:9090` (via `scripts/stop.sh` when present).  
- Stops Docker containers whose names match the lesson (`nexuscore*`, `nexuscore-qdrant`).  
- To stop **all** running Docker containers on the host: `STOP_ALL_RUNNING_CONTAINERS=1 ./cleanup.sh`  
- Runs `docker container prune`, `docker network prune`, `docker image prune -a`, and `docker builder prune`.

## Parent launcher

From the parent `lesson20/` directory, `setup.sh` runs this generator: `python3 nexuscore-cdc-day20/build_lesson20.py`.

## Security

Do not commit API keys or `.env` files with secrets. Use `.gitignore` and environment variables for any real credentials.
