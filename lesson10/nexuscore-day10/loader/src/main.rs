//! NexusCore Day 10 — Userspace Loader
//!
//! Responsibilities:
//!   1. Load + pin the eBPF XDP program onto the target interface.
//!   2. Instantiate the WASI projection engine component per tenant.
//!   3. Poll the miss_ring ringbuf; on miss, rebuild projection via WASI,
//!      then write result back into the pinned proj_cache eBPF map.
//!   4. Expose a local HTTP endpoint (port 8080) for metrics + live visualization.

use anyhow::{Context, Result};
use clap::Parser;
use libbpf_rs::{Map, MapFlags, Object, ObjectBuilder};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{net::TcpListener, sync::Mutex};
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------
#[derive(Parser, Debug)]
#[command(name = "nexus-loader", about = "NexusCore Day 10 loader")]
struct Args {
    /// Network interface to attach XDP program to
    #[arg(short, long, default_value = "lo")]
    iface: String,

    /// Path to compiled eBPF object file
    #[arg(long, default_value = "ebpf-xdp/projection_xdp.bpf.o")]
    bpf_obj: PathBuf,

    /// Path to compiled WASI component (.wasm)
    #[arg(long, default_value = "projection-engine/target/wasm32-wasip2/release/projection_engine.wasm")]
    wasm: PathBuf,

    /// BPF filesystem mount point
    #[arg(long, default_value = "/sys/fs/bpf/nexuscore")]
    bpffs: PathBuf,

    /// Metrics HTTP port
    #[arg(long, default_value_t = 8080)]
    metrics_port: u16,
}

// ---------------------------------------------------------------------------
// Global counters (mirrored from eBPF percpu stats for userspace display)
// ---------------------------------------------------------------------------
#[derive(Default)]
struct Counters {
    cache_hits:   AtomicU64,
    cache_misses: AtomicU64,
    rebuilds_ok:  AtomicU64,
    rebuild_err:  AtomicU64,
    rebuild_ns:   AtomicU64, // cumulative nanoseconds for p50 approximation
}

// ---------------------------------------------------------------------------
// Projection map key/value (mirrors the C structs in projection_xdp.h)
// ---------------------------------------------------------------------------
#[repr(C)]
struct ProjKey {
    tenant_id:     u32,
    projection_id: u32,
}

#[repr(C)]
struct ProjValue {
    version:  u64,
    data_len: u32,
    _pad:     u32,
    data:     [u8; 4096],
}

// ---------------------------------------------------------------------------
// Fake event generator for demo purposes
// ---------------------------------------------------------------------------
fn generate_demo_events(tenant_id: u32, projection_id: u32, count: usize) -> Vec<Vec<u8>> {
    (0..count)
        .map(|i| {
            // "increment" event: [event_type_len(1)][event_type(9)][tenant(4)][proj(4)][seq(8)][key(4)][delta(4)]
            let mut payload = vec![0u8; 8];
            let key: u32 = (i % 8) as u32;
            let delta: i32 = 1;
            payload[0..4].copy_from_slice(&key.to_le_bytes());
            payload[4..8].copy_from_slice(&delta.to_le_bytes());

            let mut evt = vec![0u8; 30];
            evt[0] = tenant_id as u8;
            evt[4] = projection_id as u8;
            evt[8] = i as u8;
            evt[22..30].copy_from_slice(&payload);
            evt
        })
        .collect()
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("nexus_loader=debug,wasmtime=warn")
        .init();

    let args = Args::parse();
    let counters = Arc::new(Counters::default());

    // NOTE: This lesson can run fully in demo mode (no WASI runtime).
    info!(
        "Demo mode: skipping WASI component execution (WASM path would be {:?})",
        args.wasm
    );

    // ---- Load eBPF object ----
    info!("Loading eBPF object from {:?}", args.bpf_obj);
    let _bpf_obj: Option<Object> = if args.bpf_obj.exists() {
        std::fs::create_dir_all(&args.bpffs).ok();
        let mut builder = ObjectBuilder::default();
        builder.debug(true);
        match builder.open_file(&args.bpf_obj) {
            Ok(open_obj) => {
                match open_obj.load() {
                    Ok(mut obj) => {
                        // Attach XDP to interface
                        if let Some(prog) = obj.prog_mut("xdp_proj_handler") {
                            if let Err(e) = prog.attach_xdp(
                                nix::net::if_::if_nametoindex(args.iface.as_str())
                                    .unwrap_or(1) as i32
                            ) {
                                warn!("XDP attach failed (need root + correct iface): {}", e);
                            } else {
                                info!("XDP attached to {}", args.iface);
                            }
                        }
                        Some(obj)
                    }
                    Err(e) => { warn!("eBPF load failed: {}", e); None }
                }
            }
            Err(e) => { warn!("eBPF open failed: {}", e); None }
        }
    } else {
        warn!("eBPF object not found — running in demo mode");
        None
    };

    // ---- Open pinned proj_cache map ----
    let proj_cache_path = args.bpffs.join("proj_cache");
    let proj_cache: Option<Map> = if proj_cache_path.exists() {
        match Map::from_pinned_path(&proj_cache_path) {
            Ok(m) => { info!("Opened pinned proj_cache map"); Some(m) }
            Err(e) => { warn!("proj_cache map open: {}", e); None }
        }
    } else {
        warn!("proj_cache not pinned yet — projection writes will be simulated");
        None
    };
    let proj_cache = Arc::new(Mutex::new(proj_cache));

    // ---- Spawn metrics HTTP server ----
    let counters_http = counters.clone();
    let metrics_port = args.metrics_port;
    tokio::spawn(async move {
        if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{}", metrics_port)).await {
            info!("Metrics server: http://127.0.0.1:{}/metrics", metrics_port);
            loop {
                if let Ok((mut stream, _)) = listener.accept().await {
                    let c = counters_http.clone();
                    tokio::spawn(async move {
                        use tokio::io::AsyncWriteExt;
                        let hits   = c.cache_hits.load(Ordering::Relaxed);
                        let misses = c.cache_misses.load(Ordering::Relaxed);
                        let rebuilds = c.rebuilds_ok.load(Ordering::Relaxed);
                        let err    = c.rebuild_err.load(Ordering::Relaxed);
                        let avg_ns = if rebuilds > 0 {
                            c.rebuild_ns.load(Ordering::Relaxed) / rebuilds
                        } else { 0 };
                        let total = hits + misses;
                        let hit_rate = if total > 0 { hits * 100 / total } else { 0 };

                        let body = serde_json::json!({
                            "cache_hits": hits,
                            "cache_misses": misses,
                            "hit_rate_pct": hit_rate,
                            "rebuilds_ok": rebuilds,
                            "rebuild_errors": err,
                            "avg_rebuild_ns": avg_ns,
                        }).to_string();

                        let resp = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\n\r\n{}",
                            body.len(), body
                        );
                        let _ = stream.write_all(resp.as_bytes()).await;
                    });
                }
            }
        }
    });

    // ---- Main loop: simulate miss_ring processing ----
    info!("Entering projection rebuild loop (Ctrl+C to stop)...");
    let mut interval = tokio::time::interval(Duration::from_millis(50));

    for _ in 0u64.. {
        interval.tick().await;

        // In a real deployment, we'd poll miss_ring via libbpf RingBuffer callback.
        // Here we simulate cache misses for 100 random tenants per tick.
        for tenant_id in 0u32..100 {
            counters.cache_misses.fetch_add(1, Ordering::Relaxed);

            let t_start = Instant::now();

            // Demo projection builder (mirrors WASI output format)
            let proj_data: Vec<u8> = build_demo_projection(tenant_id, 0, (tenant_id * 7 + 13) as u64);

            let elapsed_ns = t_start.elapsed().as_nanos() as u64;
            counters.rebuilds_ok.fetch_add(1, Ordering::Relaxed);
            counters.rebuild_ns.fetch_add(elapsed_ns, Ordering::Relaxed);

            // Write to eBPF map if available
            let mut cache_guard = proj_cache.lock().await;
            if let Some(ref mut map) = *cache_guard {
                let key = ProjKey { tenant_id, projection_id: 0 };
                let key_bytes = unsafe {
                    std::slice::from_raw_parts(
                        &key as *const _ as *const u8,
                        std::mem::size_of::<ProjKey>(),
                    )
                };
                let mut value = ProjValue {
                    version:  42,
                    data_len: proj_data.len() as u32,
                    _pad:     0,
                    data:     [0u8; 4096],
                };
                let copy_len = proj_data.len().min(4096);
                value.data[..copy_len].copy_from_slice(&proj_data[..copy_len]);
                let val_bytes = unsafe {
                    std::slice::from_raw_parts(
                        &value as *const _ as *const u8,
                        std::mem::size_of::<ProjValue>(),
                    )
                };
                if let Err(e) = map.update(key_bytes, val_bytes, MapFlags::ANY) {
                    error!("Map update failed for tenant {}: {}", tenant_id, e);
                    counters.rebuild_err.fetch_add(1, Ordering::Relaxed);
                } else {
                    counters.cache_hits.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                // Simulate the write latency
                counters.cache_hits.fetch_add(1, Ordering::Relaxed);
            }
        }

        // Print live stats to terminal
        let hits   = counters.cache_hits.load(Ordering::Relaxed);
        let misses = counters.cache_misses.load(Ordering::Relaxed);
        let rebuilds = counters.rebuilds_ok.load(Ordering::Relaxed);
        let total  = hits + misses;
        let hit_pct = if total > 0 { hits * 100 / total } else { 0 };
        let avg_ns = if rebuilds > 0 {
            counters.rebuild_ns.load(Ordering::Relaxed) / rebuilds
        } else { 0 };

        print!(
            "\r  hits={:<8} misses={:<6} hit%={:<3} rebuilds={:<8} avg_rebuild={:<7}ns",
            hits, misses, hit_pct, rebuilds, avg_ns
        );
        use std::io::Write;
        std::io::stdout().flush().ok();
    }

    Ok(())
}

/// Build a demo projection in the NexusCore Flatbuffer format
fn build_demo_projection(tenant_id: u32, projection_id: u32, version: u64) -> Vec<u8> {
    const MAGIC: u32 = 0x4E435052;
    const HEADER: usize = 24;
    let n_items: usize = 4;
    let item_bytes = n_items * 12;
    let mut buf = vec![0u8; HEADER + item_bytes];

    buf[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    buf[4..8].copy_from_slice(&(version as u32).to_le_bytes());
    buf[8..12].copy_from_slice(&tenant_id.to_le_bytes());
    buf[12..16].copy_from_slice(&projection_id.to_le_bytes());
    buf[16..20].copy_from_slice(&(n_items as u32).to_le_bytes());
    buf[20..24].copy_from_slice(&(item_bytes as u32).to_le_bytes());

    for i in 0..n_items {
        let base = HEADER + i * 12;
        let key: u32 = i as u32;
        let val: i64 = (tenant_id as i64 * 100 + i as i64) * (version as i64 + 1);
        buf[base..base+4].copy_from_slice(&key.to_le_bytes());
        buf[base+4..base+12].copy_from_slice(&val.to_le_bytes());
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::build_demo_projection;

    #[test]
    fn demo_projection_non_empty() {
        let b = build_demo_projection(1, 0, 2);
        assert!(b.len() > 24);
        assert_eq!(&b[0..4], &0x4E435052u32.to_le_bytes());
    }
}
