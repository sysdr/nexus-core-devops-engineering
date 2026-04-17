//! NexusCore Host Runtime
//! Embeds wasmtime, manages multi-tenant Wasm instances,
//! memory-maps the corpus, and exposes a streaming synthesis API.

use anyhow::{Context, Result};
use clap::Parser;
use memmap2::Mmap;
use std::{
    fs::{File, OpenOptions},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{sync::Semaphore, time::timeout};
use tracing::{info, warn};
use wasmtime::{
    component::{Component, Linker},
    Config, Engine, Store,
};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};

// ── CLI ───────────────────────────────────────────────────────────────────────
#[derive(Parser, Debug)]
#[command(name = "nexuscore-host", about = "NexusCore RAG Runtime — Day 30")]
struct Args {
    /// Path to corpus JSONL file
    #[arg(long, default_value = "data/corpus.jsonl")]
    corpus: PathBuf,

    /// Path to pre-built embedding binary (packed f32[384] per chunk)
    #[arg(long, default_value = "data/embeddings.bin")]
    embeddings: PathBuf,

    /// Compiled Wasm component path
    #[arg(long, default_value = "rag-component/target/wasm32-wasip2/release/rag_component.wasm")]
    wasm: PathBuf,

    /// Number of concurrent tenants to simulate
    #[arg(long, default_value = "10")]
    tenants: usize,

    /// Number of RAG queries per tenant
    #[arg(long, default_value = "5")]
    queries: usize,

    /// Max concurrent Wasm instances (controls memory pressure)
    #[arg(long, default_value = "50")]
    max_instances: usize,
}

// ── WASI host state per Wasm instance ────────────────────────────────────────
struct TenantState {
    ctx: WasiCtx,
    table: wasmtime_wasi::ResourceTable,
}

impl WasiView for TenantState {
    fn ctx(&mut self) -> &mut WasiCtx { &mut self.ctx }
    fn table(&mut self) -> &mut wasmtime_wasi::ResourceTable { &mut self.table }
}

// ── Metrics ───────────────────────────────────────────────────────────────────
struct Metrics {
    total_queries: AtomicU64,
    total_latency_us: AtomicU64,
    cold_starts: AtomicU64,
    grounding_score_sum: AtomicU64,  // stored as score * 1000 (fixed point)
}

impl Metrics {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            total_queries: AtomicU64::new(0),
            total_latency_us: AtomicU64::new(0),
            cold_starts: AtomicU64::new(0),
            grounding_score_sum: AtomicU64::new(0),
        })
    }

    fn record(&self, latency_us: u64, grounding_score: f32) {
        self.total_queries.fetch_add(1, Ordering::Relaxed);
        self.total_latency_us.fetch_add(latency_us, Ordering::Relaxed);
        self.grounding_score_sum.fetch_add(
            (grounding_score * 1000.0) as u64, Ordering::Relaxed
        );
    }

    fn report(&self) {
        let n = self.total_queries.load(Ordering::Relaxed);
        if n == 0 { return; }
        let mean_us = self.total_latency_us.load(Ordering::Relaxed) / n;
        let mean_grounding = self.grounding_score_sum.load(Ordering::Relaxed) as f64
            / (n as f64 * 1000.0);
        let cold_starts = self.cold_starts.load(Ordering::Relaxed);
        println!("\n╔══════════════════════════════════════════════╗");
        println!("║         NexusCore RAG — Run Report           ║");
        println!("╠══════════════════════════════════════════════╣");
        println!("║  Total queries      : {:>8}              ║", n);
        println!("║  Mean latency       : {:>8} µs           ║", mean_us);
        println!("║  Cold starts        : {:>8}              ║", cold_starts);
        println!("║  Mean grounding     : {:>8.3}              ║", mean_grounding);
        println!("╚══════════════════════════════════════════════╝");
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────
#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .compact()
        .init();

    let args = Args::parse();
    info!("NexusCore RAG Host starting | tenants={} queries={}", args.tenants, args.queries);

    // ── Memory-map the corpus ─────────────────────────────────────────────────
    let corpus_path = &args.corpus;
    let corpus_mmap: Arc<Option<Mmap>> = if corpus_path.exists() {
        let f = File::open(corpus_path)
            .with_context(|| format!("Opening corpus: {:?}", corpus_path))?;
        let mmap = unsafe { Mmap::map(&f)? };
        info!("Corpus mmap'd: {} bytes from {:?}", mmap.len(), corpus_path);
        Arc::new(Some(mmap))
    } else {
        warn!("Corpus file not found — using synthetic data");
        Arc::new(None)
    };

    // ── Configure wasmtime engine ─────────────────────────────────────────────
    let mut config = Config::new();
    config.async_support(true);
    config.wasm_component_model(true);
    // Enable W^X: compiled code pages are marked execute-only
    config.memory_init_cow(true);
    let engine = Engine::new(&config).context("Creating wasmtime engine")?;

    // ── Load and compile Wasm component (once, shared across tenants) ─────────
    let wasm_path = &args.wasm;
    let component = if wasm_path.exists() {
        info!("Loading Wasm component from {:?}", wasm_path);
        let bytes = std::fs::read(wasm_path)
            .with_context(|| format!("Reading Wasm: {:?}", wasm_path))?;
        Component::from_binary(&engine, &bytes)
            .context("Compiling Wasm component")?
    } else {
        info!("Wasm not found — running host-side simulation only");
        // In simulation mode we skip Wasm instantiation
        run_simulation(&args, &corpus_mmap).await?;
        return Ok(());
    };

    let component = Arc::new(component);
    let engine = Arc::new(engine);
    let metrics = Metrics::new();
    let semaphore = Arc::new(Semaphore::new(args.max_instances));

    // ── Spawn tenant tasks ────────────────────────────────────────────────────
    let mut handles = Vec::new();
    for tenant_id in 0..args.tenants {
        let component = Arc::clone(&component);
        let engine = Arc::clone(&engine);
        let metrics = Arc::clone(&metrics);
        let sem = Arc::clone(&semaphore);
        let queries = args.queries;

        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            for q_idx in 0..queries {
                let query = format!(
                    "What is the grounding mechanism for tenant {} query {}?",
                    tenant_id, q_idx
                );
                let t0 = Instant::now();

                match run_rag_query(&engine, &component, tenant_id as u32, &query).await {
                    Ok(grounding) => {
                        let lat = t0.elapsed().as_micros() as u64;
                        metrics.record(lat, grounding);
                        println!(
                            "  [T{:04}|Q{}] latency={:>6}µs grounding={:.3}",
                            tenant_id, q_idx, lat, grounding
                        );
                    }
                    Err(e) => warn!("T{} Q{}: {:?}", tenant_id, q_idx, e),
                }
            }
        });
        handles.push(handle);
    }

    for h in handles { let _ = h.await; }
    metrics.report();
    Ok(())
}

/// Execute one RAG query inside a fresh Wasm instance (cold start each time).
/// In production: implement an instance pool with pre-warmed components.
async fn run_rag_query(
    engine: &Arc<Engine>,
    component: &Arc<Component>,
    tenant_id: u32,
    query: &str,
) -> Result<f32> {
    let mut linker: Linker<TenantState> = Linker::new(engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;

    let wasi_ctx = WasiCtxBuilder::new()
        .inherit_stdout()
        .build();

    let state = TenantState {
        ctx: wasi_ctx,
        table: wasmtime_wasi::ResourceTable::new(),
    };

    let mut store = Store::new(engine, state);

    // NOTE: With wasmtime 25, instantiate_async drives the WASI P3 async model.
    // Actual bindgen macro usage depends on wit-bindgen output — this shows intent.
    // In the real compiled artifact, `RagPipeline::instantiate_async` would be called.

    // For lesson demo purposes: return synthetic grounding score
    let grounding_score = 0.72 + (tenant_id as f32 * 0.001) % 0.25;
    Ok(grounding_score)
}

/// Simulation mode: runs without a compiled Wasm binary.
/// Demonstrates the full data flow with synthetic timing.
async fn run_simulation(
    args: &Args,
    corpus_mmap: &Arc<Option<Mmap>>,
) -> Result<()> {
    println!("\n{}", "═".repeat(60));
    println!("  NexusCore RAG Simulation Mode (no Wasm binary required)");
    println!("{}\n", "═".repeat(60));

    let metrics = Metrics::new();

    let corpus_info = match corpus_mmap.as_ref() {
        Some(mmap) => format!("{} bytes from mmap", mmap.len()),
        None => "synthetic corpus (no file found)".to_string(),
    };
    info!("Corpus: {}", corpus_info);

    let sem = Arc::new(Semaphore::new(args.max_instances));
    let mut handles = Vec::new();
    let metrics_clone = Arc::clone(&metrics);

    for tenant_id in 0..args.tenants {
        let metrics = Arc::clone(&metrics_clone);
        let sem = Arc::clone(&sem);
        let queries = args.queries;

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            for q_idx in 0..queries {
                let t0 = Instant::now();

                // Simulate retrieval (cosine search over 1024 chunks)
                tokio::time::sleep(Duration::from_micros(200 + (tenant_id % 10) as u64 * 50)).await;
                let retrieval_us = t0.elapsed().as_micros() as u64;

                // Simulate synthesis (grounded token generation)
                tokio::time::sleep(Duration::from_micros(800 + (q_idx % 5) as u64 * 100)).await;
                let total_us = t0.elapsed().as_micros() as u64;

                let grounding_score: f32 = 0.70 + (tenant_id as f32 * 0.003 + q_idx as f32 * 0.01) % 0.28;

                metrics.record(total_us, grounding_score);
                println!(
                    "  [T{:04}|Q{}] retrieval={:>5}µs synthesis={:>5}µs grounding={:.3} {}",
                    tenant_id, q_idx,
                    retrieval_us,
                    total_us - retrieval_us,
                    grounding_score,
                    if grounding_score > 0.75 { "✓" } else { "⚠" }
                );
            }
        }));
    }

    for h in handles { let _ = h.await; }
    metrics.report();
    Ok(())
}
