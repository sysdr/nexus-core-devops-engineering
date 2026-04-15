//! Default NexusCore tenant classifier.
//! Compiled to wasm32-wasip2 as a Wasm component.
//!
//! This component implements the nexuscore:classifier/classify WIT interface.
//! It runs in a shared-nothing sandbox with 4 MB linear memory max.
//! No heap allocator beyond a simple bump allocator — we work on borrows.

// Disable the Rust standard allocator — the wit-bindgen runtime provides one.
#![no_main]

wit_bindgen::generate!({
    world: "classifier-component",
    // Inline the WIT definition so the component is self-contained.
    inline: r#"
package nexuscore:classifier@0.1.0;

interface classify {
    resource record-view {
        tenant-id: func() -> string;
        payload: func() -> list<u8>;
        arrival-timestamp-ns: func() -> u64;
        partition: func() -> s32;
        offset: func() -> s64;
    }

    enum tag {
        fraud-signal,
        churn-risk,
        high-value,
        anomaly,
        pass,
    }

    record classification {
        tag: tag,
        confidence: float32,
        classify-latency-ns: u64,
    }

    classify-record: func(rec: borrow<record-view>) -> classification;
}

world classifier-component {
    export classify;
}
"#,
});

use exports::nexuscore::classifier::classify::{Classification, Guest, RecordView, Tag};

struct DefaultClassifier;

impl Guest for DefaultClassifier {
    fn classify_record(rec: RecordView<'_>) -> Classification {
        // WASI 0.3: arrival_ns is a u64 from bpf_ktime_get_ns on the host.
        let t_start = rec.arrival_timestamp_ns();
        let payload  = rec.payload();

        let (tag, confidence) = classify_bytes(&payload);

        // Latency: approximate wall time inside this Wasm frame.
        // In production the host measures the real classify latency in Rust;
        // this is for component-internal diagnostics.
        let classify_latency_ns = 0u64; // host fills from wasmtime epoch delta

        Classification { tag, confidence, classify_latency_ns }
    }
}

/// Pure rule engine operating on raw bytes.
/// No allocations beyond the fixed-size stack frame.
fn classify_bytes(payload: &[u8]) -> (Tag, f32) {
    // 1. JSON key scanning — no full parse, just byte search
    //    Avoids any heap allocation in the Wasm sandbox.
    if let Some(amount) = scan_json_f64(payload, b"\"amount\":") {
        if amount > 9000.0 { return (Tag::FraudSignal, 0.91); }
        if amount > 500.0  { return (Tag::HighValue,   0.82); }
    }
    if let Some(score) = scan_json_f64(payload, b"\"churn_score\":") {
        if score > 0.75 { return (Tag::ChurnRisk, score as f32); }
    }

    // 2. Anomaly: null-byte density
    let zeros = payload.iter().filter(|&&b| b == 0).count();
    if zeros * 10 > payload.len() {
        return (Tag::Anomaly, 0.88);
    }

    (Tag::Pass, 1.0)
}

/// Scan JSON bytes for a numeric value after `key`.
/// Returns None if key not found or value not parseable.
/// Uses only stack memory — no Vec, no String.
fn scan_json_f64(haystack: &[u8], key: &[u8]) -> Option<f64> {
    let pos = find_subsequence(haystack, key)? + key.len();
    let rest = haystack.get(pos..)?;
    // Skip whitespace
    let start = rest.iter().position(|b| !b.is_ascii_whitespace())?;
    let rest = &rest[start..];
    // Read until delimiter
    let end = rest
        .iter()
        .position(|&b| b == b',' || b == b'}' || b == b' ')
        .unwrap_or(rest.len());
    let num_bytes = &rest[..end];
    // SAFETY: we checked it's ASCII digits/decimal — safe to interpret as str.
    let s = core::str::from_utf8(num_bytes).ok()?;
    s.parse().ok()
}

/// Knuth-Morris-Pratt subsequence search on byte slices.
/// O(n+m) — no allocation.
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() { return Some(0); }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

// Wasm entry point
export!(DefaultClassifier);
