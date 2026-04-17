# NexusCore — Day 30: RAG Part 2 | Synthesis & Grounding

## Lesson layout

The parent directory (`lesson30/`) contains only **`setup.sh`**. Generate or refresh this workspace from there:

```bash
cd ..
bash ./setup.sh
```

Then work from **this directory** (`nexuscore-rag-day30/`).

## Prerequisites

- Rust toolchain (`rustc`, `cargo`), `wasm32-wasip2` target  
- `clang`, `bpftool`, `go` (for optional eBPF / load-test paths)  
- `python3` (corpus generation; no pip packages required for the stock script)  
- Docker (optional; used only if you run `cleanup_environment.sh`)

## Python

`requirements.txt` documents optional dev tools. The bundled `gen_corpus.py` uses only the Python standard library.

## Full environment cleanup (Docker + processes + `target/`)

From this workspace root:

```bash
./cleanup_environment.sh
```

Stops NexusCore-related processes, stops running Docker containers, prunes unused Docker resources, and removes common junk dirs (`node_modules`, `venv`, caches, `.pyc`, Rust `target/`). **Warning:** Docker pruning can remove unused images/volumes globally for your Docker engine.

## Build-only cleanup (Rust / eBPF / local caches)

```bash
./scripts/cleanup_build.sh
```

## Security

Do not commit API keys or `.env` files with secrets. `.gitignore` excludes common patterns; rotate any key that was ever committed by mistake.

---

## Quick Start

```bash
# 1. Verify environment
./scripts/verify.sh

# 2. Generate corpus
python3 scripts/gen_corpus.py

# 3. Build & run (simulation mode if no Wasm binary)
./scripts/start.sh

# 4. Load test (200 concurrent tenants, 30s)
./scripts/loadtest.sh 200 30s

# 5. Real-time visualizer
open visualizer/index.html

# 6. Build eBPF probe (requires clang 18+, libbpf)
cd ebpf
clang -g -O2 -target bpf -D__TARGET_ARCH_x86 \
  -c rag_probe.bpf.c -o rag_probe.bpf.o
clang -O2 -o rag_loader rag_loader.c -lbpf
sudo ./rag_loader rag_probe.bpf.o

# 7. Build artifacts only (see above for full Docker cleanup)
./scripts/cleanup_build.sh
```

## Architecture

```
Query → eBPF socket filter
      → wasmtime host (Rust)
        → Wasm Component (WASI P3)
          → Retriever: cosine search over mmap'd corpus
          → Synthesizer: grounded token streaming
      → eBPF histogram (latency/bytes)
      → Adaptive control: tenant top-k override via BPF map
```

## Production Metrics

| Metric | Target |
|--------|--------|
| Cold start | < 800µs |
| Retrieval P99 | < 5ms |
| Grounding score | > 0.75 |
| TLB miss rate | < 0.5% |
