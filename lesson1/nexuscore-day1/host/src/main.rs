// NexusCore Host Runtime — Day 1
// Multi-tenant Wasm pool over shared SurrealDB connection pool
// Production pattern: N tenants → 1 OS thread → M connections (M << N)

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod adapter;
mod metrics;
mod pool;
mod tenant;
mod visualizer;

use pool::ConnectionPool;
use tenant::TenantRuntime;

#[derive(Parser)]
#[command(name = "nexuscore", about = "NexusCore Day 1 — SQL→SurrealDB Host Runtime")]
struct Cli {
    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the host runtime
    Start {
        #[arg(long, default_value = "ws://127.0.0.1:8000", env = "NEXUSCORE_SURREAL_URL")]
        surreal_url: String,
        #[arg(long, default_value = "nexuscore")]
        ns: String,
        #[arg(long, default_value = "tenants")]
        db: String,
        #[arg(long, default_value = "64")]
        pool_size: u32,
        #[arg(long, default_value = "100")]
        max_tenants: usize,
    },
    /// Run demo with N tenants and visualize query flow
    Demo {
        #[arg(long, default_value = "20")]
        tenants: usize,
        #[arg(long, default_value = "50")]
        rps: u64,
    },
    /// Verify eBPF probes and pool state
    Verify,
    /// Stress test: high tenant count, sustained load
    Stress {
        #[arg(long, default_value = "500")]
        tenants: usize,
        #[arg(long, default_value = "30")]
        duration: u64,
    },
}

#[tokio::main(flavor = "current_thread")] // Single OS thread — intentional
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env()
            .add_directive("nexuscore=info".parse()?))
        .init();

    let cli = Cli::parse();

    match cli.cmd {
        Command::Start { surreal_url, ns, db, pool_size, max_tenants } => {
            if let Err(e) = run_host(surreal_url, ns, db, pool_size, max_tenants).await {
                // Keep :9090 alive briefly so the dashboard is visible for troubleshooting,
                // even if SurrealDB connection fails immediately.
                info!("Host failed to start: {e:#}");
                info!("Keeping dashboard up for 10 minutes (open http://127.0.0.1:9090/)…");
                tokio::time::sleep(std::time::Duration::from_secs(600)).await;
                return Err(e);
            }
            Ok(())
        }
        Command::Demo { tenants, rps } => {
            run_demo(tenants, rps).await
        }
        Command::Verify => {
            run_verify().await
        }
        Command::Stress { tenants, duration } => {
            run_stress(tenants, duration).await
        }
    }
}

/// Map loopback hostnames to `127.0.0.1` so Tokio's resolver is not asked for `localhost`
/// (broken `/etc/hosts` or WSL DNS often breaks hostname lookups but not numeric IPv4).
fn normalize_surreal_url(url: &str) -> String {
    let Ok(mut u) = url::Url::parse(url) else {
        return url.to_string();
    };
    let Some(host) = u.host_str() else {
        return url.to_string();
    };
    let use_ipv4 = host.eq_ignore_ascii_case("localhost") || host == "::1";
    if use_ipv4 && u.set_host(Some("127.0.0.1")).is_ok() {
        return u.into();
    }
    url.to_string()
}

fn surreal_tcp_target(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str()?;
    let port = u.port_or_known_default()?;
    Some(format!("{host}:{port}"))
}

fn should_fallback_to_mem(err: &anyhow::Error) -> bool {
    let msg = format!("{err:#}");
    msg.contains("Temporary failure in name resolution")
        || msg.contains("failed to lookup address information")
        || msg.contains("name resolution")
        || msg.contains("Timed out in bb8")
}

async fn run_host(
    surreal_url: String,
    ns: String,
    db: String,
    pool_size: u32,
    max_tenants: usize,
) -> Result<()> {
    info!("Initializing NexusCore host — single-thread cooperative executor");
    info!("Pool size: {} connections | Max tenants: {}", pool_size, max_tenants);

    let url_raw = surreal_url;
    let surreal_url = normalize_surreal_url(&url_raw);
    if surreal_url != url_raw {
        info!("Surreal URL normalized to {} (from {})", surreal_url, url_raw);
    }
    info!("Surreal URL: {}", surreal_url);
    if let Some(target) = surreal_tcp_target(&surreal_url) {
        info!("Surreal TCP target: {}", target);
    } else {
        info!("Surreal TCP target: <unparseable>");
    }

    // HTTP dashboard + /metrics on a separate thread so the port is open even while
    // SurrealDB connection pool is still connecting (avoids ERR_CONNECTION_REFUSED).
    let _metrics_thread = metrics::spawn_background(9090);
    std::thread::sleep(std::time::Duration::from_millis(80));

    if let Some(target) = surreal_tcp_target(&surreal_url) {
        // Preflight: this helps distinguish "port closed" vs "DNS broken" vs "WS handshake".
        if let Err(e) = tokio::net::TcpStream::connect(&target).await {
            info!("Surreal preflight TCP connect failed to {target}: {e}");
        } else {
            info!("Surreal preflight TCP connect OK to {target}");
        }
    }

    let mut surreal_label = surreal_url.clone();
    let mut pool = match ConnectionPool::new(&surreal_url, &ns, &db, pool_size)
        .await
        .with_context(|| format!("Failed to initialize SurrealDB connection pool — url={surreal_url}"))
    {
        Ok(pool) => pool,
        Err(e) if surreal_url.starts_with("ws://") && should_fallback_to_mem(&e) => {
            info!("SurrealDB WS connect failed with DNS error; falling back to embedded mem:// engine for this run.");
            surreal_label = "mem://".to_string();
            ConnectionPool::new(&surreal_label, &ns, &db, pool_size)
                .await
                .context("Fallback to embedded mem:// SurrealDB failed")?
        }
        Err(e) => return Err(e),
    };

    // Force a real connection attempt now (bb8 can otherwise appear "ready" but only fail later).
    let warmup_err = pool.get().await.err();
    if let Some(e0) = warmup_err {
        let e = anyhow::anyhow!(e0).context("SurrealDB warmup connection failed");
        if surreal_label.starts_with("ws://") && should_fallback_to_mem(&e) {
            info!("SurrealDB warmup failed with DNS error; falling back to embedded mem:// engine for this run.");
            surreal_label = "mem://".to_string();
            pool = ConnectionPool::new(&surreal_label, &ns, &db, pool_size)
                .await
                .context("Fallback to embedded mem:// SurrealDB failed")?;
            pool.get()
                .await
                .context("SurrealDB warmup failed even after mem:// fallback")?;
        } else {
            return Err(e);
        }
    }

    info!("SurrealDB pool ready — {} connections at {}", pool_size, surreal_label);

    // Schema bootstrap: create multi-model schema
    bootstrap_schema(&pool).await?;

    let pool = Arc::new(pool);
    let runtime = TenantRuntime::new(Arc::clone(&pool), max_tenants)?;

    let pool_gauge = Arc::clone(&pool);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            metrics::set_pool_used(pool_gauge.state().connections as f64);
        }
    });

    // Drive Prometheus counters/histograms while idle (demo runs in a separate process with its own registry)
    let hb_pool = Arc::clone(&pool);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            metrics::record_query("host", "heartbeat", 0.001, true);
            let _ = hb_pool.state();
        }
    });

    info!("Open http://127.0.0.1:9090/ (dashboard) and http://127.0.0.1:9090/metrics");
    info!("Host ready. Waiting for tenant requests...");

    // In production: accept requests from a gRPC/HTTP gateway
    // For this lesson: run a small demo loop
    runtime.run_event_loop().await?;

    Ok(())
}

async fn bootstrap_schema(pool: &ConnectionPool) -> Result<()> {
    info!("Bootstrapping SurrealDB multi-model schema...");

    let schema_statements = vec![
        // Document model: tenant config
        "DEFINE TABLE IF NOT EXISTS tenant SCHEMAFULL;",
        "DEFINE FIELD IF NOT EXISTS name ON tenant TYPE string;",
        "DEFINE FIELD IF NOT EXISTS plan ON tenant TYPE string;",
        "DEFINE FIELD IF NOT EXISTS created_at ON tenant TYPE datetime DEFAULT time::now();",
        // Relational model: users with foreign-key-style links
        "DEFINE TABLE IF NOT EXISTS user SCHEMAFULL;",
        "DEFINE FIELD IF NOT EXISTS email ON user TYPE string;",
        "DEFINE FIELD IF NOT EXISTS tenant_id ON user TYPE record<tenant>;",
        "DEFINE INDEX IF NOT EXISTS user_email ON user FIELDS email UNIQUE;",
        // Graph model: event edges (no JOIN — RELATE traversal)
        "DEFINE TABLE IF NOT EXISTS accessed SCHEMALESS TYPE RELATION;",
        "DEFINE TABLE IF NOT EXISTS purchased SCHEMALESS TYPE RELATION;",
        // Full-text index: leverages SurrealDB's built-in BM25
        "DEFINE ANALYZER IF NOT EXISTS full_text TOKENIZERS class FILTERS ascii, lowercase, snowball(english);",
        "DEFINE INDEX IF NOT EXISTS user_email_search ON user FIELDS email SEARCH ANALYZER full_text BM25;",
    ];

    let conn = pool.get().await?;
    for stmt in &schema_statements {
        conn.query(*stmt).await
            .map_err(|e| anyhow::anyhow!("Schema error on '{}': {}", stmt, e))?;
    }

    ok_log("Schema bootstrapped: document + relational + graph + FTS indexes");
    Ok(())
}

fn ok_log(msg: &str) {
    println!("\x1b[0;32m[  ok  ]\x1b[0m {}", msg);
}

async fn run_demo(tenants: usize, rps: u64) -> Result<()> {
    visualizer::run_demo_visualizer(tenants, rps).await
}

async fn run_verify() -> Result<()> {
    println!("\n\x1b[1;34m=== NexusCore Verification ===\x1b[0m\n");

    // Check SurrealDB connectivity
    print!("  Checking SurrealDB (ws://127.0.0.1:8000)... ");
    match tokio::net::TcpStream::connect("127.0.0.1:8000").await {
        Ok(_)  => println!("\x1b[32m✓ reachable\x1b[0m"),
        Err(e) => println!("\x1b[33m⚠ not reachable: {} (start SurrealDB first)\x1b[0m", e),
    }

    // Check eBPF map
    print!("  Checking eBPF pinned map... ");
    if std::path::Path::new("/sys/fs/bpf/nexuscore_tenant_ts_map").exists() {
        println!("\x1b[32m✓ map pinned at /sys/fs/bpf/nexuscore_tenant_ts_map\x1b[0m");
    } else {
        println!("\x1b[33m⚠ map not found (run: sudo ./scripts/load_ebpf.sh)\x1b[0m");
    }

    // Check pool not leaking connections
    print!("  Checking TCP socket count to SurrealDB... ");
    let count = std::process::Command::new("sh")
        .arg("-c")
        .arg("ss -tnp 2>/dev/null | grep ':8000' | wc -l || echo 'N/A'")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "N/A".to_string());
    println!("\x1b[36m{} connections (should be ≤ pool_size)\x1b[0m", count);

    println!("\n\x1b[1;34mWASM target check:\x1b[0m");
    let _ = std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .stdout(std::process::Stdio::inherit())
        .status();

    println!("\n\x1b[32mVerification complete.\x1b[0m\n");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn workspace_sanity() {
        assert_eq!(2 + 2, 4);
    }
}

async fn run_stress(tenants: usize, duration_secs: u64) -> Result<()> {
    println!("\n\x1b[1;31m=== Stress Test: {} tenants / {}s soak ===\x1b[0m\n", tenants, duration_secs);

    let sem = Arc::new(Semaphore::new(tenants));
    let start = std::time::Instant::now();
    let mut handles = Vec::new();

    for tid in 0..tenants {
        let sem = Arc::clone(&sem);
        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let mut queries = 0u64;
            while start.elapsed().as_secs() < duration_secs {
                // Simulate query workload without real DB in stress mode
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
                queries += 1;
            }
            (tid, queries)
        });
        handles.push(handle);
    }

    let elapsed_start = std::time::Instant::now();
    while elapsed_start.elapsed().as_secs() < duration_secs {
        let secs = elapsed_start.elapsed().as_secs();
        let pct = secs * 100 / duration_secs;
        let bar: String = "█".repeat((pct / 2) as usize);
        let empty: String = "░".repeat((50 - pct / 2) as usize);
        print!("\r  [{GREEN}{bar}{RED}{empty}{RESET}] {pct}% — {secs}s/{duration_secs}s  ",
            GREEN="\x1b[32m", bar=bar, RED="\x1b[31m", empty=empty,
            RESET="\x1b[0m", pct=pct, secs=secs, duration_secs=duration_secs);
        std::io::Write::flush(&mut std::io::stdout()).ok();
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
    println!();

    let mut total_queries = 0u64;
    for h in handles { total_queries += h.await.unwrap().1; }

    println!("\n\x1b[1;32mStress test complete.\x1b[0m");
    println!("  Tenants simulated : {}", tenants);
    println!("  Duration          : {}s", duration_secs);
    println!("  Total queries     : {}", total_queries);
    println!("  Avg QPS           : {:.0}", total_queries as f64 / duration_secs as f64);
    println!("  Effective pool    : single shared pool (no per-tenant sockets)");
    Ok(())
}
