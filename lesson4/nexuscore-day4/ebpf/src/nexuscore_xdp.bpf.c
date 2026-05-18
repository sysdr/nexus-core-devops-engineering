// SPDX-License-Identifier: GPL-2.0
#include <linux/bpf.h>
#include <linux/if_ether.h>
#include <linux/ip.h>
#include <linux/udp.h>
#include <linux/in.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_endian.h>
#include <bpf/bpf_core_read.h>
#include "../headers/nexuscore_schema.h"

struct {
    __uint(type, BPF_MAP_TYPE_LRU_HASH);
    __uint(max_entries, 65536);
    __type(key,   __u32);
    __type(value, struct schema_descriptor);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} schema_map SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_LRU_HASH);
    __uint(max_entries, 65536);
    __type(key,   __u32);
    __type(value, struct schema_descriptor);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} schema_pending_map SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 1 << 20);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} schema_events SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, 256);
    __type(key,   __u32);
    __type(value, __u64);
} pkt_counters SEC(".maps");

SEC("xdp")
int nexuscore_classify(struct xdp_md *ctx)
{
    void *data     = (void *)(long)ctx->data;
    void *data_end = (void *)(long)ctx->data_end;

    struct ethhdr *eth = data;
    if ((void *)(eth + 1) > data_end)
        return XDP_DROP;
    if (bpf_ntohs(eth->h_proto) != ETH_P_IP)
        return XDP_PASS;

    struct iphdr *iph = (void *)(eth + 1);
    if ((void *)(iph + 1) > data_end)
        return XDP_DROP;
    if (iph->protocol != IPPROTO_UDP)
        return XDP_PASS;

    struct udphdr *udph = (void *)iph + (iph->ihl * 4);
    if ((void *)(udph + 1) > data_end)
        return XDP_DROP;

    if (bpf_ntohs(udph->dest) != 9900)
        return XDP_PASS;

    struct nexuscore_hdr *hdr = (void *)(udph + 1);
    if ((void *)(hdr + 1) > data_end)
        return XDP_DROP;

    __u32 tenant_id = bpf_ntohl(hdr->tenant_id);

    struct schema_descriptor *sd = bpf_map_lookup_elem(&schema_map, &tenant_id);
    if (!sd)
        return XDP_PASS;

    if (bpf_xdp_adjust_meta(ctx, -(int)sizeof(struct nexuscore_meta)) != 0)
        return XDP_PASS;

    struct nexuscore_meta *meta = (void *)(long)ctx->data_meta;
    if ((void *)(meta + 1) > (void *)(long)ctx->data)
        return XDP_PASS;

    meta->schema_version = sd->version;
    meta->tenant_id      = tenant_id;
    meta->_pad           = 0;

    __u32 bucket = tenant_id & 0xFF;
    __u64 *cnt = bpf_map_lookup_elem(&pkt_counters, &bucket);
    if (cnt)
        __sync_fetch_and_add(cnt, 1);

    return XDP_PASS;
}

char _license[] SEC("license") = "GPL";
