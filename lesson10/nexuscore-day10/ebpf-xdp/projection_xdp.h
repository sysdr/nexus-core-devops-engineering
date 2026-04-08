#pragma once
#include <linux/types.h>

#define MAX_PROJ_SIZE   4096
#define MAX_ENTRIES     (1 << 20)  /* 1M projection cache slots */
#define NEXUS_MAGIC     0x4E435052 /* "NCPR" */

/* Binary framing for NexusCore query packets (UDP payload) */
struct nexus_query_hdr {
    __be32 magic;          /* 0x4E435050 "NCPP" */
    __be32 tenant_id;
    __be32 projection_id;
    __be32 flags;
};

struct nexus_resp_hdr {
    __be32 magic;          /* 0x4E435052 "NCPR" */
    __be32 tenant_id;
    __be32 projection_id;
    __be32 data_len;
};

/* eBPF map key */
struct proj_key {
    __u32 tenant_id;
    __u32 projection_id;
};

/* eBPF map value — Flatbuffer-aligned projection slot */
struct proj_value {
    __u64 version;
    __u32 data_len;
    __u32 _pad;
    __u8  data[MAX_PROJ_SIZE];
};

/* Stats counters map key offsets */
enum stats_key {
    STAT_CACHE_HIT   = 0,
    STAT_CACHE_MISS  = 1,
    STAT_CACHE_ERROR = 2,
    STAT_MAX,
};
