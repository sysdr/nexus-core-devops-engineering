// NexusCore Day 5 — eBPF CO-RE Latency Probe
// Kernel >= 6.1 required (BTF + CO-RE + ring buffer support)
// Attach targets: vfs_read, vfs_write, tcp_sendmsg (kprobes)
//
// Design decisions:
//  PERCPU_ARRAY histogram: eliminates per-update spinlock, sum in userspace
//  Ring buffer for raw events: zero-copy, in-order, never blocks
//  log2 bucketing in kernel: reduces userspace work to O(ncpu * 64) per poll

#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include <bpf/bpf_core_read.h>

#define MAX_BUCKETS 64
#define MAX_TRACKED_PIDS 1024

// ---- Maps ------------------------------------------------------------------

// Per-CPU log2 latency histogram for SurrealDB vfs_read
struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, MAX_BUCKETS);
    __type(key, __u32);
    __type(value, __u64);
} hist_surreal_read SEC(".maps");

// Per-CPU log2 latency histogram for polyglot vfs_read
struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, MAX_BUCKETS);
    __type(key, __u32);
    __type(value, __u64);
} hist_polyglot_read SEC(".maps");

// Per-CPU histogram for tcp_sendmsg (both stacks)
struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, MAX_BUCKETS);
    __type(key, __u32);
    __type(value, __u64);
} hist_tcp_send SEC(".maps");

// Temporary per-PID start timestamps (kprobe entry → kprobe exit pairing)
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, MAX_TRACKED_PIDS);
    __type(key, __u32);   // pid
    __type(value, __u64); // entry timestamp (ns)
} start_ts SEC(".maps");

// Ring buffer for structured raw events → userspace
struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 1 << 22); // 4MB ring buffer
} events SEC(".maps");

// PID filter map: userspace populates this with target PIDs
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, MAX_TRACKED_PIDS);
    __type(key, __u32);  // pid
    __type(value, __u8); // 0 = surrealdb, 1 = polyglot
} target_pids SEC(".maps");

// ---- Types -----------------------------------------------------------------

#define STACK_SURREAL  0
#define STACK_POLYGLOT 1
#define OP_VFS_READ    0
#define OP_VFS_WRITE   1
#define OP_TCP_SEND    2

struct event {
    __u32 pid;
    __u8  stack;    // STACK_*
    __u8  op;       // OP_*
    __u16 _pad;
    __u64 latency_ns;
    __u64 bytes;
};

// ---- Helpers ---------------------------------------------------------------

static __always_inline __u32 log2_bucket(__u64 v) {
    __u32 r = 0;
    if (v == 0) return 0;
    // Unrolled for BPF verifier (no loops with variable bounds)
    if (v >= (1ULL << 32)) { r += 32; v >>= 32; }
    if (v >= (1ULL << 16)) { r += 16; v >>= 16; }
    if (v >= (1ULL <<  8)) { r +=  8; v >>=  8; }
    if (v >= (1ULL <<  4)) { r +=  4; v >>=  4; }
    if (v >= (1ULL <<  2)) { r +=  2; v >>=  2; }
    if (v >= (1ULL <<  1)) { r +=  1; }
    return r < MAX_BUCKETS ? r : MAX_BUCKETS - 1;
}

static __always_inline void hist_update(void *map, __u64 latency_ns) {
    __u32 slot = log2_bucket(latency_ns);
    __u64 *count = bpf_map_lookup_elem(map, &slot);
    if (count) {
        __sync_fetch_and_add(count, 1);
    }
}

// ---- kprobe: vfs_read entry ------------------------------------------------

SEC("kprobe/vfs_read")
int BPF_KPROBE(probe_vfs_read_enter, struct file *file, char __user *buf, size_t count, loff_t *pos)
{
    __u32 pid = bpf_get_current_pid_tgid() >> 32;
    __u8 *stack_type = bpf_map_lookup_elem(&target_pids, &pid);
    if (!stack_type) return 0;

    __u64 ts = bpf_ktime_get_ns();
    bpf_map_update_elem(&start_ts, &pid, &ts, BPF_ANY);
    return 0;
}

// ---- kprobe: vfs_read exit -------------------------------------------------

SEC("kretprobe/vfs_read")
int BPF_KRETPROBE(probe_vfs_read_exit, ssize_t ret)
{
    __u32 pid = bpf_get_current_pid_tgid() >> 32;
    __u8 *stack_type = bpf_map_lookup_elem(&target_pids, &pid);
    if (!stack_type) return 0;

    __u64 *start = bpf_map_lookup_elem(&start_ts, &pid);
    if (!start) return 0;

    __u64 latency = bpf_ktime_get_ns() - *start;
    bpf_map_delete_elem(&start_ts, &pid);

    // Update the correct per-stack histogram
    if (*stack_type == STACK_SURREAL) {
        hist_update(&hist_surreal_read, latency);
    } else {
        hist_update(&hist_polyglot_read, latency);
    }

    // Emit raw event to ring buffer (zero-copy reservation)
    struct event *e = bpf_ringbuf_reserve(&events, sizeof(struct event), 0);
    if (e) {
        e->pid        = pid;
        e->stack      = *stack_type;
        e->op         = OP_VFS_READ;
        e->_pad       = 0;
        e->latency_ns = latency;
        e->bytes      = (ret > 0) ? (__u64)ret : 0;
        bpf_ringbuf_submit(e, 0);
    }
    return 0;
}

// ---- kprobe: tcp_sendmsg entry ---------------------------------------------

SEC("kprobe/tcp_sendmsg")
int BPF_KPROBE(probe_tcp_sendmsg_enter, struct sock *sk, struct msghdr *msg, size_t size)
{
    __u32 pid = bpf_get_current_pid_tgid() >> 32;
    __u8 *stack_type = bpf_map_lookup_elem(&target_pids, &pid);
    if (!stack_type) return 0;

    __u64 ts = bpf_ktime_get_ns();
    bpf_map_update_elem(&start_ts, &pid, &ts, BPF_ANY);
    return 0;
}

SEC("kretprobe/tcp_sendmsg")
int BPF_KRETPROBE(probe_tcp_sendmsg_exit, int ret)
{
    __u32 pid = bpf_get_current_pid_tgid() >> 32;
    __u8 *stack_type = bpf_map_lookup_elem(&target_pids, &pid);
    if (!stack_type) return 0;

    __u64 *start = bpf_map_lookup_elem(&start_ts, &pid);
    if (!start) return 0;

    __u64 latency = bpf_ktime_get_ns() - *start;
    bpf_map_delete_elem(&start_ts, &pid);
    hist_update(&hist_tcp_send, latency);
    return 0;
}

char _license[] SEC("license") = "GPL";
