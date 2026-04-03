//! NexusCore Day 5 — Terminal Dashboard (stdin JSON)

use dashboard::HistogramReport;
use std::io::{self, BufRead};

fn bar(count: u64, max: u64, width: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let filled = (count as f64 / max as f64 * width as f64) as usize;
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn render_report(r: &HistogramReport) {
    println!(
        "\n\x1b[1;34m━━━ {} / {} ━━━\x1b[0m  [{}]",
        r.stack.to_uppercase(),
        r.op,
        &r.ts[..r.ts.len().min(19)]
    );
    println!(
        "  p50: \x1b[32m{:>8}\x1b[0m  p99: \x1b[33m{:>8}\x1b[0m  p999: \x1b[31m{:>8}\x1b[0m  total: {}",
        dashboard::ns_to_human(r.p50_ns),
        dashboard::ns_to_human(r.p99_ns),
        dashboard::ns_to_human(r.p999_ns),
        r.total_count,
    );

    let mut sorted: Vec<(u32, u64)> = r
        .buckets
        .iter()
        .filter_map(|(k, &v)| k.parse::<u32>().ok().map(|slot| (slot, v)))
        .collect();
    sorted.sort_by_key(|(slot, _)| *slot);

    let max_count = sorted.iter().map(|(_, v)| *v).max().unwrap_or(1);
    let lo = sorted.iter().position(|(_, v)| *v > 0).unwrap_or(0);
    let hi = (lo + 20).min(sorted.len());

    println!("  Slot  Range            Count    Distribution");
    for (slot, count) in &sorted[lo..hi] {
        let start_ns = 1u64 << slot;
        let end_ns = start_ns << 1;
        println!(
            "  {:>3}  {:>8} – {:>8}  {:>8}  {}",
            slot,
            dashboard::ns_to_human(start_ns),
            dashboard::ns_to_human(end_ns),
            count,
            bar(*count, max_count, 30),
        );
    }
}

fn main() {
    eprintln!("NexusCore Dashboard — waiting for histogram JSON on stdin...");
    let stdin = io::stdin();
    let mut buf = String::new();
    for line in stdin.lock().lines().flatten() {
        let t = line.trim();
        if t.starts_with('{') && t.ends_with('}') {
            if let Ok(report) = serde_json::from_str::<HistogramReport>(t) {
                render_report(&report);
            }
            continue;
        }
        buf.push_str(&line);
        buf.push('\n');
        if t == "}" {
            let chunk = buf.trim();
            if chunk.starts_with('{') && chunk.ends_with('}') {
                if let Ok(report) = serde_json::from_str::<HistogramReport>(chunk) {
                    render_report(&report);
                }
                buf.clear();
            }
        }
    }
}
