// NexusCore eBPF CO-RE Probe — Day 1
// Instruments SurrealDB TCP send/recv to measure per-tenant query latency
// from kernel space. Zero userspace overhead on the hot path.
//
// Compile: clang -O2 -g -target bpf -D__TARGET_ARCH_x86 \
//          -I/usr/include/$(uname -m)-linux-gnu \
//          -c tenant_latency.bpf.c -o tenant_latency.bpf.o
//
// Load: sudo bpftool prog load tenant_latency.bpf.o /sys/fs/bpf/nexuscore_prog

// When vmlinux.h is not available, use fallback definitions
#ifdef __BPF_VMLINUX__
#include "vmlinux.h"
#else
#include <linux/bpf.h>
#include <linux/ptrace.h>
#include <sys/socket.h>
typedef unsigned char      u8;
typedef unsigned short     u16;
typedef unsigned int       u32;
typedef unsigned long long u64;
typedef signed int         s32;
struct sock { char pad[256]; };
struct msghdr { char pad[128]; };
#endif

#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include <bpf/bpf_core_read.h>
#include <bpf/bpf_endian.h>

#define SURREAL_PORT    8000
#define MAX_TENANTS     65536
#define RINGBUF_SIZE    (1 << 24)  // 16MB

// --- Data Structures --------------------------------------------------------

struct tenant_event {
    u32 slot_id;       // Thread ID used as proxy for connection slot
    u64 latency_ns;    // End-to-end TCP send→recv latency
    u8  op;            // 0 = send, 1 = recv
    u8  pad[3];
};

// --- BPF Maps ---------------------------------------------------------------

// BPF ringbuf: zero-copy event emission to userspace
// Producer: kernel kprobe/kretprobe
// Consumer: Rust host's event loop via poll()
struct {
    __uint(type,        BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, RINGBUF_SIZE);
} events SEC(".maps");

// Pinned hash map: tid → send_timestamp
// LIBBPF_PIN_BY_NAME: survives process restart!
// Pinned at: /sys/fs/bpf/nexuscore_tenant_ts_map
struct {
    __uint(type,        BPF_MAP_TYPE_HASH);
    __uint(max_entries, MAX_TENANTS);
    __type(key,         u32);   // thread ID
    __type(value,       u64);   // ktime_get_ns() at send
    __uint(pinning,     1);     // LIBBPF_PIN_BY_NAME
} tenant_ts_map SEC(".maps");

// Per-CPU array for intermediate storage (avoids map contention)
struct {
    __uint(type,        BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, 1);
    __type(key,         u32);
    __type(value,       struct tenant_event);
} scratch SEC(".maps");

// --- Probes -----------------------------------------------------------------

SEC("kprobe/tcp_sendmsg")
int BPF_KPROBE(trace_tcp_sendmsg,
               struct sock *sk,
               struct msghdr *msg,
               size_t size)
{
    // Filter: only SurrealDB port (destination port in network byte order)
    u16 dport;
    BPF_CORE_READ_INTO(&dport, sk, __sk_common.skc_dport);
    if (bpf_ntohs(dport) != SURREAL_PORT)
        return 0;

    // Record send timestamp, keyed by thread ID
    // Thread ID is stable for the duration of a synchronous query
    u32 tid = (u32)(bpf_get_current_pid_tgid() & 0xFFFFFFFF);
    u64 ts  = bpf_ktime_get_ns();

    bpf_map_update_elem(&tenant_ts_map, &tid, &ts, BPF_ANY);

    return 0;
}

SEC("kretprobe/tcp_recvmsg")
int BPF_KRETPROBE(trace_tcp_recvmsg_ret, int ret)
{
    if (ret <= 0)
        return 0;

    u32 tid = (u32)(bpf_get_current_pid_tgid() & 0xFFFFFFFF);
    u64 *send_ts = bpf_map_lookup_elem(&tenant_ts_map, &tid);
    if (!send_ts)
        return 0;

    u64 now     = bpf_ktime_get_ns();
    u64 latency = now - *send_ts;

    // Clean up — don't leave stale entries
    bpf_map_delete_elem(&tenant_ts_map, &tid);

    // Reserve ringbuf slot — zero-copy: kernel writes directly to consumer buffer
    struct tenant_event *e = bpf_ringbuf_reserve(&events, sizeof(*e), 0);
    if (!e)
        return 0;  // Ring buffer full — event dropped (counter via /proc/net/bpf_stats)

    e->slot_id    = tid;
    e->latency_ns = latency;
    e->op         = 1;
    e->pad[0]     = 0; e->pad[1] = 0; e->pad[2] = 0;

    // BPF_RB_FORCE_WAKEUP: wake consumer immediately if latency > 5ms
    u64 flags = (latency > 5000000ULL) ? BPF_RB_FORCE_WAKEUP : BPF_RB_NO_WAKEUP;
    bpf_ringbuf_submit(e, flags);

    return 0;
}

// Tail call target: aggregate latencies into histogram BPF map
// (advanced pattern — requires BPF_MAP_TYPE_PROG_ARRAY)
SEC("kprobe/nexuscore_aggregate")
int aggregate_latency(struct pt_regs *ctx) {
    // Reserved for tail call chain — not attached directly
    return 0;
}

char LICENSE[] SEC("license") = "GPL";
