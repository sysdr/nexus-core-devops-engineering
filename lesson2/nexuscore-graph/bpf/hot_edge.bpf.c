// SPDX-License-Identifier: GPL-2.0
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include <bpf/bpf_core_read.h>

#define HOT_NODE_THRESHOLD  500ULL
#define MAX_NODES           (1 << 20)
#define RINGBUF_SIZE        (1 << 22)

struct {
    __uint(type,       BPF_MAP_TYPE_HASH);
    __uint(max_entries, 65536);
    __type(key,   u32);
    __type(value, u32);
} fd_to_node_map SEC(".maps");

struct {
    __uint(type,       BPF_MAP_TYPE_LRU_HASH);
    __uint(max_entries, MAX_NODES);
    __type(key,   u32);
    __type(value, u64);
} node_heat_map SEC(".maps");

struct {
    __uint(type,       BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, RINGBUF_SIZE);
} heat_events SEC(".maps");

struct heat_event {
    u32 node_id;
    u32 pid;
    u64 read_count;
    u64 ts_ns;
};

SEC("kprobe/sys_read")
int BPF_KPROBE(nexuscore_trace_read,
               unsigned int fd,
               char __user *buf,
               size_t count)
{
    u32 *node_id_ptr = bpf_map_lookup_elem(&fd_to_node_map, &fd);
    if (!node_id_ptr) return 0;
    u32 node_id = *node_id_ptr;

    u64 *cnt = bpf_map_lookup_elem(&node_heat_map, &node_id);
    u64 new_cnt;
    if (cnt) {
        new_cnt = *cnt + 1;
        bpf_map_update_elem(&node_heat_map, &node_id, &new_cnt, BPF_EXIST);
    } else {
        new_cnt = 1;
        bpf_map_update_elem(&node_heat_map, &node_id, &new_cnt, BPF_NOEXIST);
    }

    if (new_cnt == HOT_NODE_THRESHOLD) {
        struct heat_event *e = bpf_ringbuf_reserve(&heat_events, sizeof(struct heat_event), 0);
        if (!e) return 0;

        e->node_id    = node_id;
        e->pid        = bpf_get_current_pid_tgid() >> 32;
        e->read_count = new_cnt;
        e->ts_ns      = bpf_ktime_get_ns();

        bpf_ringbuf_submit(e, BPF_RB_FORCE_WAKEUP);
    }

    return 0;
}

char LICENSE[] SEC("license") = "GPL";
