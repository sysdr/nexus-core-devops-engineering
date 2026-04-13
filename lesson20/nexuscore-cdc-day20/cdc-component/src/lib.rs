#![no_std]
extern crate alloc;

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};
use libm::sqrtf;

wit_bindgen::generate!({ world: "cdc-processor", path: "wit/cdc-processor.wit" });
use exports::nexuscore::cdc::processor::{CdcEvent, DeltaStats, Guest, QdrantUpsert};

static mut DELTA_CACHE: Option<BTreeMap<[u8; 16], Vec<f32>>> = None;
static DELTA_CACHE_HITS: AtomicU32 = AtomicU32::new(0);
const DELTA_THRESHOLD: f32 = 0.02;

fn delta_cache() -> &'static mut BTreeMap<[u8; 16], Vec<f32>> {
    unsafe { DELTA_CACHE.get_or_insert_with(BTreeMap::new) }
}

/// `wit-bindgen-rt` omits `cabi_realloc` Rust glue when `target_env = "p2"`; the bundled C
/// shim still needs `cabi_realloc_wit_bindgen_*` and an exported `cabi_realloc`.
#[cfg(all(target_arch = "wasm32", target_env = "p2"))]
mod cabi_realloc_wit {
    use ::alloc::alloc::{self, Layout};

    #[no_mangle]
    pub unsafe extern "C" fn cabi_realloc_wit_bindgen_0_26_0(
        old_ptr: *mut u8,
        old_len: usize,
        align: usize,
        new_len: usize,
    ) -> *mut u8 {
        let layout;
        let ptr = if old_len == 0 {
            if new_len == 0 {
                return align as *mut u8;
            }
            layout = Layout::from_size_align_unchecked(new_len, align);
            alloc::alloc(layout)
        } else {
            debug_assert_ne!(new_len, 0, "non-zero old_len requires non-zero new_len!");
            layout = Layout::from_size_align_unchecked(old_len, align);
            alloc::realloc(old_ptr, layout, new_len)
        };
        if ptr.is_null() {
            if cfg!(debug_assertions) {
                alloc::handle_alloc_error(layout);
            } else {
                core::arch::wasm32::unreachable();
            }
        }
        ptr
    }

    #[used]
    static _KEEP_CABI_REALLOC_EXPORT: unsafe extern "C" fn(
        *mut u8,
        usize,
        usize,
        usize,
    ) -> *mut u8 = {
        extern "C" {
            fn cabi_realloc(
                old_ptr: *mut u8,
                old_len: usize,
                align: usize,
                new_len: usize,
            ) -> *mut u8;
        }
        cabi_realloc
    };
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    let (mut dot, mut ma, mut mb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        ma += x * x;
        mb += y * y;
    }
    let d = sqrtf(ma) * sqrtf(mb);
    if d < 1e-8 {
        return 1.0;
    }
    1.0 - (dot / d)
}

fn embed(payload: &[u8]) -> Vec<f32> {
    let seed = payload
        .iter()
        .fold(0u64, |h, &b| h.wrapping_mul(1099511628211).wrapping_add(b as u64));
    (0..256u32)
        .map(|i| {
            let v = seed.wrapping_mul(i as u64 + 1).wrapping_add(0xDEADBEEF);
            ((v & 0xFFFF) as f32 / 32768.0) - 1.0
        })
        .collect()
}

struct CdcComponent;

impl Guest for CdcComponent {
    fn process(event: CdcEvent) -> Result<(Option<QdrantUpsert>, DeltaStats), String> {
        let new_vec = embed(&event.payload);
        let mut cache_key = [0u8; 16];
        let sl = &event.table_hash;
        let n = sl.len().min(16);
        cache_key[..n].copy_from_slice(&sl[..n]);
        let cache = delta_cache();
        let (distance, skipped) = if let Some(prev) = cache.get(&cache_key) {
            let d = cosine_distance(prev, &new_vec);
            let skip = d < DELTA_THRESHOLD;
            if skip {
                DELTA_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
            }
            (d, skip)
        } else {
            (1.0f32, false)
        };
        cache.insert(cache_key, new_vec.clone());
        let stats = DeltaStats {
            cosine_distance: distance,
            skipped,
            tokens_processed: event.payload.len() as u32,
        };
        if skipped {
            return Ok((None, stats));
        }
        let point_id = format!(
            "{:016x}{:016x}",
            event.tenant_id,
            u64::from_le_bytes(cache_key[..8].try_into().unwrap_or([0; 8]))
        );
        let payload_json = format!(
            r#"{{"tenant_id":{},"ts_ns":{},"tokens":{}}}"#,
            event.tenant_id,
            event.ts_ns,
            event.payload.len()
        );
        let upsert = QdrantUpsert {
            collection: format!("nexuscore_t{}", event.tenant_id),
            point_id,
            vector: new_vec,
            payload_json,
            tenant_id: event.tenant_id,
        };
        Ok((Some(upsert), stats))
    }

    fn flush_batch() -> u32 {
        0
    }
}

export!(CdcComponent);
