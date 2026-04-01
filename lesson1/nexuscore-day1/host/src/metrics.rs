// Prometheus metrics exposition for NexusCore
use prometheus::{
    register_counter_vec, register_histogram_vec, register_gauge,
    CounterVec, HistogramVec, Gauge, Encoder, TextEncoder,
};
use axum::{
    Router, routing::get,
    response::{Html, IntoResponse, Response},
    body::Body,
};
use std::sync::OnceLock;
use tower_http::cors::{Any, CorsLayer};

static QUERY_LATENCY: OnceLock<HistogramVec> = OnceLock::new();
static QUERY_TOTAL:   OnceLock<CounterVec>   = OnceLock::new();
static POOL_USED:     OnceLock<Gauge>        = OnceLock::new();

pub fn init() {
    let latency = QUERY_LATENCY.get_or_init(|| {
        register_histogram_vec!(
            "surreal_query_latency_seconds",
            "SurrealDB query latency by tenant and operation",
            &["tenant", "op"],
            vec![0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1, 0.5]
        ).unwrap()
    });
    let total = QUERY_TOTAL.get_or_init(|| {
        register_counter_vec!(
            "surreal_query_total",
            "Total SurrealDB queries",
            &["tenant", "op", "status"]
        ).unwrap()
    });
    let pool = POOL_USED.get_or_init(|| {
        register_gauge!("surreal_pool_connections_used", "Active pool connections").unwrap()
    });

    // Prime vector metrics so they show up on /metrics (even at zero) and therefore appear
    // in the HTML dashboard parser (which only reads sample lines, not HELP/TYPE headers).
    let _ = latency.get_metric_with_label_values(&["__system__", "heartbeat"]);
    let _ = total.get_metric_with_label_values(&["__system__", "heartbeat", "ok"]);
    pool.set(0.0);
}

pub fn record_query(tenant: &str, op: &str, latency_secs: f64, ok: bool) {
    if let Some(h) = QUERY_LATENCY.get() {
        h.with_label_values(&[tenant, op]).observe(latency_secs);
    }
    if let Some(c) = QUERY_TOTAL.get() {
        c.with_label_values(&[tenant, op, if ok { "ok" } else { "err" }]).inc();
    }
}

pub fn set_pool_used(n: f64) {
    if let Some(g) = POOL_USED.get() {
        g.set(n);
    }
}

fn build_router() -> Router {
    Router::new()
        .route("/", get(dashboard_handler))
        .route("/metrics", get(metrics_handler))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
}

/// Dedicated OS thread + Tokio runtime so port 9090 is open before SurrealDB connects.
pub fn spawn_background(port: u16) -> std::thread::JoinHandle<()> {
    init();
    std::thread::Builder::new()
        .name("nexuscore-metrics".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("metrics tokio runtime");
            rt.block_on(async move {
                // Ensure the dashboard is never "all zeros" even if SurrealDB fails to connect.
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        record_query("__system__", "heartbeat", 0.001, true);
                    }
                });
                run_http_server(port).await
            });
        })
        .expect("spawn metrics thread")
}

async fn run_http_server(port: u16) {
    let app = build_router();
    let addr = format!("0.0.0.0:{}", port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[nexuscore-metrics] cannot bind {}: {}", addr, e);
            return;
        }
    };
    tracing::info!(
        target: "nexuscore",
        "dashboard + metrics listening on http://127.0.0.1:{}/ (bound {})",
        port,
        addr
    );
    if let Err(e) = axum::serve(listener, app).await {
        eprintln!("[nexuscore-metrics] server stopped: {}", e);
    }
}

#[allow(dead_code)]
pub async fn serve(port: u16) {
    init();
    run_http_server(port).await;
}

async fn dashboard_handler() -> impl IntoResponse {
    let path = std::path::Path::new("viz/index.html");
    match tokio::fs::read_to_string(path).await {
        Ok(html) => Html(html).into_response(),
        Err(_) => Html(
            r#"<!DOCTYPE html><html><head><meta charset="utf-8"/><title>NexusCore</title></head>
<body style="background:#0f1419;color:#e6edf3;font-family:system-ui;padding:1.5rem">
<h1>Dashboard</h1><p>Could not read <code>viz/index.html</code> from the current directory. Run the host from the workspace root (<code>nexuscore-day1/</code>).</p>
<p><a href="/metrics">/metrics</a></p></body></html>"#,
        )
        .into_response(),
    }
}

async fn metrics_handler() -> Response<Body> {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buf = Vec::new();
    encoder.encode(&metric_families, &mut buf).unwrap();
    Response::builder()
        .header("Content-Type", encoder.format_type())
        .body(Body::from(buf))
        .unwrap()
}
