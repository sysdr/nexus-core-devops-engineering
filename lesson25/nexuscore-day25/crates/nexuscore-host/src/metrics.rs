//! Prometheus metrics for the NexusCore host.
use anyhow::Result;
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;

pub fn install_recorder() -> Result<()> {
    PrometheusBuilder::new()
        .with_http_listener(([0, 0, 0, 0], 9090))
        .install()?;
    tracing::info!("Prometheus metrics at http://localhost:9090/metrics");
    Ok(())
}

pub fn record_classify_latency(tenant_id: &str, latency_ns: u64) {
    histogram!("nexuscore_classify_latency_ns",
        "tenant" => tenant_id.to_string()
    ).record(latency_ns as f64);
}

pub fn record_tag(tag: &str) {
    counter!("nexuscore_tags_total", "tag" => tag.to_string()).increment(1);
}

pub fn record_bpf_drop() {
    counter!("nexuscore_bpf_ringbuf_drops_total").increment(1);
}

pub fn set_wasm_instances(n: usize) {
    gauge!("nexuscore_wasm_instances").set(n as f64);
}
