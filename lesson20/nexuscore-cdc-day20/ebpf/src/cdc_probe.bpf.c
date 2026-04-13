// SPDX-License-Identifier: GPL-2.0
#include <linux/bpf.h>
#include <asm/ptrace.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include <bpf/bpf_core_read.h>
#include <linux/types.h>

#define EVENT_MAX_VAL_LEN 2048
#define RINGBUF_SIZE_MB   64
#define TABLE_HASH_LEN    32

struct cdc_event {
    __u64 tenant_id;
    __u64 ts_ns;
    __u8  table_hash[TABLE_HASH_LEN];
    __u16 val_len;
    __u8  _pad[2];
    __u8  payload[];
} __attribute__((packed));

struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, RINGBUF_SIZE_MB * 1024 * 1024);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} cdc_ringbuf SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 4096);
    __type(key, __u64);
    __type(value, __u64);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} cdc_tenant_map SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, 3);
    __type(key, __u32);
    __type(value, __u64);
} cdc_counters SEC(".maps");

static __always_inline __u64 fnv1a_hash(const __u8 *data, __u32 len) {
    __u64 h = 14695981039346656037ULL;
    #pragma unroll
    for (__u32 i = 0; i < 32 && i < len; i++) {
        h ^= data[i];
        h *= 1099511628211ULL;
    }
    return h;
}

static __always_inline void counter_inc(__u32 idx) {
    __u64 *v = bpf_map_lookup_elem(&cdc_counters, &idx);
    if (v) __sync_fetch_and_add(v, 1);
}

SEC("uprobe//proc/self/exe:_ZN7rocksdb10WriteBatch3PutERNS_20ColumnFamilyHandleERKNS_5SliceES5_")
int BPF_UPROBE(cdc_rocksdb_put, void *batch, void *cf, void *key_slice, void *val_slice)
{
    counter_inc(0);
    const char *key_data = NULL;
    __u64 key_size = 0;
    bpf_probe_read_user(&key_data, sizeof(key_data), key_slice);
    bpf_probe_read_user(&key_size, sizeof(key_size), (char *)key_slice + 8);
    if (key_size == 0 || key_size > 512) return 0;
    const char *val_data = NULL;
    __u64 val_size = 0;
    bpf_probe_read_user(&val_data, sizeof(val_data), val_slice);
    bpf_probe_read_user(&val_size, sizeof(val_size), (char *)val_slice + 8);
    if (val_size == 0 || val_size > EVENT_MAX_VAL_LEN) return 0;
    __u8 key_buf[32] = {};
    __u32 read_len = key_size < 32 ? (__u32)key_size : 32;
    bpf_probe_read_user(key_buf, read_len, key_data);
    __u64 table_key = fnv1a_hash(key_buf, read_len);
    __u64 *tenant_id_ptr = bpf_map_lookup_elem(&cdc_tenant_map, &table_key);
    __u64 tenant_id = tenant_id_ptr ? *tenant_id_ptr : 0;
    __u32 event_size = sizeof(struct cdc_event) + (__u32)val_size;
    struct cdc_event *evt = bpf_ringbuf_reserve(&cdc_ringbuf, event_size, 0);
    if (!evt) { counter_inc(1); return 0; }
    evt->tenant_id = tenant_id;
    evt->ts_ns     = bpf_ktime_get_ns();
    evt->val_len   = (__u16)val_size;
    evt->_pad[0]   = 0;
    evt->_pad[1]   = 0;
    __u64 h = fnv1a_hash(key_buf, read_len);
    __builtin_memcpy(evt->table_hash, &h, 8);
    bpf_probe_read_user(evt->payload, (__u32)val_size, val_data);
    bpf_ringbuf_submit(evt, BPF_RB_FORCE_WAKEUP);
    counter_inc(2);
    return 0;
}

char _license[] SEC("license") = "GPL";
