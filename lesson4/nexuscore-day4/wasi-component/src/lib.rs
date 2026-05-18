use std::string::String;
use std::vec::Vec;

wit_bindgen::generate!({
    world: "tenant-component",
    path:  "wit/nexuscore.wit",
});

use crate::exports::nexuscore::tenant::tenant_processor::{FieldValue, Guest, SchemaResult};

#[repr(C, packed)]
struct RawSchemaDescriptor {
    version:       u64,
    field_count:   u16,
    field_offsets: [u16; 64],
    field_types:   [u8; 64],
}

static mut SCHEMA: RawSchemaDescriptor = RawSchemaDescriptor {
    version:       0,
    field_count:   0,
    field_offsets: [0u16; 64],
    field_types:   [0u8; 64],
};

static mut STAT_PROCESSED:     u64 = 0;
static mut STAT_ERRORS:         u64 = 0;
static mut STAT_SCHEMA_UPDATES: u64 = 0;

struct TenantComponent;

impl Guest for TenantComponent {
    fn apply_schema_update(raw: Vec<u8>) -> SchemaResult {
        const SZ: usize = core::mem::size_of::<RawSchemaDescriptor>();
        if raw.len() < SZ {
            return SchemaResult::InvalidDescriptor;
        }

        let new_schema = unsafe {
            core::ptr::read_unaligned(raw.as_ptr() as *const RawSchemaDescriptor)
        };

        unsafe {
            if new_schema.version <= SCHEMA.version {
                return SchemaResult::StaleUpdate;
            }
            SCHEMA = new_schema;
            STAT_SCHEMA_UPDATES += 1;
        }

        SchemaResult::Ok
    }

    fn process_frame(frame: Vec<u8>, schema_version: u64) -> Vec<FieldValue> {
        unsafe {
            if schema_version != SCHEMA.version {
                STAT_ERRORS += 1;
                return Vec::new();
            }

            let field_count = SCHEMA.field_count as usize;
            let mut fields = Vec::with_capacity(field_count);

            for i in 0..field_count {
                let offset = SCHEMA.field_offsets[i] as usize;
                let ftype  = SCHEMA.field_types[i];

                let value = match ftype {
                    0 if offset + 8 <= frame.len() => {
                        let mut buf = [0u8; 8];
                        buf.copy_from_slice(&frame[offset..offset + 8]);
                        FieldValue::U64Val(u64::from_be_bytes(buf))
                    }
                    1 if offset + 8 <= frame.len() => {
                        let mut buf = [0u8; 8];
                        buf.copy_from_slice(&frame[offset..offset + 8]);
                        FieldValue::F64Val(f64::from_be_bytes(buf))
                    }
                    2 if offset + 2 <= frame.len() => {
                        let len = u16::from_be_bytes([frame[offset], frame[offset + 1]]) as usize;
                        let end = offset + 2 + len;
                        if end <= frame.len() {
                            FieldValue::BytesVal(frame[offset + 2..end].to_vec())
                        } else {
                            FieldValue::Unknown
                        }
                    }
                    3 if offset + 2 <= frame.len() => {
                        let len = u16::from_be_bytes([frame[offset], frame[offset + 1]]) as usize;
                        let end = offset + 2 + len;
                        if end <= frame.len() {
                            match core::str::from_utf8(&frame[offset + 2..end]) {
                                Ok(s)  => FieldValue::StrVal(String::from(s)),
                                Err(_) => FieldValue::Unknown,
                            }
                        } else {
                            FieldValue::Unknown
                        }
                    }
                    _ => FieldValue::Unknown,
                };

                fields.push(value);
            }

            STAT_PROCESSED += 1;
            fields
        }
    }

    fn current_schema_version() -> u64 {
        unsafe { SCHEMA.version }
    }

    fn stats() -> (u64, u64, u64) {
        unsafe { (STAT_PROCESSED, STAT_ERRORS, STAT_SCHEMA_UPDATES) }
    }
}

export!(TenantComponent);
