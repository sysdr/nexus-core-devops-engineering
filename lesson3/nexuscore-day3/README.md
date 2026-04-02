# NexusCore Day 3 — Document-graph hybrid (demo workspace)

## Quick start

```bash
./scripts/verify.sh    # unit + host smoke tests
./scripts/demo.sh      # tests + regenerate graph.blob + print dashboard path
./scripts/start.sh     # build, graph.blob, run synthetic host (Ctrl+C to stop)
./scripts/stress.sh 5 80000   # host for 5s at 80k events/s (requires `timeout`)
```

## Dashboard

With the host running, open **http://127.0.0.1:9847/** (override with `NEXUSCORE_DASHBOARD_PORT`). The UI loads `data/visualizer.html` and polls **`/api/metrics`** for Lesson 3 fields: CSR query rate, p99 BFS latency, tenant-slot activity, arena fragmentation; TLB and XDP are labeled as not attached in this build (per lesson, measure with `perf` / loaded XDP).

## Layout

- `src/nexuscore_graph` — CSR engine + unit tests
- `src/host` — Tokio demo host (`NEXUSCORE_RPS` env)
- `scripts/` — lifecycle helpers
- `data/` — `graph.blob`, `visualizer.html`
- `wit/` — reference WIT (narrative / future Wasm work)
