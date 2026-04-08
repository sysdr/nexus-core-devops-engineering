.PHONY: build load demo verify stress cleanup stop test help

IFACE     ?= lo
BPFFS     ?= /sys/fs/bpf/nexuscore
WASM_OUT  := projection-engine/target/wasm32-wasip2/release/projection_engine.wasm

help:
	@echo ""
	@echo "  NexusCore Day 10 — Available Targets"
	@echo "  ─────────────────────────────────────────────────────────────"
	@echo "  make build    — Compile components (best-effort)"
	@echo "  make load     — Pin eBPF maps, attach XDP (requires root)"
	@echo "  make demo     — Run loader in demo mode + open visualizer"
	@echo "  make verify   — Check eBPF maps, XDP attachment, hit rate"
	@echo "  make stress   — Run Go stress tester (500k req/s, 10k tenants)"
	@echo "  make cleanup  — Detach XDP, unpin maps"
	@echo "  make stop     — Kill loader on metrics port (scripts/stop.sh)"
	@echo "  make test     — Smoke tests (no-zero metrics)"
	@echo "  ─────────────────────────────────────────────────────────────"
	@echo ""

test:
	@set -e; \
	echo "── [1/3] Python script syntax check"; \
	python3 -m py_compile scripts/demo_metrics.py; \
	echo "── [2/3] Dashboard file present"; \
	test -f visualizer/index.html; \
	echo "── [3/3] Metrics + dashboard on port"; \
	PIDFILE="$$PWD/.demo.pid" METRICS_PORT=8080 bash scripts/start.sh >/dev/null 2>&1; \
	sleep 0.4; \
	python3 -c "import json,urllib.request; m=json.load(urllib.request.urlopen('http://127.0.0.1:8080/metrics', timeout=2)); assert m['cache_hits']>0 and m['cache_misses']>0 and m['rebuilds_ok']>0, m; print('OK', m)"; \
	python3 -c "import urllib.request; b=urllib.request.urlopen('http://127.0.0.1:8080/', timeout=2).read().decode(); assert 'NexusCore' in b and '/metrics' in b; print('OK dashboard HTML')"; \
	bash scripts/stop.sh >/dev/null 2>&1 || true

stop:
	@bash scripts/stop.sh

build:
	@echo "── [1/3] Compiling WASI projection engine (optional)..."
	@command -v cargo >/dev/null 2>&1 && \
	  (cd projection-engine && cargo build --release --target wasm32-wasip2) || \
	  echo "⚠  Rust/WASI build skipped"
	@command -v wasm-tools >/dev/null 2>&1 && [ -f "$(WASM_OUT)" ] && wasm-tools validate "$(WASM_OUT)" || true

	@echo "── [2/3] Compiling eBPF XDP program..."
	$(MAKE) -C ebpf-xdp all 2>&1 | tail -10 || \
	  echo "⚠  eBPF compile skipped (needs clang-17 + kernel headers)"

	@echo "── [3/3] Compiling Rust loader..."
	@command -v cargo >/dev/null 2>&1 && \
	  (cargo build --release -p nexus-loader 2>&1 | tail -20) || \
	  echo "⚠  Rust loader build skipped"

load:
	@echo "Mounting bpffs and pinning maps..."
	sudo mkdir -p $(BPFFS)
	sudo mount -t bpf bpf $(BPFFS) 2>/dev/null || true
	@echo "Attaching XDP to $(IFACE)..."
	sudo ip link set dev $(IFACE) xdp obj ebpf-xdp/projection_xdp.bpf.o \
	     sec xdp 2>/dev/null || echo "⚠  XDP attach: need kernel ≥5.15 + CAP_BPF"
	@echo "✓ Load complete"

demo:
	@echo "Starting demo metrics server..."
	@echo "Dashboard: http://127.0.0.1:8080/"
	@echo "Metrics:   http://127.0.0.1:8080/metrics"
	@PIDFILE="$(PWD)/.demo.pid" METRICS_PORT=8080 bash scripts/start.sh

verify:
	@echo ""
	@echo "── Verification Report ─────────────────────────────────────"
	@echo ""
	@echo "[XDP attachment]"
	ip link show $(IFACE) | grep -o 'xdp[^ ]*' || echo "  No XDP attached (run make load as root)"
	@echo ""
	@echo "[eBPF maps]"
	ls -la $(BPFFS)/ 2>/dev/null || echo "  bpffs not mounted"
	@echo ""
	@echo "[Pinned map stats]"
	sudo bpftool map show pinned $(BPFFS)/proj_cache 2>/dev/null || echo "  proj_cache not pinned"
	@echo ""
	@echo "[Loader metrics]"
	curl -sf http://localhost:8080/metrics 2>/dev/null | python3 -m json.tool || \
	  echo "  Loader not running (start with: make demo)"
	@echo ""

stress:
	@echo "── Stress Test: 500k req/s target, $(TENANTS) tenants ──"
	cd stress && go run main.go \
	  --workers=64 \
	  --tenants=$(or $(TENANTS),10000) \
	  --duration=30 \
	  --target=127.0.0.1:9000 \
	  --metrics=http://127.0.0.1:8080/metrics

cleanup:
	@echo "Detaching XDP from $(IFACE)..."
	sudo ip link set dev $(IFACE) xdp off 2>/dev/null || true
	@echo "Unpinning eBPF maps..."
	sudo rm -rf $(BPFFS) 2>/dev/null || true
	@echo "✓ Cleanup complete"
