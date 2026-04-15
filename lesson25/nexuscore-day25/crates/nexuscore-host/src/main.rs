//! NexusCore Day 25 — Host process.
//! Pulls from Redpanda, dispatches to per-tenant Wasm classifiers,
//! produces tagged records back to Redpanda.
//!
//! Architecture:
//!   BPF ringbuf poll → Kafka consume → Wasm dispatch → Kafka produce

use anyhow::Result;
use parking_lot::RwLock;
use std::sync::Arc;
use tracing::{info, warn};

mod classifier;
mod metrics;
mod simulator; // eBPF simulator for dev environments without kernel access

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    // Log to stderr so simulator TUI (println! on stdout) is not interleaved with tracing lines.
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "nexuscore=info".to_string()),
        )
        .with_writer(std::io::stderr)
        .init();

    info!("NexusCore Day 25 — Real-Time Classification starting");

    // Start Prometheus metrics exporter on :9090
    metrics::install_recorder()?;

    // Component pool: shared across async tasks
    let pool = Arc::new(RwLock::new(classifier::ComponentPool::new().await?));

    // Check whether we have real eBPF available
    let use_ebpf = std::path::Path::new("/sys/fs/bpf").exists()
        && nix_user_is_root();

    if use_ebpf {
        info!("eBPF subsystem available — loading kernel probe");
        // Production: load BPF skeleton, attach kprobe, drive ring buffer
        // For full implementation see ebpf/src/nexuscore_ts.bpf.c
        // and libbpf-rs skeleton generation in the Makefile.
        warn!("Full eBPF path requires bpftool-generated skeleton — see Makefile");
    } else {
        info!("Running in simulator mode (no root / no eBPF)");
        simulator::run_simulated_pipeline(pool.clone()).await?;
    }

    Ok(())
}

fn nix_user_is_root() -> bool {
    unsafe { libc::getuid() == 0 }
}
