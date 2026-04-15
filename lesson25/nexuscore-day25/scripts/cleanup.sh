#!/usr/bin/env bash
echo "Stopping Redpanda..."
docker stop nexuscore-redpanda 2>/dev/null || true
docker rm   nexuscore-redpanda 2>/dev/null || true

echo "Unloading BPF probe (if loaded)..."
if [[ $(id -u) -eq 0 ]]; then
    bpftool prog detach pinned /sys/fs/bpf/nexuscore_ts 2>/dev/null || true
    rm -f /sys/fs/bpf/nexuscore_ts 2>/dev/null || true
fi

echo "Cleaning build artifacts..."
cargo clean 2>/dev/null || true
make -C ebpf clean 2>/dev/null || true

echo "Done."
