// SPDX-License-Identifier: GPL-2.0
// NexusCore RAG eBPF Probe — CO-RE (Compile Once, Run Everywhere)
// Attaches to read() syscall tracepoints; measures per-request I/O latency.
// Requires: kernel >= 5.15, BTF enabled, libbpf 1.4+

#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include <bpf/bpf_core_read.h>

#define MAX_ENTRIES  65536
#define HIST_BUCKETS 20     // log2 buckets: 1µs → ~500ms

// ── BPF Maps ─────────────────────────────────────────────────────────────────

/// Per-thread read() entry timestamps (nanoseconds)
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, MAX_ENTRIES);
    __type(key, u64);    // tid
    __type(value, u64);  // ktime_ns at syscall entry
} read_start SEC(".maps");

/// Log2 latency histogram (µs): bucket[i] = count of reads in [2^i µs, 2^(i+1) µs)
struct {
    __uint(type, BPF_MAP_TYPE_ARRAY);
    __uint(max_entries, HIST_BUCKETS);
    __type(key, u32);
    __type(value, u64);
} latency_hist SEC(".maps");

/// Per-tenant byte counters — key=tenant_id (derived from pid heuristic)
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 1024);
    __type(key, u32);   // tenant_id
    __type(value, u64); // bytes read
} tenant_bytes SEC(".maps");

/// Adaptive control plane: maps tenant_id → recommended top-k
/// Written by userspace when degradation is detected; read by Wasm component.
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 1024);
    __type(key, u32);   // tenant_id
    __type(value, u32); // top-k override
} tenant_topk_control SEC(".maps");

// ── Read entry: record start timestamp ────────────────────────────────────────
SEC("tracepoint/syscalls/sys_enter_read")
int trace_read_enter(struct trace_event_raw_sys_enter *ctx)
{
    u64 tid = bpf_get_current_pid_tgid();
    u64 ts  = bpf_ktime_get_ns();
    bpf_map_update_elem(&read_start, &tid, &ts, BPF_ANY);
    return 0;
}

// ── Read exit: compute latency, update histogram and tenant bytes ──────────────
SEC("tracepoint/syscalls/sys_exit_read")
int trace_read_exit(struct trace_event_raw_sys_exit *ctx)
{
    u64 tid = bpf_get_current_pid_tgid();
    u64 *tsp = bpf_map_lookup_elem(&read_start, &tid);
    if (!tsp)
        return 0;

    u64 now   = bpf_ktime_get_ns();
    u64 delta = now - *tsp;
    bpf_map_delete_elem(&read_start, &tid);

    // Convert delta to microseconds for bucketing
    u64 delta_us = delta / 1000;
    if (delta_us < 1) delta_us = 1;

    // log2 bucketing (manual, no libc)
    u32 bucket = 0;
    u64 tmp = delta_us;
    while (tmp > 1 && bucket < HIST_BUCKETS - 1) {
        tmp >>= 1;
        bucket++;
    }

    u64 *cnt = bpf_map_lookup_elem(&latency_hist, &bucket);
    if (cnt)
        __sync_fetch_and_add(cnt, 1);

    // Update per-tenant byte counter
    // Derive tenant_id from lower 16 bits of TID (demo heuristic; production
    // would use a cgroup ID or socket cookie)
    u32 tenant_id = (u32)(tid & 0xFFFF) % 1024;
    u64 bytes_read = (u64)(long)ctx->ret;
    if ((long)bytes_read > 0) {
        u64 *tb = bpf_map_lookup_elem(&tenant_bytes, &tenant_id);
        if (tb)
            __sync_fetch_and_add(tb, bytes_read);
        else
            bpf_map_update_elem(&tenant_bytes, &tenant_id, &bytes_read, BPF_ANY);
    }

    return 0;
}

// ── Adaptive throttle: cgroup BPF to enforce per-tenant I/O weight ────────────
// Attach to: BPF_PROG_TYPE_CGROUP_SKB or BPF_PROG_TYPE_CGROUP_SYSCTL
// This is the kernel-enforced budget; no userspace wakeup required.
SEC("cgroup/skb")
int rag_io_throttle(struct __sk_buff *skb)
{
    // Placeholder: in production, read tenant_bytes and compare against
    // a per-tenant quota map. Return 0 to drop (throttle), 1 to pass.
    // Budget enforcement happens at syscall boundary — no Wasm runtime involvement.
    return 1; // pass all traffic in demo
}

char LICENSE[] SEC("license") = "GPL";
