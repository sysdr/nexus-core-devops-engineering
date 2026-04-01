# NexusCore Day 1: SQL → SurrealDB Multi-Model Migration

## Project Structure
```
nexuscore-day1/
├── host/                   # Rust host runtime (single OS thread, shared pool)
│   └── src/
│       ├── main.rs         # CLI: start | demo | verify | stress
│       ├── pool.rs         # bb8 connection pool (single pool for ALL tenants)
│       ├── adapter.rs      # WIT interface host implementation
│       ├── tenant.rs       # Wasm component lifecycle management
│       ├── metrics.rs      # Prometheus metrics on :9090/metrics
│       └── visualizer.rs   # Live CLI dashboard (raw ANSI, no TUI dep)
├── guest-component/        # Rust WASM component (wasm32-wasip2)
│   └── src/lib.rs          # Tenant isolation boundary, shared-nothing
├── ebpf/
│   ├── tenant_latency.bpf.c  # eBPF CO-RE probe: kernel-space latency tracking
│   └── Makefile
├── wit/
│   └── surreal-adapter.wit   # WASI 0.3 component model interface
├── viz/
│   └── index.html            # Metrics dashboard (also served at :9090/)
├── lesson_article.md         # Lesson overview
├── bench/
│   └── README.txt
├── scripts/
│   ├── start.sh            # Start SurrealDB + host
│   ├── demo.sh             # Live visualizer demo
│   ├── verify.sh           # System verification
│   ├── bench.sh            # Stress test
│   ├── cleanup.sh          # Teardown
│   ├── load_ebpf.sh        # eBPF loader (requires root)
│   ├── test.sh             # cargo test wrapper
│   └── migration_patterns.surql  # SQL → SurrealDB pattern reference
└── Cargo.toml              # Workspace
```

## Quick Start
```bash
# 1. Start SurrealDB (Docker)
docker run -d -p 8000:8000 surrealdb/surrealdb:latest start --user root --pass root memory

# 2. Start the host runtime
./scripts/start.sh

# 3. Watch live demo (in another terminal)
./scripts/demo.sh 20 100    # 20 tenants, 100 RPS

# 4. Verify system state
./scripts/verify.sh

# 5. Load eBPF probe (Linux, requires root)
sudo ./scripts/load_ebpf.sh

# 6. Stress test
./scripts/bench.sh 500 30   # 500 tenants, 30s soak
```

## Key Design Decisions

### Why single OS thread?
`#[tokio::main(flavor = "current_thread")]` on the host.
10K tenants sharing 1 OS thread = zero TLB shootdown IPIs.
Wasm components yield cooperatively (no preemption = no scheduler thrashing).

### Why one connection pool for all tenants?
64 connections serving 10K tenants = 99.4% socket reduction vs naive approach.
Tenant isolation is at the WASM boundary, not the TCP boundary.

### Why CBOR instead of JSON?
CBOR is ~30% smaller and parses without string allocation.
SurrealDB speaks CBOR natively — no JSON serialization round-trip.

### Why pin the eBPF map?
`LIBBPF_PIN_BY_NAME` makes the map survive host process restarts.
Your self-healing controller restarts the host; eBPF state is preserved.

## Metrics
- Dashboard (HTML): http://127.0.0.1:9090/
- Prometheus text: http://127.0.0.1:9090/metrics
- Key metrics: `surreal_query_total`, `surreal_query_latency_seconds`, `surreal_pool_connections_used`
- Run `./scripts/demo.sh` while the host is running so counters and histograms move off zero.

## Migration Reference
See `scripts/migration_patterns.surql` for SQL → SurrealDB pattern translations.
