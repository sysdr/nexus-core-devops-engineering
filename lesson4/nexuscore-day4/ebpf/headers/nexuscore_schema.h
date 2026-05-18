#pragma once
#include <linux/types.h>

struct nexuscore_hdr {
    __be32 tenant_id;
    __be16 payload_len;
    __u8   flags;
    __u8   reserved;
} __attribute__((packed));

struct nexuscore_meta {
    __u64 schema_version;
    __u32 tenant_id;
    __u32 _pad;
};

#define FIELD_TYPE_U64   0
#define FIELD_TYPE_F64   1
#define FIELD_TYPE_BYTES 2
#define FIELD_TYPE_STR   3

struct schema_descriptor {
    __u64  version;
    __u16  field_count;
    __u16  field_offsets[64];
    __u8   field_types[64];
} __attribute__((packed));

struct schema_event {
    __u64 timestamp_ns;
    __u32 tenant_id;
    __u64 old_version;
    __u64 new_version;
    __u8  event_type;
} __attribute__((packed));
