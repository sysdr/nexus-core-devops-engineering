//! Web dashboard — default bind 0.0.0.0:3030 (WSL2 + Windows browser friendly).

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use dashboard::{lesson_default_histograms, HistogramReport, LoadClientMetrics, MetricsPayload};
use std::path::Path;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

struct AppState {
    histograms: RwLock<Vec<HistogramReport>>,
    load_client: RwLock<Option<LoadClientMetrics>>,
    surreal_reachable: RwLock<bool>,
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("DASHBOARD_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3030);
    // 0.0.0.0: reachable from host browser when running inside WSL2 (127.0.0.1-only often fails from Windows).
    let host = std::env::var("DASHBOARD_HOST").unwrap_or_else(|_| "0.0.0.0".into());

    let now = Utc::now();
    let state = Arc::new(AppState {
        histograms: RwLock::new(lesson_default_histograms(&now)),
        load_client: RwLock::new(None),
        surreal_reachable: RwLock::new(false),
    });

    let state_bg = Arc::clone(&state);
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            tick.tick().await;
            let ok = TcpStream::connect("127.0.0.1:8000").await.is_ok();
            *state_bg.surreal_reachable.write().await = ok;
        }
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(index))
        .route("/api/metrics", get(api_metrics))
        .route("/api/ingest/load", post(ingest_load))
        .route("/api/ingest/histogram", post(ingest_histogram))
        .layer(cors)
        .with_state(state);

    let addr = format!("{host}:{port}");
    println!("NexusCore web dashboard listening on http://{addr}/");
    println!("  Open in browser: http://127.0.0.1:{port}/ or http://localhost:{port}/");
    println!("  (Keep this process running — connection refused means the server is not started.)");
    println!("  POST /api/ingest/load  — paste load-gen JSON report");
    println!("  POST /api/ingest/histogram — one HistogramReport JSON line");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("bind {addr}: {e} — try another port: DASHBOARD_PORT=3031"));
    axum::serve(listener, app).await.expect("serve");
}

/// Prefer `static/index.html` on disk so UI edits apply after refresh without rebuilding.
async fn index() -> Html<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("static/index.html");
    let html = tokio::fs::read_to_string(&path)
        .await
        .unwrap_or_else(|_| include_str!("../static/index.html").to_string());
    Html(html)
}

async fn api_metrics(State(state): State<Arc<AppState>>) -> Json<MetricsPayload> {
    let histograms = state.histograms.read().await.clone();
    let load_client = state.load_client.read().await.clone();
    let surrealdb_reachable = *state.surreal_reachable.read().await;
    Json(MetricsPayload {
        updated_rfc3339: Utc::now().to_rfc3339(),
        surrealdb_reachable,
        histograms,
        load_client,
    })
}

async fn ingest_load(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoadClientMetrics>,
) -> impl IntoResponse {
    *state.load_client.write().await = Some(body);
    (StatusCode::OK, "ok")
}

async fn ingest_histogram(
    State(state): State<Arc<AppState>>,
    Json(report): Json<HistogramReport>,
) -> impl IntoResponse {
    let mut list = state.histograms.write().await;
    if let Some(i) = list
        .iter()
        .position(|r| r.stack == report.stack && r.op == report.op)
    {
        list[i] = report;
    } else {
        list.push(report);
    }
    (StatusCode::OK, "ok")
}
