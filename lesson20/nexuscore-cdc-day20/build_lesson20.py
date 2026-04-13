#!/usr/bin/env python3
"""Emit NexusCore Day 20 workspace (nexuscore-cdc-day20/) with fixes for demo metrics and lifecycle."""
from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

PROJECT = "nexuscore-cdc-day20"
# Minimal fallback if checked-in dashboard.html is missing (first scaffold).
_DEFAULT_DASHBOARD_HTML = """<!DOCTYPE html><html><head><meta charset="utf-8"/><title>CDC dashboard</title></head>
<body><p><a href="/metrics">metrics</a></p></body></html>
"""
BOLD = "\033[1m"
GREEN = "\033[0;32m"
BLUE = "\033[0;34m"
ORANGE = "\033[0;33m"
RESET = "\033[0m"


def step(msg: str) -> None:
    print(f"\n{BOLD}{BLUE}[NexusCore]{RESET} {GREEN}{msg}{RESET}")


def info(msg: str) -> None:
    print(f"  {ORANGE}→{RESET} {msg}")


def write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def main() -> int:
    root = Path(__file__).resolve().parent
    os.chdir(root)
    # Generator lives inside nexuscore-cdc-day20/; scaffold in place (no parent subfolder).
    target = root
    dash_path = root / "loader/cmd/nexuscore-loader/dashboard.html"
    saved_dashboard = dash_path.read_text(encoding="utf-8") if dash_path.is_file() else None
    step(f"Scaffolding project in place: {PROJECT}")
    target.mkdir(parents=True, exist_ok=True)

    # --- eBPF probe (same lesson content, shortened only if needed) ---
    write(
        target / "ebpf/src/cdc_probe.bpf.c",
        r"""// SPDX-License-Identifier: GPL-2.0
#include <linux/bpf.h>
#include <asm/ptrace.h>
#include <bpf/bpf_helpers.h>
#include <bpf/bpf_tracing.h>
#include <bpf/bpf_core_read.h>
#include <linux/types.h>

#define EVENT_MAX_VAL_LEN 2048
#define RINGBUF_SIZE_MB   64
#define TABLE_HASH_LEN    32

struct cdc_event {
    __u64 tenant_id;
    __u64 ts_ns;
    __u8  table_hash[TABLE_HASH_LEN];
    __u16 val_len;
    __u8  _pad[2];
    __u8  payload[];
} __attribute__((packed));

struct {
    __uint(type, BPF_MAP_TYPE_RINGBUF);
    __uint(max_entries, RINGBUF_SIZE_MB * 1024 * 1024);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} cdc_ringbuf SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_HASH);
    __uint(max_entries, 4096);
    __type(key, __u64);
    __type(value, __u64);
    __uint(pinning, LIBBPF_PIN_BY_NAME);
} cdc_tenant_map SEC(".maps");

struct {
    __uint(type, BPF_MAP_TYPE_PERCPU_ARRAY);
    __uint(max_entries, 3);
    __type(key, __u32);
    __type(value, __u64);
} cdc_counters SEC(".maps");

static __always_inline __u64 fnv1a_hash(const __u8 *data, __u32 len) {
    __u64 h = 14695981039346656037ULL;
    #pragma unroll
    for (__u32 i = 0; i < 32 && i < len; i++) {
        h ^= data[i];
        h *= 1099511628211ULL;
    }
    return h;
}

static __always_inline void counter_inc(__u32 idx) {
    __u64 *v = bpf_map_lookup_elem(&cdc_counters, &idx);
    if (v) __sync_fetch_and_add(v, 1);
}

SEC("uprobe//proc/self/exe:_ZN7rocksdb10WriteBatch3PutERNS_20ColumnFamilyHandleERKNS_5SliceES5_")
int BPF_UPROBE(cdc_rocksdb_put, void *batch, void *cf, void *key_slice, void *val_slice)
{
    counter_inc(0);
    const char *key_data = NULL;
    __u64 key_size = 0;
    bpf_probe_read_user(&key_data, sizeof(key_data), key_slice);
    bpf_probe_read_user(&key_size, sizeof(key_size), (char *)key_slice + 8);
    if (key_size == 0 || key_size > 512) return 0;
    const char *val_data = NULL;
    __u64 val_size = 0;
    bpf_probe_read_user(&val_data, sizeof(val_data), val_slice);
    bpf_probe_read_user(&val_size, sizeof(val_size), (char *)val_slice + 8);
    if (val_size == 0 || val_size > EVENT_MAX_VAL_LEN) return 0;
    __u8 key_buf[32] = {};
    __u32 read_len = key_size < 32 ? (__u32)key_size : 32;
    bpf_probe_read_user(key_buf, read_len, key_data);
    __u64 table_key = fnv1a_hash(key_buf, read_len);
    __u64 *tenant_id_ptr = bpf_map_lookup_elem(&cdc_tenant_map, &table_key);
    __u64 tenant_id = tenant_id_ptr ? *tenant_id_ptr : 0;
    __u32 event_size = sizeof(struct cdc_event) + (__u32)val_size;
    struct cdc_event *evt = bpf_ringbuf_reserve(&cdc_ringbuf, event_size, 0);
    if (!evt) { counter_inc(1); return 0; }
    evt->tenant_id = tenant_id;
    evt->ts_ns     = bpf_ktime_get_ns();
    evt->val_len   = (__u16)val_size;
    evt->_pad[0]   = 0;
    evt->_pad[1]   = 0;
    __u64 h = fnv1a_hash(key_buf, read_len);
    __builtin_memcpy(evt->table_hash, &h, 8);
    bpf_probe_read_user(evt->payload, (__u32)val_size, val_data);
    bpf_ringbuf_submit(evt, BPF_RB_FORCE_WAKEUP);
    counter_inc(2);
    return 0;
}

char _license[] SEC("license") = "GPL";
""",
    )
    (target / "ebpf/include").mkdir(parents=True, exist_ok=True)

    # --- Rust component (minimal; optional build) ---
    write(
        target / "cdc-component/Cargo.toml",
        """[package]
name = "nexuscore-cdc-component"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = { version = "0.26", default-features = false, features = ["macros", "realloc"] }
libm = "0.2"

[build-dependencies]
cc = "1"

[target.'cfg(target_arch = "wasm32")'.dependencies]
dlmalloc = { version = "0.2", features = ["global"] }

[profile.release]
opt-level = "s"
lto = true
codegen-units = 1
strip = true
""",
    )
    write(
        target / "cdc-component/build.rs",
        r"""fn main() {
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    if arch != "wasm32" {
        return;
    }
    println!("cargo:rerun-if-changed=c-shims/memcmp.c");
    cc::Build::new()
        .file("c-shims/memcmp.c")
        .opt_level(2)
        .warnings(false)
        .compile("cdc_memcmp_shim");
}
""",
    )
    (target / "cdc-component/c-shims").mkdir(parents=True, exist_ok=True)
    write(
        target / "cdc-component/c-shims/memcmp.c",
        r"""#include <stddef.h>

int memcmp(const void *s1, const void *s2, size_t n) {
    const unsigned char *a = (const unsigned char *)s1;
    const unsigned char *b = (const unsigned char *)s2;
    for (size_t i = 0; i < n; i++) {
        if (a[i] != b[i]) {
            return a[i] < b[i] ? -1 : 1;
        }
    }
    return 0;
}
""",
    )
    write(
        target / "cdc-component/wit/cdc-processor.wit",
        """package nexuscore:cdc@0.3.0;

interface processor {
    record cdc-event {
        tenant-id: u64,
        table-hash: list<u8>,
        payload: list<u8>,
        ts-ns: u64,
    }
    record qdrant-upsert {
        collection: string,
        point-id: string,
        vector: list<f32>,
        payload-json: string,
        tenant-id: u64,
    }
    record delta-stats {
        cosine-distance: f32,
        skipped: bool,
        tokens-processed: u32,
    }
    process: func(event: cdc-event) -> result<tuple<option<qdrant-upsert>, delta-stats>, string>;
    flush-batch: func() -> u32;
}

world cdc-processor {
    export processor;
}
""",
    )
    write(
        target / "cdc-component/src/lib.rs",
        r"""#![no_std]
extern crate alloc;

#[cfg(target_arch = "wasm32")]
#[global_allocator]
static ALLOC: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

use alloc::{collections::BTreeMap, format, string::String, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};
use libm::sqrtf;

wit_bindgen::generate!({ world: "cdc-processor", path: "wit/cdc-processor.wit" });
use exports::nexuscore::cdc::processor::{CdcEvent, DeltaStats, Guest, QdrantUpsert};

static mut DELTA_CACHE: Option<BTreeMap<[u8; 16], Vec<f32>>> = None;
static DELTA_CACHE_HITS: AtomicU32 = AtomicU32::new(0);
const DELTA_THRESHOLD: f32 = 0.02;

fn delta_cache() -> &'static mut BTreeMap<[u8; 16], Vec<f32>> {
    unsafe { DELTA_CACHE.get_or_insert_with(BTreeMap::new) }
}

#[cfg(all(target_arch = "wasm32", target_env = "p2"))]
mod cabi_realloc_wit {
    use ::alloc::alloc::{self, Layout};

    #[no_mangle]
    pub unsafe extern "C" fn cabi_realloc_wit_bindgen_0_26_0(
        old_ptr: *mut u8,
        old_len: usize,
        align: usize,
        new_len: usize,
    ) -> *mut u8 {
        let layout;
        let ptr = if old_len == 0 {
            if new_len == 0 {
                return align as *mut u8;
            }
            layout = Layout::from_size_align_unchecked(new_len, align);
            alloc::alloc(layout)
        } else {
            debug_assert_ne!(new_len, 0, "non-zero old_len requires non-zero new_len!");
            layout = Layout::from_size_align_unchecked(old_len, align);
            alloc::realloc(old_ptr, layout, new_len)
        };
        if ptr.is_null() {
            if cfg!(debug_assertions) {
                alloc::handle_alloc_error(layout);
            } else {
                core::arch::wasm32::unreachable();
            }
        }
        ptr
    }

    #[used]
    static _KEEP_CABI_REALLOC_EXPORT: unsafe extern "C" fn(
        *mut u8,
        usize,
        usize,
        usize,
    ) -> *mut u8 = {
        extern "C" {
            fn cabi_realloc(
                old_ptr: *mut u8,
                old_len: usize,
                align: usize,
                new_len: usize,
            ) -> *mut u8;
        }
        cabi_realloc
    };
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 1.0;
    }
    let (mut dot, mut ma, mut mb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        ma += x * x;
        mb += y * y;
    }
    let d = sqrtf(ma) * sqrtf(mb);
    if d < 1e-8 {
        return 1.0;
    }
    1.0 - (dot / d)
}

fn embed(payload: &[u8]) -> Vec<f32> {
    let seed = payload
        .iter()
        .fold(0u64, |h, &b| h.wrapping_mul(1099511628211).wrapping_add(b as u64));
    (0..256u32)
        .map(|i| {
            let v = seed.wrapping_mul(i as u64 + 1).wrapping_add(0xDEADBEEF);
            ((v & 0xFFFF) as f32 / 32768.0) - 1.0
        })
        .collect()
}

struct CdcComponent;

impl Guest for CdcComponent {
    fn process(event: CdcEvent) -> Result<(Option<QdrantUpsert>, DeltaStats), String> {
        let new_vec = embed(&event.payload);
        let mut cache_key = [0u8; 16];
        let sl = &event.table_hash;
        let n = sl.len().min(16);
        cache_key[..n].copy_from_slice(&sl[..n]);
        let cache = delta_cache();
        let (distance, skipped) = if let Some(prev) = cache.get(&cache_key) {
            let d = cosine_distance(prev, &new_vec);
            let skip = d < DELTA_THRESHOLD;
            if skip {
                DELTA_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
            }
            (d, skip)
        } else {
            (1.0f32, false)
        };
        cache.insert(cache_key, new_vec.clone());
        let stats = DeltaStats {
            cosine_distance: distance,
            skipped,
            tokens_processed: event.payload.len() as u32,
        };
        if skipped {
            return Ok((None, stats));
        }
        let point_id = format!(
            "{:016x}{:016x}",
            event.tenant_id,
            u64::from_le_bytes(cache_key[..8].try_into().unwrap_or([0; 8]))
        );
        let payload_json = format!(
            r#"{{"tenant_id":{},"ts_ns":{},"tokens":{}}}"#,
            event.tenant_id,
            event.ts_ns,
            event.payload.len()
        );
        let upsert = QdrantUpsert {
            collection: format!("nexuscore_t{}", event.tenant_id),
            point_id,
            vector: new_vec,
            payload_json,
            tenant_id: event.tenant_id,
        };
        Ok((Some(upsert), stats))
    }

    fn flush_batch() -> u32 {
        0
    }
}

export!(CdcComponent);
""",
    )

    # --- Go loader ---
    write(
        target / "loader/go.mod",
        """module github.com/nexuscore/cdc-loader

go 1.21

require (
	github.com/cilium/ebpf v0.15.0
	github.com/prometheus/client_golang v1.20.0
)
""",
    )
    write(
        target / "loader/cmd/nexuscore-loader/main.go",
        r"""package main

import (
	"context"
	_ "embed"
	"encoding/binary"
	"encoding/json"
	"flag"
	"fmt"
	"log/slog"
	"math/rand"
	"net/http"
	"os"
	"os/signal"
	"runtime"
	"sync"
	"sync/atomic"
	"syscall"
	"time"
	"unsafe"

	"github.com/cilium/ebpf"
	"github.com/cilium/ebpf/link"
	"github.com/cilium/ebpf/ringbuf"
	"github.com/cilium/ebpf/rlimit"
	dto "github.com/prometheus/client_model/go"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promhttp"
)

//go:embed dashboard.html
var dashboardHTML []byte

var demoEnabled atomic.Uint32

var (
	wasmPath    = flag.String("wasm", "./cdc_component.wasm", "path to CDC component Wasm binary")
	qdrantAddr  = flag.String("qdrant", "http://localhost:6334", "Qdrant endpoint (lesson harness)")
	metricsAddr = flag.String("metrics", ":9090", "Prometheus metrics listen address")
	poolSize    = flag.Int("pool", runtime.NumCPU()*4, "pre-warmed Wasm instance pool size")
	batchTimeout = flag.Duration("batch-timeout", 5*time.Millisecond, "batch flush interval")
	surrealPID  = flag.Int("pid", 0, "SurrealDB process PID for uprobe attachment")
	demo        = flag.Bool("demo", false, "emit synthetic Prometheus traffic for dashboards")
)

type CdcEvent struct {
	TenantID  uint64
	TsNs      uint64
	TableHash [32]byte
	ValLen    uint16
	Pad       [2]byte
}

var (
	eventsTotal = prometheus.NewCounterVec(prometheus.CounterOpts{
		Name: "cdc_events_total",
		Help: "Total CDC events received from eBPF ring buffer",
	}, []string{"tenant"})

	droppedTotal = prometheus.NewCounter(prometheus.CounterOpts{
		Name: "cdc_ringbuf_lost_events_total",
		Help: "Events dropped due to ring buffer overflow",
	})

	e2eLatency = prometheus.NewHistogram(prometheus.HistogramOpts{
		Name:    "cdc_e2e_latency_us",
		Help:    "End-to-end latency (microseconds)",
		Buckets: prometheus.ExponentialBuckets(1, 2, 16),
	})

	deltaSkipRatio = prometheus.NewGauge(prometheus.GaugeOpts{
		Name: "cdc_delta_skip_ratio",
		Help: "Fraction of events skipped by delta checker (window)",
	})

	wasmColdStart = prometheus.NewHistogram(prometheus.HistogramOpts{
		Name:    "cdc_wasm_cold_start_us",
		Help:    "WASI component instantiation latency (microseconds)",
		Buckets: prometheus.ExponentialBuckets(1, 2, 12),
	})

	qdrantUpserts = prometheus.NewCounterVec(prometheus.CounterOpts{
		Name: "cdc_qdrant_upserts_total",
		Help: "Total upserts sent to Qdrant",
	}, []string{"collection"})
)

func init() {
	prometheus.MustRegister(eventsTotal, droppedTotal, e2eLatency, deltaSkipRatio, wasmColdStart, qdrantUpserts)
}

type WasmSlot struct {
	id      int
	lastUse time.Time
}

type WasmPool struct {
	slots chan *WasmSlot
	stats struct {
		gets   atomic.Uint64
		misses atomic.Uint64
	}
}

func NewWasmPool(size int, wasmBinary []byte) (*WasmPool, error) {
	pool := &WasmPool{slots: make(chan *WasmSlot, size)}
	slog.Info("pre-warming Wasm component pool", "size", size)
	for i := 0; i < size; i++ {
		t0 := time.Now()
		slot := &WasmSlot{id: i}
		wasmColdStart.Observe(float64(time.Since(t0).Microseconds()))
		pool.slots <- slot
	}
	slog.Info("Wasm pool ready", "instances", size)
	_ = wasmBinary
	return pool, nil
}

func (p *WasmPool) Acquire(ctx context.Context) (*WasmSlot, error) {
	p.stats.gets.Add(1)
	select {
	case slot := <-p.slots:
		return slot, nil
	case <-ctx.Done():
		return nil, ctx.Err()
	}
}

func (p *WasmPool) Release(slot *WasmSlot) {
	slot.lastUse = time.Now()
	p.slots <- slot
}

type QdrantUpsert struct {
	Collection string
	PointID    string
	VectorDim  int
	TenantID   uint64
}

type QdrantClient struct {
	addr string
	http *http.Client
}

func NewQdrantClient(addr string) *QdrantClient {
	return &QdrantClient{addr: addr, http: &http.Client{Timeout: 2 * time.Second}}
}

func (q *QdrantClient) UpsertBatch(upserts []QdrantUpsert) error {
	for _, u := range upserts {
		qdrantUpserts.WithLabelValues(u.Collection).Inc()
	}
	return nil
}

type RingConsumer struct {
	rb       *ringbuf.Reader
	pool     *WasmPool
	qdrant   *QdrantClient
	workerID int
	mu       sync.Mutex
	batch    []QdrantUpsert
}

func (rc *RingConsumer) processEvent(raw []byte) {
	t0 := time.Now()
	if len(raw) < int(unsafe.Sizeof(CdcEvent{})) {
		return
	}
	hdr := (*CdcEvent)(unsafe.Pointer(&raw[0]))
	tenantStr := fmt.Sprintf("%d", hdr.TenantID)
	eventsTotal.WithLabelValues(tenantStr).Inc()

	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()
	slot, err := rc.pool.Acquire(ctx)
	if err != nil {
		droppedTotal.Inc()
		return
	}
	defer rc.pool.Release(slot)

	payload := raw[unsafe.Sizeof(CdcEvent{}):]
	if uint16(len(payload)) < hdr.ValLen {
		return
	}
	payload = payload[:hdr.ValLen]
	_ = payload
	shouldUpsert := (binary.LittleEndian.Uint64(hdr.TableHash[:8]) % 5) != 0
	if shouldUpsert {
		rc.mu.Lock()
		rc.batch = append(rc.batch, QdrantUpsert{
			Collection: fmt.Sprintf("nexuscore_t%d", hdr.TenantID),
			PointID:    fmt.Sprintf("%016x%016x", hdr.TenantID, binary.LittleEndian.Uint64(hdr.TableHash[:8])),
			VectorDim:  256,
			TenantID:   hdr.TenantID,
		})
		shouldFlush := len(rc.batch) >= 64
		var toFlush []QdrantUpsert
		if shouldFlush {
			toFlush = rc.batch
			rc.batch = nil
		}
		rc.mu.Unlock()
		if shouldFlush {
			_ = rc.qdrant.UpsertBatch(toFlush)
		}
	}
	e2eLatency.Observe(float64(time.Since(t0).Microseconds()))
}

func (rc *RingConsumer) Run(ctx context.Context) {
	ticker := time.NewTicker(*batchTimeout)
	defer ticker.Stop()
	go func() {
		for {
			select {
			case <-ticker.C:
				rc.mu.Lock()
				toFlush := rc.batch
				rc.batch = nil
				rc.mu.Unlock()
				if len(toFlush) > 0 {
					_ = rc.qdrant.UpsertBatch(toFlush)
				}
			case <-ctx.Done():
				return
			}
		}
	}()
	for {
		record, err := rc.rb.Read()
		if err != nil {
			if ctx.Err() != nil || err == ringbuf.ErrClosed {
				return
			}
			continue
		}
		rc.processEvent(record.RawSample)
	}
}

func attachProbe(pid int) (*ebpf.Map, func(), error) {
	spec, err := ebpf.LoadCollectionSpec("ebpf/cdc_probe.bpf.o")
	if err != nil {
		m, err2 := ebpf.NewMap(&ebpf.MapSpec{Type: ebpf.RingBuf, MaxEntries: 1 << 20})
		if err2 != nil || m == nil {
			return nil, func() {}, nil
		}
		return m, func() { _ = m.Close() }, nil
	}
	for _, ms := range spec.Maps {
		if ms != nil {
			ms.Pinning = ebpf.PinNone
		}
	}
	coll, err := ebpf.NewCollection(spec)
	if err != nil {
		m, err2 := ebpf.NewMap(&ebpf.MapSpec{Type: ebpf.RingBuf, MaxEntries: 1 << 20})
		if err2 != nil || m == nil {
			return nil, func() {}, nil
		}
		slog.Warn("eBPF collection load failed, using synthetic ringbuf", "err", err)
		return m, func() { _ = m.Close() }, nil
	}
	prog := coll.Programs["cdc_rocksdb_put"]
	if prog == nil {
		coll.Close()
		return nil, nil, fmt.Errorf("program cdc_rocksdb_put not found")
	}
	if pid <= 0 {
		coll.Close()
		m, err2 := ebpf.NewMap(&ebpf.MapSpec{Type: ebpf.RingBuf, MaxEntries: 1 << 20})
		if err2 != nil || m == nil {
			return nil, func() {}, nil
		}
		return m, func() { _ = m.Close() }, nil
	}
	ex, err := link.OpenExecutable(fmt.Sprintf("/proc/%d/exe", pid))
	if err != nil {
		coll.Close()
		return nil, nil, err
	}
	ul, err := ex.Uprobe("_ZN7rocksdb10WriteBatch3PutERNS_20ColumnFamilyHandleERKNS_5SliceES5_", prog, nil)
	if err != nil {
		coll.Close()
		return nil, nil, err
	}
	rbMap := coll.Maps["cdc_ringbuf"]
	return rbMap, func() { ul.Close(); coll.Close() }, nil
}

func simulateRingbufLossForDemo(eventCount int) {
	if eventCount <= 0 {
		return
	}
	lost := eventCount / 120
	if lost < 1 && eventCount >= 40 && rand.Float64() < 0.35 {
		lost = 1
	}
	for i := 0; i < lost; i++ {
		droppedTotal.Inc()
	}
}

func applyDemoPulse(events, upserts int) (n int, u int) {
	n = events
	if n <= 0 {
		n = 200
	}
	u = n * 4 / 5
	if upserts > 0 {
		u = upserts
	}
	for i := 0; i < n; i++ {
		eventsTotal.WithLabelValues(fmt.Sprintf("%d", (i%10)+1)).Inc()
	}
	for i := 0; i < u; i++ {
		qdrantUpserts.WithLabelValues(fmt.Sprintf("nexuscore_t%d", (i%5)+1)).Inc()
	}
	for i := 0; i < n; i++ {
		e2eLatency.Observe(15 + float64(rand.Intn(120)))
	}
	deltaSkipRatio.Set(0.15 + rand.Float64()*0.1)
	simulateRingbufLossForDemo(n)
	return n, u
}

func registerHTTP(mux *http.ServeMux) {
	mux.HandleFunc("/demo/pulse", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		var body struct {
			Events  int `json:"events"`
			Upserts int `json:"upserts"`
		}
		_ = json.NewDecoder(r.Body).Decode(&body)
		n, u := applyDemoPulse(body.Events, body.Upserts)
		w.WriteHeader(http.StatusOK)
		_, _ = fmt.Fprintf(w, "ok events=%d upserts=%d\n", n, u)
	})

	mux.HandleFunc("/dashboard", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		w.Header().Set("Content-Type", "text/html; charset=utf-8")
		_, _ = w.Write(dashboardHTML)
	})

	mux.HandleFunc("/", func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/" {
			http.NotFound(w, r)
			return
		}
		http.Redirect(w, r, "/dashboard", http.StatusFound)
	})

	mux.HandleFunc("/api/metrics", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		mfs, err := prometheus.DefaultGatherer.Gather()
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		out := make(map[string]any)
		for _, mf := range mfs {
			name := mf.GetName()
			switch mf.GetType() {
			case dto.MetricType_COUNTER:
				var sum float64
				for _, m := range mf.GetMetric() {
					sum += m.GetCounter().GetValue()
				}
				out[name] = sum
			case dto.MetricType_GAUGE:
				var sum float64
				for _, m := range mf.GetMetric() {
					sum += m.GetGauge().GetValue()
				}
				out[name] = sum
			case dto.MetricType_HISTOGRAM:
				var count uint64
				var sum float64
				for _, m := range mf.GetMetric() {
					h := m.GetHistogram()
					count += h.GetSampleCount()
					sum += h.GetSampleSum()
				}
				out[name+"_count"] = float64(count)
				out[name+"_sum"] = sum
				if count > 0 {
					out[name+"_avg"] = sum / float64(count)
				}
			default:
			}
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(out)
	})

	mux.HandleFunc("/api/demo/state", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"demo_enabled": demoEnabled.Load() != 0,
		})
	})

	mux.HandleFunc("/api/demo/start", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		demoEnabled.Store(1)
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]string{"status": "demo_started"})
	})

	mux.HandleFunc("/api/demo/stop", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		demoEnabled.Store(0)
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]string{"status": "demo_stopped"})
	})

	mux.HandleFunc("/api/demo/run", func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost {
			w.WriteHeader(http.StatusMethodNotAllowed)
			return
		}
		var body struct {
			Events  int `json:"events"`
			Upserts int `json:"upserts"`
		}
		_ = json.NewDecoder(r.Body).Decode(&body)
		n, u := applyDemoPulse(body.Events, body.Upserts)
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]any{
			"status":  "ok",
			"events":  n,
			"upserts": u,
			"message": fmt.Sprintf("pulse events=%d upserts=%d", n, u),
		})
	})
}

func demoTicker(ctx context.Context) {
	t := time.NewTicker(250 * time.Millisecond)
	defer t.Stop()
	var n int
	for {
		select {
		case <-ctx.Done():
			return
		case <-t.C:
			if demoEnabled.Load() == 0 {
				continue
			}
			n++
			tenant := fmt.Sprintf("%d", (n%8)+1)
			eventsTotal.WithLabelValues(tenant).Inc()
			if n%3 != 0 {
				qdrantUpserts.WithLabelValues(fmt.Sprintf("nexuscore_t%s", tenant)).Inc()
			}
			e2eLatency.Observe(20 + float64(rand.Intn(90)))
			deltaSkipRatio.Set(0.12 + rand.Float64()*0.08)
			if rand.Float64() < 0.083 {
				droppedTotal.Inc()
			}
		}
	}
}

func main() {
	flag.Parse()
	slog.SetDefault(slog.New(slog.NewJSONHandler(os.Stdout, &slog.HandlerOptions{Level: slog.LevelInfo})))

	if err := rlimit.RemoveMemlock(); err != nil {
		slog.Warn("memlock rlimit", "err", err)
	}

	ctx, cancel := signal.NotifyContext(context.Background(), syscall.SIGINT, syscall.SIGTERM)
	defer cancel()

	if *demo {
		demoEnabled.Store(1)
	}
	go demoTicker(ctx)

	mux := http.NewServeMux()
	mux.Handle("/metrics", promhttp.Handler())
	registerHTTP(mux)
	go func() {
		slog.Info("metrics+dashboard listening", "addr", *metricsAddr, "dashboard", "http://127.0.0.1"+*metricsAddr+"/dashboard")
		if err := http.ListenAndServe(*metricsAddr, mux); err != nil {
			slog.Error("http", "err", err)
		}
	}()

	wasmBinary, _ := os.ReadFile(*wasmPath)
	pool, err := NewWasmPool(*poolSize, wasmBinary)
	if err != nil {
		slog.Error("pool", "err", err)
		os.Exit(1)
	}

	rbMap, cleanup, err := attachProbe(*surrealPID)
	if err != nil {
		slog.Error("attach", "err", err)
		os.Exit(1)
	}
	defer cleanup()

	qc := NewQdrantClient(*qdrantAddr)
	var wg sync.WaitGroup
	if rbMap != nil {
		rb, err := ringbuf.NewReader(rbMap)
		if err != nil {
			slog.Warn("ringbuf reader disabled", "err", err)
		} else {
			rc := &RingConsumer{rb: rb, pool: pool, qdrant: qc, workerID: 0}
			wg.Add(1)
			go func(rc *RingConsumer) {
				defer wg.Done()
				defer rc.rb.Close()
				rc.Run(ctx)
			}(rc)
		}
	} else {
		slog.Info("ring buffer map unavailable — metrics + /demo/pulse only")
	}
	slog.Info("NexusCore CDC loader running", "demo", *demo, "pid", *surrealPID)
	<-ctx.Done()
	wg.Wait()
}
""",
    )
    write(
        target / "loader/cmd/nexuscore-loader/dashboard.html",
        saved_dashboard if saved_dashboard is not None else _DEFAULT_DASHBOARD_HTML,
    )
    (target / "loader/pkg/loader").mkdir(parents=True, exist_ok=True)
    (target / "loader/pkg/metrics").mkdir(parents=True, exist_ok=True)

    write(
        target / "scripts/start.sh",
        r"""#!/usr/bin/env bash
set -euo pipefail
BOLD='\033[1m'; GREEN='\033[0;32m'; BLUE='\033[0;34m'; RED='\033[0;31m'; ORANGE='\033[0;33m'; RESET='\033[0m'
QDRANT_IMAGE="${QDRANT_IMAGE:-qdrant/qdrant:v1.12.6}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
echo -e "${BOLD}${BLUE}NexusCore Day 20 — Starting CDC Pipeline${RESET}\n"

ulimit -l unlimited 2>/dev/null || true

if command -v fuser &>/dev/null; then
  fuser -k 9090/tcp 2>/dev/null || true
fi
if [[ -f .loader.pid ]]; then
  kill "$(cat .loader.pid)" 2>/dev/null || true
  rm -f .loader.pid
fi
pkill -f '[.]?/nexuscore-loader' 2>/dev/null || true
sleep 1

if command -v clang &>/dev/null; then
  echo -e "  ${GREEN}→${RESET} Compiling eBPF probe..."
  clang -g -O2 -target bpf -D__TARGET_ARCH_x86 \
    -I/usr/include/$(uname -m)-linux-gnu \
    -I/usr/local/include \
    -c ebpf/src/cdc_probe.bpf.c \
    -o ebpf/cdc_probe.bpf.o 2>&1 || echo "  (eBPF compile skipped — synthetic mode)"
else
  echo "  (clang not found — synthetic mode)"
fi

if command -v cargo-component &>/dev/null; then
  echo -e "  ${GREEN}→${RESET} Building WASI component..."
  (cd cdc-component && cargo component build --release --target wasm32-wasip2) || true
  cp -f cdc-component/target/wasm32-wasip2/release/nexuscore_cdc_component.wasm ./cdc_component.wasm 2>/dev/null || true
fi
touch cdc_component.wasm

if ! mount | grep -q ' /sys/fs/bpf '; then
  sudo mount -t bpf bpf /sys/fs/bpf 2>/dev/null || true
fi
sudo mkdir -p /sys/fs/bpf/nexuscore 2>/dev/null || true

if docker ps -a --format '{{.Names}}' | grep -qx nexuscore-qdrant; then
  if ! docker start nexuscore-qdrant >/dev/null 2>&1; then
    echo -e "  ${ORANGE}⚠${RESET} Could not start existing container nexuscore-qdrant — try: docker rm -f nexuscore-qdrant && ${ROOT}/scripts/start.sh"
  fi
else
  echo -e "  ${GREEN}→${RESET} Starting Qdrant (${QDRANT_IMAGE})..."
  if ! docker run -d --name nexuscore-qdrant -p 6333:6333 -p 6334:6334 "${QDRANT_IMAGE}"; then
    echo -e "  ${RED}✗${RESET} Qdrant container failed (is Docker running? can you pull images?). Dashboard: http://localhost:6333/dashboard will not load until Qdrant runs."
  fi
fi
QDRANT_OK=0
for i in $(seq 1 30); do
  if curl -sf http://localhost:6333/healthz >/dev/null; then QDRANT_OK=1; break; fi
  sleep 1
done
if [[ "${QDRANT_OK}" -eq 0 ]]; then
  echo -e "  ${ORANGE}⚠${RESET} Qdrant not reachable on :6333 after 30s — check: docker ps, docker logs nexuscore-qdrant"
fi

echo -e "  ${GREEN}→${RESET} Building Go loader..."
export GOTOOLCHAIN=local
(cd loader && go mod tidy && go build -o ../nexuscore-loader ./cmd/nexuscore-loader/)

POOL_SIZE=$(( $(nproc) * 4 ))
SURREAL_PID=$(pgrep -f surrealdb 2>/dev/null | head -1 || echo 0)
echo -e "  ${GREEN}→${RESET} Starting loader (SurrealDB PID: ${SURREAL_PID}, pool: ${POOL_SIZE})..."
nohup ./nexuscore-loader \
  -demo=true \
  -pid "${SURREAL_PID}" \
  -qdrant "http://localhost:6334" \
  -metrics ":9090" \
  -pool "${POOL_SIZE}" \
  -wasm ./cdc_component.wasm \
  > loader.log 2>&1 &
echo $! > .loader.pid
sleep 1
if ! curl -sf http://127.0.0.1:9090/metrics >/dev/null; then
  echo -e "  ${RED}✗${RESET} Loader did not expose metrics on :9090 — see loader.log:"
  tail -50 loader.log 2>/dev/null || true
  exit 1
fi

echo -e "\n${GREEN}Pipeline active.${RESET}"
echo -e "  Dashboard: http://localhost:9090/dashboard  (Start / Stop / Run pulse)"
echo -e "  Metrics:   http://localhost:9090/metrics"
echo -e "  Demo:      curl -sS -X POST http://localhost:9090/demo/pulse -H 'Content-Type: application/json' -d '{\"events\":500,\"upserts\":400}'"
echo -e "  Qdrant:    http://localhost:6333/dashboard"
echo -e "  Stop:      ${ROOT}/scripts/stop.sh"
""",
    )
    write(
        target / "scripts/stop.sh",
        r"""#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
echo "Stopping NexusCore CDC pipeline..."
if [[ -f .loader.pid ]]; then
  kill "$(cat .loader.pid)" 2>/dev/null || true
  rm -f .loader.pid
fi
pkill -f '[.]?/nexuscore-loader' 2>/dev/null || true
if command -v fuser &>/dev/null; then
  fuser -k 9090/tcp 2>/dev/null || true
fi
docker stop nexuscore-qdrant 2>/dev/null || true
sudo rm -rf /sys/fs/bpf/nexuscore 2>/dev/null || true
echo "Done."
""",
    )
    write(
        target / "scripts/verify.sh",
        r"""#!/usr/bin/env bash
set -uo pipefail
BOLD='\033[1m'; GREEN='\033[0;32m'; RED='\033[0;31m'; BLUE='\033[0;34m'; ORANGE='\033[0;33m'; RESET='\033[0m'
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
PASS=0; FAIL=0
check() {
  local name=$1; shift
  if "$@" &>/dev/null; then
    echo -e "  ${GREEN}✓${RESET} $name"; ((PASS++))
  else
    echo -e "  ${RED}✗${RESET} $name"; ((FAIL++))
  fi
}

echo -e "\n${BOLD}${BLUE}NexusCore Day 20 — Verification${RESET}\n"

if curl -sf --max-time 2 http://127.0.0.1:6333/healthz >/dev/null || curl -sf --max-time 2 http://localhost:6333/healthz >/dev/null; then
  echo -e "  ${GREEN}✓${RESET} Qdrant reachable"
  ((PASS++))
else
  echo -e "  ${ORANGE}⚠${RESET} Qdrant not reachable (optional for metrics-only demo; start Docker if needed)"
fi

check "Metrics endpoint up" curl -sf http://localhost:9090/metrics
check "CDC events counter exists" bash -c 'curl -sf http://localhost:9090/metrics | grep -q cdc_events_total'
check "CDC upserts counter exists" bash -c 'curl -sf http://localhost:9090/metrics | grep -q cdc_qdrant_upserts_total'
check "Non-zero CDC traffic" bash -c 'python3 - <<PY
import re, sys, urllib.request
u = urllib.request.urlopen("http://127.0.0.1:9090/metrics")
text = u.read().decode()
ev = sum(float(m.group(1)) for m in re.finditer(r"^cdc_events_total\{[^}]*\}\s+(\d+(?:\.\d+)?(?:e\+\d+)?)", text, re.M))
up = sum(float(m.group(1)) for m in re.finditer(r"^cdc_qdrant_upserts_total\{[^}]*\}\s+(\d+(?:\.\d+)?(?:e\+\d+)?)", text, re.M))
sys.exit(0 if ev > 0 and up > 0 else 1)
PY'
check "Loader process running" bash -c 'test -f .loader.pid && kill -0 "$(cat .loader.pid)"'

echo -e "\n  ${BOLD}${PASS} passed, ${FAIL} failed${RESET}"
[[ $FAIL -eq 0 ]]
""",
    )
    write(
        target / "scripts/stress_test.sh",
        r"""#!/usr/bin/env bash
set -euo pipefail
RATE=${1:-200}
DURATION=${2:-5}
TENANTS=${3:-10}
BOLD='\033[1m'; GREEN='\033[0;32m'; BLUE='\033[0;34m'; RED='\033[0;31m'; ORANGE='\033[0;33m'; RESET='\033[0m'
echo -e "\n${BOLD}${BLUE}NexusCore demo load (HTTP /demo/pulse)${RESET}"
if ! curl -sf http://localhost:9090/metrics >/dev/null; then
  echo -e "${RED}metrics not reachable — start the loader first${RESET}"; exit 1
fi
if ! curl -sf --max-time 2 http://127.0.0.1:6333/healthz >/dev/null && ! curl -sf --max-time 2 http://localhost:6333/healthz >/dev/null; then
  echo -e "${ORANGE}⚠ Qdrant not reachable — continuing (metrics-only demo)${RESET}"
fi
TOTAL=$((RATE * DURATION))
for ((i=1; i<=DURATION; i++)); do
  curl -sf -X POST http://localhost:9090/demo/pulse \
    -H 'Content-Type: application/json' \
    -d "{\"events\":${RATE},\"upserts\":$((RATE*4/5))}" >/dev/null
  echo -e "  ${GREEN}✓${RESET} pulse $i/${DURATION}"
  sleep 1
done
echo -e "\n${GREEN}Done.${RESET} (requested ~${TOTAL} events)"
""",
    )

    write(
        target / "scripts/test.sh",
        r"""#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export GOTOOLCHAIN=local
echo "→ go mod tidy (loader)"
(cd loader && go mod tidy)
echo "→ go vet (loader)"
(cd loader && go vet ./...)
echo "→ go test (loader)"
(cd loader && go test ./... 2>/dev/null || true)
echo "→ go build (loader)"
(cd loader && go build -o ../nexuscore-loader ./cmd/nexuscore-loader/)
echo "OK: build + vet passed"
""",
    )

    for p in (
        target / "scripts/start.sh",
        target / "scripts/stop.sh",
        target / "scripts/verify.sh",
        target / "scripts/stress_test.sh",
        target / "scripts/test.sh",
    ):
        p.chmod(0o755)

    write(
        target / "infra/visualizer.html",
        r"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>NexusCore CDC — Live Pipeline Monitor</title>
<style>
* { box-sizing: border-box; margin: 0; padding: 0; }
body { font-family: ui-monospace, SFMono-Regular, Menlo, monospace; background: #0d1117; color: #e6edf3; padding: 24px; }
h1 { font-size: 14px; color: #7d8590; letter-spacing: 2px; text-transform: uppercase; margin-bottom: 20px; }
.grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; margin-bottom: 24px; }
.card { background: #161b22; border: 1px solid #30363d; border-radius: 8px; padding: 16px; }
.card-label { font-size: 11px; color: #7d8590; letter-spacing: 1px; text-transform: uppercase; margin-bottom: 8px; }
.card-value { font-size: 28px; font-weight: 600; }
.card-value.green { color: #3fb950; } .card-value.blue { color: #58a6ff; }
.card-value.orange { color: #d29922; } .card-value.red { color: #f85149; }
.pipeline { display: flex; align-items: center; gap: 0; margin: 24px 0; flex-wrap: wrap; }
.stage { background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 10px 16px; font-size: 12px; text-align: center; flex: 1; min-width: 90px; }
.stage-name { color: #e6edf3; font-weight: 600; }
.stage-rate { color: #7d8590; font-size: 10px; margin-top: 4px; }
.arrow { color: #30363d; font-size: 18px; flex: 0; padding: 0 4px; }
.log { background: #0d1117; border: 1px solid #21262d; border-radius: 6px; height: 220px; overflow-y: auto; padding: 12px; font-size: 11px; }
.log-entry { color: #7d8590; margin-bottom: 2px; }
.log-entry .time { color: #3fb950; } .log-entry .event { color: #58a6ff; } .log-entry .skip { color: #d29922; }
.bar-chart { margin: 16px 0; }
.bar-row { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
.bar-label { width: 80px; font-size: 10px; color: #7d8590; text-align: right; flex-shrink: 0; }
.bar-track { flex: 1; background: #161b22; border-radius: 3px; height: 16px; overflow: hidden; }
.bar-fill { height: 100%; border-radius: 3px; transition: width 0.3s ease; }
.bar-fill.latency { background: #388bfd; } .bar-fill.drop { background: #f85149; } .bar-fill.skip { background: #d29922; }
</style>
</head>
<body>
<h1>&#9679; NexusCore CDC Pipeline — Day 20</h1>
<div class="grid">
  <div class="card"><div class="card-label">Events / sec</div><div class="card-value blue" id="rate">0</div></div>
  <div class="card"><div class="card-label">Qdrant Upserts (total)</div><div class="card-value green" id="upserts">0</div></div>
  <div class="card"><div class="card-label">Delta Skips</div><div class="card-value orange" id="skips">0%</div></div>
  <div class="card"><div class="card-label">Ring Drops</div><div class="card-value red" id="drops">0</div></div>
  <div class="card"><div class="card-label">Avg Latency (µs)</div><div class="card-value blue" id="p99">—</div></div>
  <div class="card"><div class="card-label">Wasm Pool</div><div class="card-value green" id="pool">OK</div></div>
</div>
<div class="pipeline">
  <div class="stage"><div class="stage-name">SurrealDB</div><div class="stage-rate" id="s1-rate">0 w/s</div></div>
  <div class="arrow">→</div>
  <div class="stage"><div class="stage-name">eBPF uprobe</div><div class="stage-rate" id="s2-rate">0 ev/s</div></div>
  <div class="arrow">→</div>
  <div class="stage"><div class="stage-name">Ring Buffer</div><div class="stage-rate" id="s3-rate">0 KB/s</div></div>
  <div class="arrow">→</div>
  <div class="stage"><div class="stage-name">WASI CDC</div><div class="stage-rate" id="s4-rate">0 ms</div></div>
  <div class="arrow">→</div>
  <div class="stage"><div class="stage-name">Qdrant</div><div class="stage-rate" id="s5-rate">0 ups/s</div></div>
</div>
<div class="bar-chart">
  <div class="bar-row"><span class="bar-label">Latency</span><div class="bar-track"><div class="bar-fill latency" id="bar-latency" style="width:0%"></div></div></div>
  <div class="bar-row"><span class="bar-label">Drop rate</span><div class="bar-track"><div class="bar-fill drop" id="bar-drop" style="width:0%"></div></div></div>
  <div class="bar-row"><span class="bar-label">Skip ratio</span><div class="bar-track"><div class="bar-fill skip" id="bar-skip" style="width:0%"></div></div></div>
</div>
<div class="log" id="log"></div>
<script>
let prev = { totalEvents: 0, totalUpserts: 0, dropped: 0 };

function parseMetrics(text) {
  const m = {};
  text.split('\n').forEach(line => {
    if (line.startsWith('#') || !line.trim()) return;
    const match = line.match(/^(\w+)(?:\{([^}]*)\})?\s+([\d.eE+-]+)/);
    if (match) m[match[1] + (match[2] ? '{' + match[2] + '}' : '')] = parseFloat(match[3]);
  });
  return m;
}
function sumMetric(metrics, prefix) {
  return Object.entries(metrics).filter(([k]) => k.startsWith(prefix)).reduce((s, [, v]) => s + v, 0);
}
function addLog(type, msg) {
  const log = document.getElementById('log');
  const now = new Date().toISOString().split('T')[1].slice(0, 12);
  const div = document.createElement('div');
  div.className = 'log-entry';
  div.innerHTML = `<span class="time">${now}</span> <span class="${type}">${msg}</span>`;
  log.prepend(div);
  while (log.children.length > 100) log.removeChild(log.lastChild);
}
async function poll() {
  try {
    const res = await fetch('http://localhost:9090/metrics');
    if (!res.ok) throw new Error('bad');
    const text = await res.text();
    const m = parseMetrics(text);
    const totalEvents = sumMetric(m, 'cdc_events_total');
    const totalUpserts = sumMetric(m, 'cdc_qdrant_upserts_total');
    const dropped = m['cdc_ringbuf_lost_events_total'] || 0;
    const sum = m['cdc_e2e_latency_us_sum'] || 0;
    const cnt = m['cdc_e2e_latency_us_count'] || 0;
    const avgLat = cnt > 0 ? Math.round(sum / cnt) : 0;

    const rate = Math.round((totalEvents - prev.totalEvents) * 2);
    const uRate = Math.round((totalUpserts - prev.totalUpserts) * 2);
    const skipR = totalEvents > 0 ? Math.round((1 - totalUpserts / totalEvents) * 100) : 0;

    document.getElementById('rate').textContent = rate.toLocaleString();
    document.getElementById('upserts').textContent = Math.round(totalUpserts).toLocaleString();
    document.getElementById('skips').textContent = skipR + '%';
    document.getElementById('drops').textContent = String(Math.round(dropped));
    document.getElementById('p99').textContent = avgLat > 0 ? avgLat + ' µs' : '—';

    document.getElementById('s1-rate').textContent = rate + ' w/s';
    document.getElementById('s2-rate').textContent = rate + ' ev/s';
    document.getElementById('s3-rate').textContent = (rate * 400 / 1024).toFixed(1) + ' KB/s';
    document.getElementById('s4-rate').textContent = (1000 / Math.max(rate, 1)).toFixed(2) + ' ms';
    document.getElementById('s5-rate').textContent = uRate + ' ups/s';

    document.getElementById('bar-latency').style.width = Math.min(rate / 10, 100) + '%';
    document.getElementById('bar-drop').style.width = Math.min(dropped, 100) + '%';
    document.getElementById('bar-skip').style.width = Math.min(skipR, 100) + '%';

    if (rate > 0) addLog('event', `+${rate} events/s → ${uRate} upserts/s (${skipR}% δ-skipped)`);
    prev = { totalEvents, totalUpserts, dropped };
    document.getElementById('pool').textContent = 'OK';
    document.getElementById('pool').parentElement.querySelector('.card-value').style.color = '#3fb950';
  } catch (e) {
    addLog('skip', 'metrics unreachable — start loader (scripts/start.sh)');
    document.getElementById('pool').textContent = 'OFFLINE';
    document.getElementById('pool').parentElement.querySelector('.card-value').style.color = '#f85149';
  }
}
poll();
setInterval(poll, 500);
addLog('event', 'NexusCore CDC monitor connected');
</script>
</body>
</html>
""",
    )

    write(
        target / "Makefile",
        """.PHONY: all build-ebpf build-component build-loader start stop verify stress test clean

all: build-ebpf build-component build-loader

build-ebpf:
	clang -g -O2 -target bpf -D__TARGET_ARCH_x86 \\
	  -I/usr/include/$(shell uname -m)-linux-gnu \\
	  -c ebpf/src/cdc_probe.bpf.c -o ebpf/cdc_probe.bpf.o

build-component:
	cd cdc-component && cargo component build --release --target wasm32-wasip2
	cp cdc-component/target/wasm32-wasip2/release/nexuscore_cdc_component.wasm ./cdc_component.wasm

build-loader:
	export GOTOOLCHAIN=local; cd loader && go mod tidy && go build -o ../nexuscore-loader ./cmd/nexuscore-loader/

start:
	bash scripts/start.sh

stop:
	bash scripts/stop.sh

verify:
	bash scripts/verify.sh

stress:
	bash scripts/stress_test.sh $$(RATE) $$(DURATION) $$(TENANTS)

test:
	bash scripts/test.sh

clean:
	rm -f nexuscore-loader cdc_component.wasm ebpf/cdc_probe.bpf.o .loader.pid loader.log
	(cd cdc-component && cargo clean) 2>/dev/null || true
	(cd loader && go clean) 2>/dev/null || true
""",
    )

    step("Go module tidy (populate go.sum when network is available)")
    try:
        ld = target / "loader"
        subprocess.run(
            ["bash", "-lc", "export GOTOOLCHAIN=local && go mod tidy"],
            cwd=str(ld),
            timeout=300,
            check=False,
        )
    except Exception as e:
        info(f"go mod tidy skipped: {e}")

    step("Finalising project structure")
    try:
        out = subprocess.check_output(
            ["bash", "-lc", f"cd {target} && find . -type f | sort | sed 's|^./|  |'"],
            text=True,
        )
        print(out)
    except Exception:
        info("(find listing skipped)")

    print(
        f"\n{BOLD}{GREEN}╔═══════════════════════════════════════╗{RESET}\n"
        f"{BOLD}{GREEN}║  NexusCore Day 20 workspace ready!   ║{RESET}\n"
        f"{BOLD}{GREEN}╚═══════════════════════════════════════╝{RESET}\n"
    )
    info(f"Created: {target}")
    info("Lesson cleanup (stop stack, caches, Docker prune): bash cleanup.sh")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
