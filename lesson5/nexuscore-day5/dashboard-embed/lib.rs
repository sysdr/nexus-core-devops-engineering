//! Shared models for terminal and web dashboards (NexusCore Day 5).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistogramReport {
    pub ts: String,
    pub stack: String,
    pub op: String,
    pub p50_ns: u64,
    pub p99_ns: u64,
    pub p999_ns: u64,
    pub total_count: u64,
    pub buckets: HashMap<String, u64>,
}

/// Client-side load-gen summary (`load-gen` final JSON), microseconds for latencies.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoadClientMetrics {
    pub elapsed_secs: f64,
    pub target_rps: u64,
    pub actual_rps: f64,
    pub total_requests: u64,
    pub errors: u64,
    pub p50_us: u64,
    pub p99_us: u64,
    pub p999_us: u64,
    pub max_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsPayload {
    pub updated_rfc3339: String,
    pub surrealdb_reachable: bool,
    pub histograms: Vec<HistogramReport>,
    pub load_client: Option<LoadClientMetrics>,
}

/// eBPF-style log₂ histogram samples: SurrealDB vs polyglot vfs_read + combined tcp_sendmsg.
pub fn lesson_default_histograms(now: &chrono::DateTime<chrono::Utc>) -> Vec<HistogramReport> {
    let mut b1 = HashMap::new();
    for (k, v) in [
        ("14", 120u64),
        ("15", 8200),
        ("16", 22000),
        ("17", 12000),
        ("18", 5000),
        ("19", 2000),
        ("20", 680),
    ] {
        b1.insert(k.to_string(), v);
    }
    let mut b2 = HashMap::new();
    for (k, v) in [
        ("15", 400u64),
        ("16", 6000),
        ("17", 18000),
        ("18", 9000),
        ("19", 6000),
        ("20", 2800),
    ] {
        b2.insert(k.to_string(), v);
    }
    let mut b3 = HashMap::new();
    for (k, v) in [
        ("13", 200u64),
        ("14", 5000),
        ("15", 12000),
        ("16", 8000),
        ("17", 3500),
        ("18", 2300),
    ] {
        b3.insert(k.to_string(), v);
    }
    vec![
        HistogramReport {
            ts: now.to_rfc3339(),
            stack: "surrealdb".into(),
            op: "vfs_read".into(),
            p50_ns: 18_432,
            p99_ns: 72_800,
            p999_ns: 215_000,
            total_count: 50_000,
            buckets: b1,
        },
        HistogramReport {
            ts: now.to_rfc3339(),
            stack: "polyglot".into(),
            op: "vfs_read".into(),
            p50_ns: 22_100,
            p99_ns: 91_000,
            p999_ns: 280_000,
            total_count: 42_000,
            buckets: b2,
        },
        HistogramReport {
            ts: now.to_rfc3339(),
            stack: "combined".into(),
            op: "tcp_sendmsg".into(),
            p50_ns: 9_500,
            p99_ns: 45_200,
            p999_ns: 132_000,
            total_count: 31_000,
            buckets: b3,
        },
    ]
}

pub fn ns_to_human(ns: u64) -> String {
    if ns < 1_000 {
        format!("{ns} ns")
    } else if ns < 1_000_000 {
        format!("{:.1} µs", ns as f64 / 1_000.0)
    } else {
        format!("{:.2} ms", ns as f64 / 1_000_000.0)
    }
}
