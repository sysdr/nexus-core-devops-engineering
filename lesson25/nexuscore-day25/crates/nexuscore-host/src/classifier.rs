//! Wasm component pool for per-tenant classifiers.
//!
//! Design goals:
//!   - Single wasmtime Engine shared across all instances (shared code cache)
//!   - One Store per tenant (isolated linear memory, isolated state)
//!   - Pool backed by parking_lot::RwLock (fast reads, rare writes)

use anyhow::{Context, Result};
use std::{collections::HashMap, path::PathBuf, time::Instant};
use tracing::{debug, info, warn};
use wasmtime::{
    component::{Component, Linker},
    Engine, Store,
};
use wasmtime_wasi::{ResourceTable, WasiCtx, WasiCtxBuilder, WasiView};

// ─── Public Tag type (mirrors WIT enum) ──────────────────────────────────────
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    FraudSignal,
    ChurnRisk,
    HighValue,
    Anomaly,
    Pass,
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tag::FraudSignal => write!(f, "fraud-signal"),
            Tag::ChurnRisk   => write!(f, "churn-risk"),
            Tag::HighValue   => write!(f, "high-value"),
            Tag::Anomaly     => write!(f, "anomaly"),
            Tag::Pass        => write!(f, "pass"),
        }
    }
}

#[derive(Debug)]
pub struct Classification {
    pub tag: Tag,
    pub confidence: f32,
    pub latency_ns: u64,
}

// ─── WASI host state ──────────────────────────────────────────────────────────
pub struct HostState {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for HostState {
    fn ctx(&mut self) -> &mut WasiCtx { &mut self.ctx }
    fn table(&mut self) -> &mut ResourceTable { &mut self.table }
}

// ─── Component pool ───────────────────────────────────────────────────────────
pub struct ComponentPool {
    engine: Engine,
    linker: Linker<HostState>,
    /// tenant_id → pre-compiled Component (code only, no instance state)
    components: HashMap<String, Component>,
    /// tenant_id → active Store+Instance (holds linear memory, call state)
    instances: HashMap<String, (Store<HostState>, wasmtime::component::Instance)>,
}

impl ComponentPool {
    pub async fn new() -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.async_support(true);
        config.wasm_component_model(true);
        // Enable epoch interruption — prevents a runaway tenant from
        // blocking the dispatcher indefinitely.
        config.epoch_interruption(true);

        let engine = Engine::new(&config)?;
        let mut linker: Linker<HostState> = Linker::new(&engine);
        wasmtime_wasi::add_to_linker_async(&mut linker)?;

        info!("ComponentPool initialised (epoch interruption: ON)");

        Ok(Self {
            engine,
            linker,
            components: HashMap::new(),
            instances: HashMap::new(),
        })
    }

    /// Load (or reload) a tenant's Wasm component from a .wasm file.
    /// This compiles the component — call ahead of time, not on the hot path.
    pub fn load_tenant(&mut self, tenant_id: &str, wasm_path: &PathBuf) -> Result<()> {
        let bytes = std::fs::read(wasm_path)
            .with_context(|| format!("reading wasm for tenant {tenant_id}"))?;
        let component = Component::new(&self.engine, &bytes)
            .with_context(|| format!("compiling wasm for tenant {tenant_id}"))?;
        self.components.insert(tenant_id.to_string(), component);
        info!(%tenant_id, "Wasm component loaded and compiled");
        Ok(())
    }

    /// Classify a single Kafka record payload for a given tenant.
    /// Returns a synthetic classification when running in dev (no real Wasm).
    pub fn classify_dev(
        &mut self,
        tenant_id: &str,
        payload: &[u8],
        arrival_ns: u64,
    ) -> Classification {
        let t0 = Instant::now();

        // Dev mode: deterministic rule-based classifier
        // In production this dispatches into the Wasm component via WIT.
        let (tag, confidence) = classify_payload_rules(payload);

        let latency_ns = t0.elapsed().as_nanos() as u64;
        debug!(%tenant_id, tag = %tag, confidence, latency_ns, "classified");

        crate::metrics::record_classify_latency(tenant_id, latency_ns);

        Classification { tag, confidence, latency_ns }
    }
}

/// Lightweight rule-based classifier for the simulated dev path.
/// Emulates what would be in the Wasm component.
fn classify_payload_rules(payload: &[u8]) -> (Tag, f32) {
    // Parse as JSON value; fall back to bytes inspection.
    let s = std::str::from_utf8(payload).unwrap_or("");

    // Rule tree (priority order)
    if s.contains("\"amount\":") {
        if let Some(amount) = extract_json_f64(s, "amount") {
            if amount > 9000.0 { return (Tag::FraudSignal, 0.91); }
            if amount > 500.0  { return (Tag::HighValue,   0.82); }
        }
    }
    if s.contains("\"churn_score\":") {
        if let Some(score) = extract_json_f64(s, "churn_score") {
            if score > 0.75 { return (Tag::ChurnRisk, score as f32); }
        }
    }
    // Byte-level anomaly: unexpected null bytes or trailing garbage
    let null_ratio = payload.iter().filter(|&&b| b == 0).count() as f32
        / payload.len().max(1) as f32;
    if null_ratio > 0.1 { return (Tag::Anomaly, 0.88); }

    (Tag::Pass, 1.0)
}

fn extract_json_f64(s: &str, key: &str) -> Option<f64> {
    let needle = format!("\"{}\":", key);
    let pos = s.find(&needle)? + needle.len();
    let rest = s[pos..].trim_start();
    rest.split(|c: char| c == ',' || c == '}')
        .next()?
        .trim()
        .parse()
        .ok()
}
