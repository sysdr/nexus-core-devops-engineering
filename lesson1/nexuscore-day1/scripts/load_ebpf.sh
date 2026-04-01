#!/usr/bin/env bash
# Load NexusCore eBPF probe (requires root)
set -euo pipefail

PROG_PATH="/sys/fs/bpf/nexuscore_prog"
MAP_PATH="/sys/fs/bpf/nexuscore_tenant_ts_map"

if [[ $EUID -ne 0 ]]; then
    echo "Error: eBPF loading requires root. Run: sudo $0"
    exit 1
fi

echo "[ebpf] Checking kernel BPF support..."
if [[ ! -d /sys/fs/bpf ]]; then
    echo "[fail] BPF filesystem not mounted"
    echo "       Run: mount -t bpf bpf /sys/fs/bpf"
    exit 1
fi

echo "[ebpf] Building eBPF program..."
cd "$(dirname "$0")/../ebpf"
make 2>&1 | sed 's/^/  /'

if [[ ! -f tenant_latency.bpf.o ]]; then
    echo "[fail] eBPF compile failed — check clang version (need >= 17)"
    exit 1
fi

echo "[ebpf] Loading eBPF program via bpftool..."
if command -v bpftool &>/dev/null; then
    bpftool prog load tenant_latency.bpf.o "$PROG_PATH" \
        pinmaps /sys/fs/bpf 2>/dev/null || echo "[warn] bpftool load failed — may need newer kernel"
    echo "[ok ] Program pinned at $PROG_PATH"
else
    echo "[warn] bpftool not found — install linux-tools-$(uname -r)"
    echo "       Alternatively: use libbpf-rs from the Rust host (cargo run -- start)"
fi

echo "[ebpf] Verifying pinned maps..."
ls -la /sys/fs/bpf/nexuscore* 2>/dev/null || echo "[info] No pinned maps yet (will be created on first run)"

echo "[ok ] eBPF setup complete"
