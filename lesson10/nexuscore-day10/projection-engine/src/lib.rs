// ============================================================================
// NexusCore Projection Engine — WASI 0.3 Component
// ============================================================================
// Implements the `nexuscore:projection/projection-engine` WIT interface.
// Compiled to wasm32-wasip2 (WASI Preview 2/3 component model target).
//
// Design constraints:
//   • No heap allocations beyond the linear memory flat-buffer builder.
//   • Pure function: same events → same projection. No hidden global state.
//   • Flatbuffer layout is the ABI shared with the eBPF XDP reader.
// ============================================================================

#![no_std]
extern crate alloc;

use alloc::{string::{String, ToString}, vec, vec::Vec, format};
wit_bindgen::generate!({
    world: "nexuscore-projection",
    path:  "wit",
});

use exports::nexuscore::projection::projection_engine::{
    Event, EngineError, Guest, Projection, RebuildRequest,
};

// ---------------------------------------------------------------------------
// Flatbuffer layout (hand-written, no schema compiler dependency)
// All fields little-endian. This is the contract read by the eBPF program.
//
//  Offset  Size  Field
//  0       4     magic    (0x4E435052 = "NCPR")
//  4       4     version  (lower 32 bits)
//  8       4     tenant_id
//  12      4     projection_id
//  16      4     item_count
//  20      4     data_len
//  24      N     data bytes (variable, max 4072)
// ---------------------------------------------------------------------------
const MAGIC: u32 = 0x4E435052;
const HEADER_SIZE: usize = 24;
const MAX_PAYLOAD: usize = 4096;

fn encode_projection(
    tenant_id: u32,
    projection_id: u32,
    version: u64,
    item_count: u32,
    items: &[u8],
) -> Result<Vec<u8>, EngineError> {
    let data_len = items.len();
    if HEADER_SIZE + data_len > MAX_PAYLOAD {
        return Err(EngineError::PayloadTooLarge((HEADER_SIZE + data_len) as u32));
    }

    let mut buf = vec![0u8; HEADER_SIZE + data_len];
    buf[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    buf[4..8].copy_from_slice(&(version as u32).to_le_bytes());
    buf[8..12].copy_from_slice(&tenant_id.to_le_bytes());
    buf[12..16].copy_from_slice(&projection_id.to_le_bytes());
    buf[16..20].copy_from_slice(&item_count.to_le_bytes());
    buf[20..24].copy_from_slice(&(data_len as u32).to_le_bytes());
    buf[24..].copy_from_slice(items);
    Ok(buf)
}

// ---------------------------------------------------------------------------
// Projection state accumulator
// ---------------------------------------------------------------------------
#[derive(Default)]
struct ProjState {
    tenant_id:     u32,
    projection_id: u32,
    version:       u64,
    // Simple counter projection: sum of "increment" events per key.
    // In production this would be a columnar store; here it's demonstrative.
    counters:      [(u32, i64); 32], // (key, value) pairs, fixed-size
    counter_len:   usize,
}

impl ProjState {
    fn apply_event(&mut self, evt: &Event) -> Result<(), EngineError> {
        self.tenant_id     = evt.tenant_id;
        self.projection_id = evt.projection_id;
        self.version       = evt.seq;

        match evt.kind.as_str() {
            "increment" => {
                if evt.payload.len() < 8 {
                    return Err(EngineError::InvalidEvent("increment needs 8 bytes".to_string()));
                }
                let key = u32::from_le_bytes(evt.payload[0..4].try_into().unwrap());
                let delta = i32::from_le_bytes(evt.payload[4..8].try_into().unwrap()) as i64;

                // Update existing counter or insert new
                for i in 0..self.counter_len {
                    if self.counters[i].0 == key {
                        self.counters[i].1 = self.counters[i].1.saturating_add(delta);
                        return Ok(());
                    }
                }
                if self.counter_len < 32 {
                    self.counters[self.counter_len] = (key, delta);
                    self.counter_len += 1;
                }
                Ok(())
            }
            "reset" => {
                self.counters = [(0, 0); 32];
                self.counter_len = 0;
                Ok(())
            }
            other => Err(EngineError::InvalidEvent(format!("unknown event kind: {}", other))),
        }
    }

    fn encode(&self) -> Result<Vec<u8>, EngineError> {
        // Pack counters as (u32 key, i64 value) pairs
        let mut items = vec![0u8; self.counter_len * 12];
        for (i, &(k, v)) in self.counters[..self.counter_len].iter().enumerate() {
            items[i*12..i*12+4].copy_from_slice(&k.to_le_bytes());
            items[i*12+4..i*12+12].copy_from_slice(&v.to_le_bytes());
        }
        encode_projection(
            self.tenant_id,
            self.projection_id,
            self.version,
            self.counter_len as u32,
            &items,
        )
    }
}

// ---------------------------------------------------------------------------
// WIT Guest implementation
// ---------------------------------------------------------------------------
struct Engine;

impl Guest for Engine {
    fn rebuild(req: RebuildRequest) -> Result<Projection, EngineError> {
        if req.events.is_empty() {
            return Err(EngineError::InvalidEvent("empty event list".to_string()));
        }

        let first = &req.events[0];
        let mut state = ProjState {
            tenant_id:     first.tenant_id,
            projection_id: first.projection_id,
            ..Default::default()
        };

        // Skip events below snapshot_seq if incremental replay
        let start_seq = req.snapshot_seq.unwrap_or(0);
        for evt in req.events.iter().filter(|e| e.seq > start_seq) {
            state.apply_event(evt)?;
        }

        let data = state.encode()?;
        Ok(Projection {
            tenant_id:     state.tenant_id,
            projection_id: state.projection_id,
            version:       state.version,
            data,
        })
    }

    fn apply_delta(current: Projection, evt: Event) -> Result<Projection, EngineError> {
        // Decode current projection header
        if current.data.len() < HEADER_SIZE {
            return Err(EngineError::EncodeFailed("truncated projection".to_string()));
        }
        let magic = u32::from_le_bytes(current.data[0..4].try_into().unwrap());
        if magic != MAGIC {
            return Err(EngineError::EncodeFailed("invalid magic bytes".to_string()));
        }

        let item_count = u32::from_le_bytes(current.data[16..20].try_into().unwrap()) as usize;
        let mut state = ProjState {
            tenant_id:     current.tenant_id,
            projection_id: current.projection_id,
            version:       current.version,
            counters:      [(0, 0); 32],
            counter_len:   item_count.min(32),
        };

        // Reload counter array from encoded bytes
        let items_start = HEADER_SIZE;
        for i in 0..state.counter_len {
            let base = items_start + i * 12;
            if base + 12 > current.data.len() { break; }
            let k = u32::from_le_bytes(current.data[base..base+4].try_into().unwrap());
            let v = i64::from_le_bytes(current.data[base+4..base+12].try_into().unwrap());
            state.counters[i] = (k, v);
        }

        state.apply_event(&evt)?;
        let data = state.encode()?;

        Ok(Projection {
            tenant_id:     evt.tenant_id,
            projection_id: evt.projection_id,
            version:       evt.seq,
            data,
        })
    }

    fn decode(data: Vec<u8>) -> Result<String, EngineError> {
        if data.len() < HEADER_SIZE {
            return Err(EngineError::EncodeFailed("too short".to_string()));
        }
        let magic      = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let version    = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let tenant_id  = u32::from_le_bytes(data[8..12].try_into().unwrap());
        let proj_id    = u32::from_le_bytes(data[12..16].try_into().unwrap());
        let item_count = u32::from_le_bytes(data[16..20].try_into().unwrap());

        if magic != MAGIC {
            return Err(EngineError::EncodeFailed(format!("bad magic: 0x{:08X}", magic)));
        }

        let mut out = format!(
            "{{\"magic\":\"0x{:08X}\",\"tenant_id\":{},\"projection_id\":{},\"version\":{},\"items\":[",
            magic, tenant_id, proj_id, version
        );
        for i in 0..(item_count as usize).min(32) {
            let base = HEADER_SIZE + i * 12;
            if base + 12 > data.len() { break; }
            let k = u32::from_le_bytes(data[base..base+4].try_into().unwrap());
            let v = i64::from_le_bytes(data[base+4..base+12].try_into().unwrap());
            if i > 0 { out.push(','); }
            let entry = format!("{{\"key\":{},\"value\":{}}}", k, v);
            out.push_str(&entry);
        }
        out.push_str("]}");
        Ok(out)
    }
}

export!(Engine);
