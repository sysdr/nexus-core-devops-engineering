// SPDX-License-Identifier: GPL-2.0
// NexusCore Day 15 - XDP tweet ingestion, CO-RE
// clang -target bpf -O2 -g -D__TARGET_ARCH_x86_64 \
//   -I/usr/include/x86_64-linux-gnu -c xdp_tweet.c -o xdp_tweet.o
#include <linux/bpf.h>
#include <linux/if_ether.h>
#include <linux/ip.h>
#include <linux/tcp.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_endian.h>

struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, 4 * 1024 * 1024);
} tweet_ringbuf SEC(".maps");

// Per-tenant write-segment routing (homework extension hook)
struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 256);
    __type(key, __u32);
    __type(value, __u32);
} tenant_segment SEC(".maps");

#define MAX_TWEET_LEN 280

struct tweet_record {
    __u64 ingress_ns;
    __u32 tenant_id;
    __u16 text_len;
    __u8  pad[2];
    char  text[MAX_TWEET_LEN];
};

static __always_inline const char *
extract_json_text(const char *p, __u32 plen, __u16 *out_len)
{
    #pragma unroll
    for (__u32 i = 0; i < 512 && i + 8 < plen; i++) {
        if (p[i]=='\"'&&p[i+1]=='t'&&p[i+2]=='e'&&p[i+3]=='x'&&
            p[i+4]=='t'&&p[i+5]=='\"'&&p[i+6]==':'&&p[i+7]=='\"') {
            __u32 s = i + 8; __u16 len = 0;
            #pragma unroll
            for (__u16 j = 0; j < MAX_TWEET_LEN && s+j < plen; j++) {
                if (p[s+j] == '\"') break; len++;
            }
            *out_len = len; return p + s;
        }
    }
    return NULL;
}

SEC("xdp")
int xdp_tweet_ingress(struct xdp_md *ctx)
{
    void *de = (void *)(long)ctx->data_end;
    void *d  = (void *)(long)ctx->data;
    struct ethhdr *eth = d;
    if ((void*)(eth+1)>de) return XDP_PASS;
    if (bpf_ntohs(eth->h_proto)!=ETH_P_IP) return XDP_PASS;
    struct iphdr *ip = (void*)(eth+1);
    if ((void*)(ip+1)>de || ip->protocol!=IPPROTO_TCP) return XDP_PASS;
    struct tcphdr *tcp = (void*)ip + (ip->ihl*4);
    if ((void*)(tcp+1)>de || bpf_ntohs(tcp->dest)!=9090) return XDP_PASS;
    char *payload = (char*)tcp + tcp->doff*4;
    __u32 plen = (char*)de - payload;
    if (plen < 8) return XDP_PASS;
    __u16 tlen = 0;
    const char *tptr = extract_json_text(payload, plen, &tlen);
    if (!tptr || tlen==0) return XDP_PASS;
    struct tweet_record *rec = bpf_ringbuf_reserve(&tweet_ringbuf,sizeof(*rec),0);
    if (!rec) return XDP_PASS;
    rec->ingress_ns = bpf_ktime_get_ns();
    rec->tenant_id  = bpf_ntohs(tcp->source) & 0xFF;
    rec->text_len   = tlen > MAX_TWEET_LEN ? MAX_TWEET_LEN : tlen;
    rec->pad[0] = rec->pad[1] = 0;
    bpf_probe_read_kernel(rec->text, rec->text_len, tptr);
    bpf_ringbuf_submit(rec, 0);
    return XDP_PASS;
}
char _license[] SEC("license") = "GPL";
