#!/usr/bin/env bash
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

rm -f "$ROOT/ebpf/nexuscore_xdp.bpf.o"
rm -f "$ROOT/control-plane/nexuscore-ctrl"
(cd "$ROOT/wasi-component" && cargo clean 2>/dev/null) || true
sudo ip link set dev lo xdpgeneric off 2>/dev/null || true
sudo rm -rf /sys/fs/bpf/nexuscore 2>/dev/null || true
echo "[cleanup] done"
