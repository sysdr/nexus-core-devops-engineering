# NexusCore Day 10 — Pre-Computed Read Projections

## Architecture

```
 Write Event
     │
     ▼
 kprobe ring buffer ─────────────────────────────────────────────┐
                                                                  │
                                                    WASI 0.3 Projection Engine
                                                    (projection-engine/src/lib.rs)
                                                                  │
                                                    Flatbuffer encode (4096B max)
                                                                  │
                                              ┌───────────────────▼──────────────────┐
                                              │  BPF_MAP_TYPE_LRU_HASH               │
                                              │  /sys/fs/bpf/nexuscore/proj_cache     │
                                              │  key: {tenant_id, projection_id}      │
                                              └───────────────────┬──────────────────┘
                                                                  │
 Inbound Query ──► XDP (port 9000) ──► map lookup ───────────────┤
                                           HIT (< 200ns)  ◄──────┘
                                           MISS ──► XDP_PASS ──► userspace rebuild
```

## Quick Start

```bash
# Prerequisites
rustup target add wasm32-wasip2
cargo install wasm-tools
apt-get install clang-17 libbpf-dev linux-headers-$(uname -r)

# Build everything
make build

# Run in demo mode (no root required)
make demo

# Open: http://127.0.0.1:8080/  (dashboard + /metrics on same port)

# With real eBPF (requires root + kernel ≥ 5.15)
sudo make load
make demo

# Stress test
make stress TENANTS=10000

# Verify
make verify

# Cleanup
sudo make cleanup
```

## File Structure

```
nexuscore-day10/
├── projection-engine/       WASI 0.3 Rust component
│   ├── Cargo.toml
│   ├── src/lib.rs           Pure Flatbuffer projection engine
│   └── wit/world.wit        WIT interface definition
├── ebpf-xdp/                eBPF CO-RE XDP program
│   ├── projection_xdp.bpf.c XDP handler + LRU cache
│   ├── projection_xdp.h     Shared map/header structs
│   └── Makefile
├── loader/                  Rust userspace loader
│   └── src/main.rs          wasmtime embedder + libbpf-rs
├── stress/                  Go stress tester
│   └── main.go
├── visualizer/
│   └── index.html           Live metrics dashboard
├── scripts/
│   ├── start.sh
│   └── quick-verify.sh
├── Cargo.toml               Workspace
└── Makefile
```

## Key Invariants

1. **Cache hits never leave the kernel** — XDP returns < 200ns.
2. **WASI components are shared-nothing** — one linear memory per tenant instance, zero TLB cross-contamination.
3. **The Flatbuffer layout is the ABI** — eBPF reads it directly, no deserialization in the kernel.
4. **Map pinning = explicit ownership** — loader holds fd with CAP_BPF, WASI component has zero kernel privileges.
