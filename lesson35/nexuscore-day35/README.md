# NexusCore Day 35 — ReAct Agents: WASI 0.3 + eBPF CO-RE

## Quickstart

```bash
./scripts/verify.sh
./scripts/start.sh          # mock LLM :9876, mock search :8765, visualizer :9090
./scripts/demo.sh "What is eBPF?"
curl -s http://127.0.0.1:8080/metrics | head
./scripts/cleanup.sh
```

Set **`ANTHROPIC_API_KEY`** in the environment to use the real Anthropic API; otherwise demos use the local mock on **`http://127.0.0.1:9876/v1/messages`**. Do not commit API keys (see `.gitignore` for `.env`).

## Layout

| Path | Role |
|------|------|
| `agent-component/` | WASI component (Rust + WIT) |
| `host/` | Wasmtime host, `/metrics`, `/api/viz` |
| `visualizer/` | Static UI; polls live API and falls back to `visualizer/last-viz.json` after a run |
| `scripts/` | `start.sh`, `demo.sh`, `loadtest.sh`, `verify.sh`, **`cleanup.sh`** (full teardown) |

## Dashboard

With **`./scripts/start.sh`**, open **`http://127.0.0.1:9090`**. While **`./scripts/demo.sh`** runs, the host serves **`http://127.0.0.1:8080/api/viz`**; when the host exits, the page reads **`./last-viz.json`** written under `visualizer/`.

## Full cleanup (`./scripts/cleanup.sh`)

1. Stops mock LLM, search, visualizer and clears ports **8765 / 9090 / 9876**.
2. Deletes **`node_modules`**, **`venv`** / **`.venv`**, **`__pycache__`**, **`.pytest_cache`**, **`*.pyc`** under this tree only.
3. Removes paths whose names match **`*istio*`**.
4. If Docker is available: stops **all** running containers, then **`docker container prune`**, **`docker image prune`** (dangling; use **`NEXUSCORE_IMAGE_PRUNE_ALL=1`** for **`docker image prune -a`**), plus network / volume / builder prune.

## Prometheus metrics

- `nexuscore_step_latency_seconds`
- `nexuscore_agent_latency_seconds`

## Parent folder

If you checked out the course repo with an outer **`lesson35/`** folder, run **`./setup.sh`** there once to verify paths, then work entirely inside **`nexuscore-day35/`**.
