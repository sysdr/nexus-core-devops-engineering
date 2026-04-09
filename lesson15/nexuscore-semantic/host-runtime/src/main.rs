//! NexusCore Day 15 — Wasmtime Host Runtime
//!
//! All tenant Wasm components run in ONE OS process.
//! The embedding model (wasi:nn Graph) is loaded ONCE — shared across
//! all components via capability-based resource handles.
//! iTLB footprint: single process = no CR3 reload on tenant switch.

use anyhow::{Context, Result};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Instant};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::{UnixListener, UnixStream},
};
use wasmtime::{
    component::{Component, Linker},
    Config, Engine, Store,
};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi_nn::WasiNnCtx;

wasmtime::component::bindgen!({
    path: "../wit/world.wit",
    world: "semantic-index",
    async: true,
});

struct TenantState {
    wasi: wasmtime_wasi::WasiCtx,
    wasi_nn: WasiNnCtx,
}
impl wasmtime_wasi::WasiView for TenantState {
    fn ctx(&mut self) -> &mut wasmtime_wasi::WasiCtx {
        &mut self.wasi
    }
}
impl wasmtime_wasi_nn::WasiNnView for TenantState {
    fn ctx(&mut self) -> &mut WasiNnCtx {
        &mut self.wasi_nn
    }
}

struct TenantEntry {
    instance: SemanticIndex,
    store: Store<TenantState>,
}

type TenantMap = Arc<DashMap<u32, tokio::sync::Mutex<TenantEntry>>>;

#[derive(Deserialize)]
#[serde(tag = "op")]
enum Req {
    #[serde(rename = "ingest")]
    Ingest { tenant_id: u32, text: String },
    #[serde(rename = "query")]
    Query { tenant_id: u32, text: String, top_k: u32 },
    #[serde(rename = "stats")]
    Stats { tenant_id: u32 },
}

#[derive(Serialize)]
#[serde(untagged)]
enum Resp {
    Ingest { id: u64, latency_us: u64 },
    Query { results: Vec<(u64, f32)>, latency_us: u64 },
    Stats { total: u64, layers: u32, mem_bytes: u64 },
    Err { error: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    let wasm_path = std::env::var("NEXUS_WASM")
        .unwrap_or_else(|_| "../semantic-index/target/wasm32-wasip2/release/semantic_index.wasm".into());
    let sock_path = std::env::var("NEXUS_HOST_SOCK").unwrap_or_else(|_| "/tmp/nexuscore-host.sock".into());

    eprintln!("[host] Engine init + AOT compile: {}", wasm_path);
    let mut cfg = Config::new();
    cfg.async_support(true)
        .wasm_component_model(true)
        .cranelift_opt_level(wasmtime::OptLevel::SpeedAndSize);
    let engine = Engine::new(&cfg)?;
    let component = Component::from_file(&engine, &wasm_path)
        .context("load component — run 'cargo component build --release' first")?;
    eprintln!("[host] AOT complete. Spawning tenants on demand (cold start ~1-2ms each).");

    let mut linker: Linker<TenantState> = Linker::new(&engine);
    wasmtime_wasi::add_to_linker_async(&mut linker)?;
    wasmtime_wasi_nn::wit::ML::add_to_linker(&mut linker, |s| s)?;

    let tenants: TenantMap = Arc::new(DashMap::new());

    let _ = std::fs::remove_file(&sock_path);
    let listener = UnixListener::bind(&sock_path)?;
    eprintln!("[host] Unix socket: {}", sock_path);

    loop {
        let (stream, _) = listener.accept().await?;
        let tenants = tenants.clone();
        let engine = engine.clone();
        let component = component.clone();
        let linker = linker.clone();
        tokio::spawn(async move {
            if let Err(e) = handle(stream, tenants, engine, component, linker).await {
                eprintln!("[host] handler error: {e}");
            }
        });
    }
}

async fn handle(
    stream: UnixStream,
    tenants: TenantMap,
    engine: Engine,
    component: Component,
    linker: Linker<TenantState>,
) -> Result<()> {
    let mut rdr = BufReader::new(stream);
    let mut line = String::new();
    rdr.read_line(&mut line).await?;
    let req: Req = serde_json::from_str(line.trim())?;

    let resp = match req {
        Req::Ingest { tenant_id, text } => {
            let e = ensure(&tenants, tenant_id, &engine, &component, &linker).await?;
            let mut g = e.lock().await;
            let t0 = Instant::now();
            match g.instance.nexus_semantic_ingest(&mut g.store, &text).await? {
                Ok(id) => Resp::Ingest {
                    id,
                    latency_us: t0.elapsed().as_micros() as u64,
                },
                Err(e) => Resp::Err { error: e },
            }
        }
        Req::Query { tenant_id, text, top_k } => {
            let e = ensure(&tenants, tenant_id, &engine, &component, &linker).await?;
            let mut g = e.lock().await;
            let t0 = Instant::now();
            match g.instance.nexus_semantic_query(&mut g.store, &text, top_k).await? {
                Ok(results) => Resp::Query {
                    results,
                    latency_us: t0.elapsed().as_micros() as u64,
                },
                Err(e) => Resp::Err { error: e },
            }
        }
        Req::Stats { tenant_id } => {
            let e = ensure(&tenants, tenant_id, &engine, &component, &linker).await?;
            let mut g = e.lock().await;
            let s = g.instance.nexus_semantic_stats(&mut g.store).await?;
            Resp::Stats {
                total: s.total_vectors,
                layers: s.index_layers,
                mem_bytes: s.memory_bytes,
            }
        }
    };
    eprintln!("[host] {}", &serde_json::to_string(&resp)?[..]);
    Ok(())
}

async fn ensure(
    tenants: &TenantMap,
    tid: u32,
    engine: &Engine,
    component: &Component,
    linker: &Linker<TenantState>,
) -> Result<Arc<tokio::sync::Mutex<TenantEntry>>> {
    if !tenants.contains_key(&tid) {
        let t0 = Instant::now();
        let nn_ctx = WasiNnCtx::new([]);
        let wasi = WasiCtxBuilder::new().inherit_stdio().build();
        let mut store = Store::new(engine, TenantState { wasi, wasi_nn: nn_ctx });
        let (inst, _) = SemanticIndex::instantiate_async(&mut store, component, linker).await?;
        eprintln!("[host] tenant {} cold start: {:.1}ms", tid, t0.elapsed().as_secs_f64() * 1000.0);
        tenants.insert(tid, tokio::sync::Mutex::new(TenantEntry { instance: inst, store }));
    }
    Ok(tenants.get(&tid).unwrap().clone())
}
