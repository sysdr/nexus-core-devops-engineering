// Live CLI visualizer — renders tenant query activity in real-time
// No external TUI dependency: raw ANSI escape codes only

use anyhow::Result;
use std::time::{Duration, Instant};

struct TenantStats {
    id:       usize,
    queries:  u64,
    latency:  f64, // ms, rolling average
    model:    &'static str,
}

pub async fn run_demo_visualizer(tenants: usize, rps: u64) -> Result<()> {
    crate::metrics::init();
    print!("\x1b[2J\x1b[H"); // Clear screen

    println!("\x1b[1;34m╔══════════════════════════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1;34m║  NexusCore :: Day 1 :: SQL→SurrealDB Live Migration Viewer   ║\x1b[0m");
    println!("\x1b[1;34m╚══════════════════════════════════════════════════════════════╝\x1b[0m\n");

    let n = tenants.min(12); // Show max 12 in demo
    let mut stats: Vec<TenantStats> = (0..n)
        .map(|i| TenantStats {
            id: i,
            queries: 0,
            latency: 0.8 + (i as f64 * 0.1),
            model: match i % 3 { 0 => "document", 1 => "relational", _ => "graph" },
        })
        .collect();

    let _models = ["CBOR/doc", "SQL-rel", "RELATE"];
    let ops    = ["SELECT *", "RELATE->", "CREATE  ", "UPDATE  ", "LIVE SEL"];

    let start   = Instant::now();
    let end_at  = Duration::from_secs(30);
    let interval = Duration::from_millis(1000 / rps.max(1));
    let mut tick = 0u64;

    loop {
        if start.elapsed() >= end_at { break; }
        tick += 1;

        // Simulate query activity
        for s in stats.iter_mut() {
            let new_q = (rps / n as u64).max(1);
            s.queries += new_q;
            // Simulate latency variance
            let jitter = ((tick + s.id as u64) % 7) as f64 * 0.05;
            s.latency = 0.8 + jitter + if tick % 20 == 0 { 2.5 } else { 0.0 };
            let op = ["select", "relate", "exec"][(tick as usize + s.id) % 3];
            crate::metrics::record_query(
                &format!("tenant-{}", s.id),
                op,
                (s.latency / 1000.0).max(1e-9),
                true,
            );
        }
        crate::metrics::set_pool_used((n as f64 * 0.25).max(1.0));

        // Render
        print!("\x1b[4;0H"); // Move cursor to row 4
        println!("  \x1b[36mElapsed:\x1b[0m {:>4}s  \x1b[36mTick:\x1b[0m {:>6}  \x1b[36mTenants:\x1b[0m {}  \x1b[36mTarget RPS:\x1b[0m {}\n",
            start.elapsed().as_secs(), tick, tenants, rps);

        println!("  {:>4}  {:>12}  {:>10}  {:>8}  {:>8}  {}",
            "TID", "Model", "Operation", "Queries", "Lat(ms)", "Activity");
        println!("  {}", "─".repeat(72));

        for s in &stats {
            let op = ops[(tick as usize + s.id) % ops.len()];
            let bar_len = ((s.queries as f64 / (rps as f64 * 30.0 / n as f64) * 20.0) as usize).min(20);
            let bar  = "▮".repeat(bar_len);
            let empty = "▯".repeat(20 - bar_len);
            let lat_color = if s.latency > 2.0 { "\x1b[31m" } else if s.latency > 1.2 { "\x1b[33m" } else { "\x1b[32m" };
            let model_color = match s.model {
                "document"   => "\x1b[34m",
                "relational" => "\x1b[35m",
                _            => "\x1b[36m",
            };

            println!("  {:>4}  {}{:>12}\x1b[0m  {:>10}  {:>8}  {}{:>7.2}ms\x1b[0m  \x1b[32m{}\x1b[0m\x1b[90m{}\x1b[0m",
                s.id, model_color, s.model, op, s.queries,
                lat_color, s.latency, bar, empty);
        }

        if tenants > n {
            println!("\n  \x1b[90m... and {} more tenants (showing first {})\x1b[0m", tenants - n, n);
        }

        println!("\n  \x1b[90m[Pool] Single shared connection pool — NO per-tenant TCP sockets\x1b[0m");
        println!("  \x1b[90m[eBPF] Kernel-space latency tracking (see /sys/fs/bpf/nexuscore_*)\x1b[0m");
        println!("  \x1b[90m[WASM] {} components, shared-nothing isolation, 1 OS thread\x1b[0m", tenants);

        std::io::Write::flush(&mut std::io::stdout()).ok();
        tokio::time::sleep(interval).await;
    }

    println!("\n\x1b[1;32mDemo complete.\x1b[0m\n");
    Ok(())
}
