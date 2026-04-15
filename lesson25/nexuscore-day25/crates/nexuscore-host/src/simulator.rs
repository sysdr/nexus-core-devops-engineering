//! Simulated Redpanda + BPF pipeline for dev environments.
//! Generates realistic synthetic events at configurable RPS.

use crate::{classifier::ComponentPool, metrics};
use anyhow::Result;
use parking_lot::RwLock;
use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tracing::info;

/// Number of simulated tenants
const TENANT_COUNT: usize = 50;
/// Target events per second across all tenants
const TARGET_RPS: u64 = 10_000;

pub async fn run_simulated_pipeline(pool: Arc<RwLock<ComponentPool>>) -> Result<()> {
    info!("Simulated pipeline: {} tenants, {} RPS", TENANT_COUNT, TARGET_RPS);

    let interval_ns = 1_000_000_000u64 / TARGET_RPS;
    let mut event_count = 0u64;
    let mut tag_counts = std::collections::HashMap::<String, u64>::new();
    let start = Instant::now();

    // Terminal progress bar
    println!("\n{}", "─".repeat(60));
    println!("  NexusCore Day 25 — Live Classification Pipeline");
    println!("{}", "─".repeat(60));

    loop {
        let t0 = Instant::now();

        // Simulate BPF ring buffer event (arrival timestamp)
        let arrival_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        // Round-robin tenants (Zipf in production — see homework)
        let tenant_id = format!("tenant-{:03}", event_count % TENANT_COUNT as u64);
        let payload = generate_event_payload(event_count, &tenant_id);

        let classification = {
            let mut p = pool.write();
            p.classify_dev(&tenant_id, payload.as_bytes(), arrival_ns)
        };

        *tag_counts.entry(classification.tag.to_string()).or_insert(0) += 1;
        metrics::record_tag(&classification.tag.to_string());
        event_count += 1;

        // Terminal visualizer — update every 5000 events
        if event_count % 5000 == 0 {
            let elapsed = start.elapsed().as_secs_f64();
            let actual_rps = event_count as f64 / elapsed;
            render_dashboard(event_count, actual_rps, &tag_counts, &classification);
        }

        // Throttle to target RPS
        let elapsed_ns = t0.elapsed().as_nanos() as u64;
        if elapsed_ns < interval_ns {
            tokio::time::sleep(Duration::from_nanos(interval_ns - elapsed_ns)).await;
        }
    }
}

fn generate_event_payload(seq: u64, tenant_id: &str) -> String {
    // Realistic distribution of event types
    match seq % 100 {
        0..=2  => format!(r#"{{"tenant":"{tenant_id}","event":"payment","amount":9500.0,"currency":"USD","seq":{seq}}}"#),
        3..=8  => format!(r#"{{"tenant":"{tenant_id}","event":"payment","amount":650.0,"currency":"USD","seq":{seq}}}"#),
        9..=14 => format!(r#"{{"tenant":"{tenant_id}","event":"session_end","churn_score":0.82,"seq":{seq}}}"#),
        15..=20 => {
            // Anomalous: inject null bytes via explicit nul encoding placeholder
            format!(r#"{{"tenant":"{tenant_id}","event":"corrupt","seq":{seq},"data":"\u0000\u0000\u0000"}}"#)
        }
        _ => format!(r#"{{"tenant":"{tenant_id}","event":"pageview","path":"/dashboard","seq":{seq}}}"#),
    }
}

fn render_dashboard(
    count: u64,
    rps: f64,
    tags: &std::collections::HashMap<String, u64>,
    last: &crate::classifier::Classification,
) {
    // Move cursor up to overwrite previous output
    print!("\x1b[8A");

    let bar_width = 30usize;
    let total = tags.values().sum::<u64>().max(1) as f64;

    println!("  Events processed : {:>10}", fmt_num(count));
    println!("  Throughput       : {:>10.0} RPS", rps);
    println!("  Last tag         : {:>10} ({:.2}%)", last.tag, last.confidence * 100.0);
    println!("  Last latency     : {:>10} µs", last.latency_ns / 1000);
    println!();

    let tag_order = ["pass", "high-value", "churn-risk", "fraud-signal", "anomaly"];
    let colors = ["\x1b[32m", "\x1b[34m", "\x1b[33m", "\x1b[31m", "\x1b[35m"];

    for (tag, color) in tag_order.iter().zip(colors.iter()) {
        let n = *tags.get(*tag).unwrap_or(&0);
        let frac = n as f64 / total;
        let filled = (frac * bar_width as f64) as usize;
        let bar: String = "█".repeat(filled) + &"░".repeat(bar_width - filled);
        println!("  {color}{tag:<14}\x1b[0m │{bar}│ {:.1}%", frac * 100.0);
    }
    println!("{}", "─".repeat(60));
    std::io::Write::flush(&mut std::io::stdout()).ok();
}

fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { result.push(','); }
        result.push(c);
    }
    result.chars().rev().collect()
}
