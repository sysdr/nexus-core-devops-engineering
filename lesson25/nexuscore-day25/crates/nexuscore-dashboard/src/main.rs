//! Minimal terminal dashboard — polls the Prometheus /metrics endpoint
//! exposed by nexuscore-host and renders classification stats.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

static FIRST_RENDER: AtomicBool = AtomicBool::new(true);

#[tokio::main]
async fn main() -> Result<()> {
    println!("\x1b[2J\x1b[H"); // clear screen
    println!("  NexusCore Day 25 — Classification Dashboard");
    println!("  Polling http://localhost:9090/metrics every 2s");
    println!("  Press Ctrl+C to exit\n");

    loop {
        match fetch_metrics().await {
            Ok(text) => render(&text),
            Err(e) => eprintln!("  [metrics unavailable] {e}"),
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}

async fn fetch_metrics() -> Result<String> {
    // Use std::process to call curl — avoids adding reqwest dependency.
    let output = tokio::process::Command::new("curl")
        .args(["-s", "--max-time", "1", "http://localhost:9090/metrics"])
        .output()
        .await?;
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn render(metrics: &str) {
    // First render: print enough lines so cursor-up rewrites work in a normal terminal.
    if FIRST_RENDER.swap(false, Ordering::Relaxed) {
        for _ in 0..6 {
            println!();
        }
    } else {
        print!("\x1b[6A"); // move up past previous render
    }

    let tag_names = ["pass", "fraud-signal", "high-value", "churn-risk", "anomaly"];
    let colors    = ["\x1b[32m", "\x1b[31m", "\x1b[34m", "\x1b[33m", "\x1b[35m"];

    for (tag, color) in tag_names.iter().zip(colors.iter()) {
        let count = extract_counter(metrics, "nexuscore_tags_total", "tag", tag);
        println!("  {color}{tag:<16}\x1b[0m {:>10}", fmt_num(count));
    }
    println!();
    std::io::Write::flush(&mut std::io::stdout()).ok();
}

fn extract_counter(metrics: &str, name: &str, label: &str, value: &str) -> u64 {
    // Prometheus text format: metric_name{label="value",...} 123
    // Label order is not guaranteed, so we match by substrings.
    let prefix = format!("{name}{{");
    let label_pair = format!("{label}=\"{value}\"");
    for line in metrics.lines() {
        if !line.starts_with(&prefix) {
            continue;
        }
        if !line.contains(&label_pair) {
            continue;
        }
        if let Some(v) = line.split_whitespace().last() {
            return v.parse().unwrap_or(0);
        }
    }
    0
}

fn fmt_num(n: u64) -> String {
    if n == 0 { return "—".to_string(); }
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 { result.push(','); }
        result.push(c);
    }
    result.chars().rev().collect()
}
