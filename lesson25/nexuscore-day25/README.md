# Lesson 25 — NexusCore (Day 25 workspace)

This directory holds the **Day 25** generator (`setup.sh`) and the generated Rust workspace under `nexuscore-day25/` (after you run `./setup.sh`).

## Quick reference

| Item | Purpose |
|------|---------|
| `setup.sh` | Generates the `nexuscore-day25/` Rust workspace (host, dashboard, eBPF sources, scripts). |
| `cleanup.sh` | Stops local NexusCore processes, stops **all** running Docker containers, prunes unused Docker objects, and removes common cache dirs (`node_modules`, `venv`, `.pytest_cache`, `__pycache__`, `*.pyc`, Istio-named paths) across the **parent repository**. |
| `requirements.txt` | Optional Python dependencies for tooling (see comments inside). |
| `.gitignore` | Ignores build artifacts, secrets, caches, and generated files. |

## Security

- Do **not** commit real API keys, tokens, or `.env` files with secrets.
- Prefer environment variables and `.env.example` (without real values) for documentation.

## Cleanup

From this directory:

```bash
chmod +x cleanup.sh
./cleanup.sh
```

**Warning:** `./cleanup.sh` stops **every** running Docker container on this machine and runs aggressive `docker image prune -af` and related prunes. Use only when you intend to reclaim disk space and stop all containers.
