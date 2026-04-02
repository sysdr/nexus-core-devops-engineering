//! NexusCore Day 3 — demo host: CSR BFS queries, live dashboard metrics (Lesson 3 targets).

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use nexuscore_graph::GraphEngine;
use rand::Rng;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};

const TENANT_POOL: u32 = 256;
const LAT_SAMPLES_CAP: usize = 12_288;
const MAX_RESULTS_PER_QUERY: u32 = 64;

#[derive(Debug)]
struct QueryTask {
    tenant_id: u32,
    root_node_id: u32,
    max_depth: u8,
}

struct Agg {
    lat_ns: Vec<u64>,
    /// Last-second slot activity: slot -> last seen
    slot_last: HashMap<u8, Instant>,
    queries_total: u64,
    qps_window_start: Instant,
    qps_window_count: u64,
    last_mean_docs: f64,
}

impl Default for Agg {
    fn default() -> Self {
        Self {
            lat_ns: Vec::new(),
            slot_last: HashMap::new(),
            queries_total: 0,
            qps_window_start: Instant::now(),
            qps_window_count: 0,
            last_mean_docs: 0.0,
        }
    }
}

impl Agg {
    fn record_query(&mut self, tenant_id: u32, lat_ns: u64, docs_returned: usize) {
        self.queries_total += 1;
        self.qps_window_count += 1;
        if self.lat_ns.len() >= LAT_SAMPLES_CAP {
            self.lat_ns.drain(0..self.lat_ns.len() / 4);
        }
        self.lat_ns.push(lat_ns);
        let slot = (tenant_id % TENANT_POOL) as u8;
        self.slot_last.insert(slot, Instant::now());
        let alpha = 0.05;
        self.last_mean_docs = self.last_mean_docs * (1.0 - alpha) + (docs_returned as f64) * alpha;
    }

    fn prune_slots(&mut self) {
        let cutoff = Instant::now() - Duration::from_secs(1);
        self.slot_last.retain(|_, t| *t > cutoff);
    }

    fn p99_latency_us(&self) -> f64 {
        if self.lat_ns.is_empty() {
            return 0.0;
        }
        let mut v = self.lat_ns.clone();
        v.sort_unstable();
        let idx = ((v.len() as f64 * 0.99).floor() as usize).min(v.len() - 1);
        v[idx] as f64 / 1000.0
    }

    fn mean_latency_us(&self) -> f64 {
        if self.lat_ns.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.lat_ns.iter().sum();
        (sum as f64 / self.lat_ns.len() as f64) / 1000.0
    }

    fn roll_qps_window(&mut self) -> f64 {
        let now = Instant::now();
        let elapsed = now.duration_since(self.qps_window_start).as_secs_f64().max(0.001);
        if elapsed >= 1.0 {
            let qps = self.qps_window_count as f64 / elapsed;
            self.qps_window_start = now;
            self.qps_window_count = 0;
            qps
        } else {
            self.qps_window_count as f64 / elapsed
        }
    }
}

#[derive(Serialize)]
struct DashboardMetrics {
    /// Measured query throughput (CSR BFS + arena reads) over ~1s window.
    queries_per_sec: f64,
    /// p99 BFS path latency (Lesson target &lt;40µs when graph fits cache).
    p99_bfs_latency_us: f64,
    mean_bfs_latency_us: f64,
    /// Distinct tenant slots (tenant_id % 256) active in the last 1s.
    active_tenant_slots: u32,
    tenant_pool_size: u32,
    /// Average documents returned per query (capped by max_results).
    avg_docs_per_query: f64,
    /// Not sampled by this host — use `perf` / PMU in production (target &lt;2%).
    tlb_miss_pct: Option<f64>,
    /// Not attached in demo — requires loaded XDP program (target 10M pps).
    xdp_redirect_mps: Option<f64>,
    /// Arena bytes not accounted for by packed doc lengths (slab waste proxy).
    arena_fragmentation_pct: f64,
    /// io_uring batching not used in this build (lesson extension).
    io_uring_batch_avg: Option<f64>,
    graph_nodes: u32,
    graph_edges: u32,
    arena_bytes: u64,
    queries_total: u64,
    graph_loaded: bool,
    uptime_secs: f64,
}

struct AppState {
    graph: Option<Arc<GraphEngine>>,
    agg: Mutex<Agg>,
    started: Instant,
    avg_qps_smoothed: Mutex<f64>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    let rps: u64 = std::env::var("NEXUSCORE_RPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50_000);

    let port: u16 = std::env::var("NEXUSCORE_DASHBOARD_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(9847);

    let blob_path = std::env::var("NEXUSCORE_GRAPH_BLOB").unwrap_or_else(|_| "data/graph.blob".into());

    let graph = match std::fs::read(&blob_path) {
        Ok(bytes) => match GraphEngine::from_blob(&bytes) {
            Ok(g) => {
                info!(
                    "Loaded CSR graph from {} — nodes={} edges={} arena={}B frag={:.2}%",
                    blob_path,
                    g.n_nodes(),
                    g.nnz(),
                    g.arena_len(),
                    g.arena_fragmentation_pct()
                );
                Some(Arc::new(g))
            }
            Err(e) => {
                warn!("Invalid graph blob {}: {} — metrics will show graph_loaded=false", blob_path, e);
                None
            }
        },
        Err(e) => {
            warn!("Could not read {}: {} — run scripts/gen_graph.py first", blob_path, e);
            None
        }
    };

    let state = Arc::new(AppState {
        graph: graph.clone(),
        agg: Mutex::new(Agg {
            qps_window_start: Instant::now(),
            ..Default::default()
        }),
        started: Instant::now(),
        avg_qps_smoothed: Mutex::new(0.0),
    });

    let (tx, mut rx) = mpsc::channel::<QueryTask>(8192);

    let ingest = state.clone();
    tokio::spawn(async move {
        let mut interval =
            tokio::time::interval(std::time::Duration::from_nanos(1_000_000_000 / rps.max(1)));
        loop {
            interval.tick().await;
            let task = {
                let mut rng = rand::thread_rng();
                let g = ingest.graph.as_ref();
                let max_root = g.map(|x| x.n_nodes().max(1) - 1).unwrap_or(0);
                QueryTask {
                    tenant_id: rng.gen_range(0..50_000),
                    root_node_id: rng.gen_range(0..=max_root),
                    max_depth: rng.gen_range(1..=4),
                }
            };
            let _ = tx.send(task).await;
        }
    });

    let worker_state = state.clone();
    tokio::spawn(async move {
        while let Some(task) = rx.recv().await {
            if let Some(ref eng) = worker_state.graph {
                let t0 = Instant::now();
                let docs = eng.bfs_posts(
                    task.root_node_id,
                    task.max_depth.min(8),
                    MAX_RESULTS_PER_QUERY,
                );
                let ns = t0.elapsed().as_nanos() as u64;
                let mut a = worker_state.agg.lock().expect("agg lock");
                a.record_query(task.tenant_id, ns, docs.len());
            } else {
                let mut a = worker_state.agg.lock().expect("agg lock");
                a.record_query(task.tenant_id, 0, 0);
            }
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(index_html))
        .route("/api/metrics", get(metrics_json))
        .route("/healthz", get(|| async { "ok" }))
        .layer(cors)
        .with_state(state.clone());

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    info!(
        "NexusCore host — synthetic load ~{rps} q/s | dashboard http://127.0.0.1:{port}/"
    );
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind dashboard port");
    axum::serve(listener, app).await.expect("serve");
}

async fn index_html() -> impl IntoResponse {
    match tokio::fs::read_to_string("data/visualizer.html").await {
        Ok(html) => Html(html).into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            "Missing data/visualizer.html (run from nexuscore-day3 root)",
        )
            .into_response(),
    }
}

async fn metrics_json(State(st): State<Arc<AppState>>) -> Json<DashboardMetrics> {
    let mut a = st.agg.lock().expect("agg");
    a.prune_slots();
    let raw_qps = a.roll_qps_window();
    let mut smooth = st.avg_qps_smoothed.lock().expect("smooth");
    *smooth = *smooth * 0.7 + raw_qps * 0.3;

    let (nodes, edges, arena_b, frag) = st
        .graph
        .as_ref()
        .map(|g| {
            (
                g.n_nodes(),
                g.nnz(),
                g.arena_len(),
                g.arena_fragmentation_pct(),
            )
        })
        .unwrap_or((0, 0, 0, 0.0));

    let snap = DashboardMetrics {
        queries_per_sec: *smooth,
        p99_bfs_latency_us: a.p99_latency_us(),
        mean_bfs_latency_us: a.mean_latency_us(),
        active_tenant_slots: a.slot_last.len() as u32,
        tenant_pool_size: TENANT_POOL,
        avg_docs_per_query: a.last_mean_docs,
        tlb_miss_pct: None,
        xdp_redirect_mps: None,
        arena_fragmentation_pct: frag,
        io_uring_batch_avg: None,
        graph_nodes: nodes,
        graph_edges: edges,
        arena_bytes: arena_b,
        queries_total: a.queries_total,
        graph_loaded: st.graph.is_some(),
        uptime_secs: st.started.elapsed().as_secs_f64(),
    };
    Json(snap)
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn smoke_channel() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<u32>(4);
        tx.send(1).await.unwrap();
        assert_eq!(rx.recv().await, Some(1));
    }
}
