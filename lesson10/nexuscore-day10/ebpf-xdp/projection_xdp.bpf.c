// ============================================================================
// NexusCore XDP Projection Cache — eBPF CO-RE Program
// Kernel ≥ 5.15 with CONFIG_DEBUG_INFO_BTF=y
//
// Attach: ip link set dev <iface> xdp obj projection_xdp.bpf.o sec xdp
// Pin:    auto-pinned to /sys/fs/bpf/nexuscore/ via LIBBPF_PIN_BY_NAME
// ============================================================================
#include "vmlinux.h"
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_endian.h>
#include "projection_xdp.h"

// ---------------------------------------------------------------------------
// MAP DEFINITIONS
// All maps are pinned to /sys/fs/bpf/nexuscore/<map_name>
// ---------------------------------------------------------------------------
struct {
    __uint(type,        BPF_MAP_TYPE_LRU_HASH);
    __uint(max_entries, MAX_ENTRIES);
    __type(key,         struct proj_key);
    __type(value,       struct proj_value);
    __uint(pinning,     LIBBPF_PIN_BY_NAME);
} proj_cache SEC(".maps");

struct {
    __uint(type,        BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, STAT_MAX);
    __type(key,         __u32);
    __type(value,       __u64);
    __uint(pinning,     LIBBPF_PIN_BY_NAME);
} nexus_stats SEC(".maps");

// Ringbuf for cache miss notifications → userspace projection engine
struct {
    __uint(type,        BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 256 * 1024); /* 256KB ring */
    __uint(pinning,     LIBBPF_PIN_BY_NAME);
} miss_ring SEC(".maps");

// ---------------------------------------------------------------------------
// HELPERS
// ---------------------------------------------------------------------------
static __always_inline void stat_inc(enum stats_key key) {
    __u32 k = key;
    __u64 *val = bpf_map_lookup_elem(&nexus_stats, &k);
    if (val)
        __sync_fetch_and_add(val, 1);
}

// ---------------------------------------------------------------------------
// XDP HANDLER
// Packet layout: [eth][ip][udp][nexus_query_hdr][...ignored...]
// Response:      swap src/dst, replace UDP payload with [nexus_resp_hdr][proj_data]
// ---------------------------------------------------------------------------
SEC("xdp")
int xdp_proj_handler(struct xdp_md *ctx)
{
    void *data     = (void *)(long)ctx->data;
    void *data_end = (void *)(long)ctx->data_end;

    // ---- Ethernet header ----
    struct ethhdr *eth = data;
    if ((void *)(eth + 1) > data_end)
        return XDP_PASS;
    if (eth->h_proto != bpf_htons(ETH_P_IP))
        return XDP_PASS;

    // ---- IPv4 header ----
    struct iphdr *ip = (void *)(eth + 1);
    if ((void *)(ip + 1) > data_end)
        return XDP_PASS;
    if (ip->protocol != IPPROTO_UDP)
        return XDP_PASS;

    // ---- UDP header ----
    struct udphdr *udp = (void *)ip + (ip->ihl * 4);
    if ((void *)(udp + 1) > data_end)
        return XDP_PASS;
    if (bpf_ntohs(udp->dest) != 9000) /* NexusCore query port */
        return XDP_PASS;

    // ---- NexusCore query header ----
    struct nexus_query_hdr *qhdr = (void *)(udp + 1);
    if ((void *)(qhdr + 1) > data_end)
        return XDP_PASS;
    if (bpf_ntohl(qhdr->magic) != 0x4E435050)
        return XDP_PASS;

    struct proj_key key = {
        .tenant_id     = bpf_ntohl(qhdr->tenant_id),
        .projection_id = bpf_ntohl(qhdr->projection_id),
    };

    struct proj_value *val = bpf_map_lookup_elem(&proj_cache, &key);
    if (!val) {
        // Cache miss: notify userspace via ring buffer
        stat_inc(STAT_CACHE_MISS);

        struct proj_key *missed = bpf_ringbuf_reserve(&miss_ring, sizeof(*missed), 0);
        if (missed) {
            *missed = key;
            bpf_ringbuf_submit(missed, 0);
        }
        return XDP_PASS;
    }

    stat_inc(STAT_CACHE_HIT);

    // ---- Rewrite packet as response ----
    // Swap MAC addresses
    __u8 tmp_mac[6];
    __builtin_memcpy(tmp_mac,       eth->h_dest,   6);
    __builtin_memcpy(eth->h_dest,   eth->h_source, 6);
    __builtin_memcpy(eth->h_source, tmp_mac,        6);

    // Swap IP addresses
    __be32 tmp_ip   = ip->saddr;
    ip->saddr       = ip->daddr;
    ip->daddr       = tmp_ip;

    // Swap UDP ports
    __be16 tmp_port = udp->source;
    udp->source     = udp->dest;
    udp->dest       = tmp_port;

    // Write response header immediately after UDP header
    struct nexus_resp_hdr *rhdr = (void *)(udp + 1);
    if ((void *)(rhdr + 1) > data_end) {
        stat_inc(STAT_CACHE_ERROR);
        return XDP_PASS;
    }

    rhdr->magic         = bpf_htonl(NEXUS_MAGIC);
    rhdr->tenant_id     = bpf_htonl(key.tenant_id);
    rhdr->projection_id = bpf_htonl(key.projection_id);

    __u32 copy_len = val->data_len;
    if (copy_len > MAX_PROJ_SIZE || copy_len == 0) {
        stat_inc(STAT_CACHE_ERROR);
        return XDP_PASS;
    }
    rhdr->data_len = bpf_htonl(copy_len);

    // bpf_xdp_store_bytes for the projection data region
    int rc = bpf_xdp_store_bytes(ctx,
        (void *)rhdr - data + sizeof(*rhdr),
        val->data,
        copy_len);
    if (rc < 0) {
        stat_inc(STAT_CACHE_ERROR);
        return XDP_PASS;
    }

    return XDP_TX;
}

char _license[] SEC("license") = "GPL";
