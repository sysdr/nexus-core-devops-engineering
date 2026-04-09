//! NexusCore Day 15 — Semantic Index WASI 0.3 Component
//!
//! HNSW graph lives entirely in Wasm linear memory (~44MB per tenant).
//! Embedding model lives in host memory as a wasi:nn Graph resource —
//! all N tenant instances share ONE model load.  Zero iTLB duplication.
#![no_std]
extern crate alloc;

use alloc::{format, string::String, vec, vec::Vec};
use core::cell::RefCell;
use instant_distance::{Builder, HnswMap, Point, Search};

wit_bindgen::generate!({
    world: "semantic-index",
    path: "../wit",
    exports: {
        "nexus:semantic/ingest": Component,
        "nexus:semantic/query":  Component,
        "nexus:semantic/stats":  Component,
    }
});

const DIM: usize = 384;
const REBUILD_BATCH: usize = 256;
const HNSW_M: usize = 16;
const HNSW_EF: usize = 200;

#[derive(Clone)]
struct EmbVec([i8; DIM]);

impl Point for EmbVec {
    fn distance(&self, other: &Self) -> f32 {
        // Cosine distance on pre-normalised int8 embeddings.
        // Upcast to i32 to avoid saturation on intermediate products.
        let dot: i32 = self
            .0
            .iter()
            .zip(other.0.iter())
            .map(|(&a, &b)| (a as i32) * (b as i32))
            .sum();
        let mag_a: i32 = self.0.iter().map(|&x| (x as i32) * (x as i32)).sum();
        let mag_b: i32 = other.0.iter().map(|&x| (x as i32) * (x as i32)).sum();
        if mag_a == 0 || mag_b == 0 {
            return 1.0;
        }
        let cos = (dot as f32) / ((mag_a as f32).sqrt() * (mag_b as f32).sqrt());
        1.0 - cos.clamp(-1.0, 1.0)
    }
}

struct IndexState {
    vectors: Vec<(u64, EmbVec)>,
    hnsw: Option<HnswMap<EmbVec, u64>>,
    pending: usize,
    next_id: u64,
}

impl IndexState {
    fn new() -> Self {
        Self {
            vectors: Vec::new(),
            hnsw: None,
            pending: 0,
            next_id: 0,
        }
    }

    fn insert(&mut self, emb: EmbVec) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.vectors.push((id, emb));
        self.pending += 1;
        if self.pending >= REBUILD_BATCH || self.hnsw.is_none() {
            self.rebuild();
        }
        id
    }

    fn rebuild(&mut self) {
        let pts: Vec<EmbVec> = self.vectors.iter().map(|(_, v)| v.clone()).collect();
        let vals: Vec<u64> = self.vectors.iter().map(|(id, _)| *id).collect();
        self.hnsw = Some(
            Builder::default()
                .ef_construction(HNSW_EF)
                .num_neighbors(HNSW_M)
                .build(pts, vals),
        );
        self.pending = 0;
    }

    fn query(&self, emb: &EmbVec, top_k: usize) -> Vec<(u64, f32)> {
        let Some(ref h) = self.hnsw else { return vec![]; };
        let mut s = Search::default();
        h.search(emb, &mut s)
            .take(top_k)
            .map(|item| (*item.value, 1.0 - item.distance))
            .collect()
    }

    fn memory_bytes(&self) -> u64 {
        let vec_b = self.vectors.len() * (core::mem::size_of::<EmbVec>() + 8);
        let graph_b = self.vectors.len() * HNSW_M * 4;
        (vec_b + graph_b) as u64
    }
}

thread_local! {
    static STATE: RefCell<IndexState> = RefCell::new(IndexState::new());
}

fn embed(text: &str) -> Result<EmbVec, String> {
    use wasi::nn::{graph, inference, tensor};
    let model =
        graph::load_by_name("all-minilm-l6-v2-int8").map_err(|e| format!("load_by_name: {:?}", e))?;
    let ctx = model
        .init_execution_context()
        .map_err(|e| format!("init_ctx: {:?}", e))?;
    let input = tensor::Tensor::new(&[1u32, text.len() as u32], tensor::TensorType::U8, text.as_bytes());
    ctx.set_input(0, input)
        .map_err(|e| format!("set_input: {:?}", e))?;
    ctx.compute().map_err(|e| format!("compute: {:?}", e))?;
    let output = ctx.get_output(0).map_err(|e| format!("get_output: {:?}", e))?;
    let bytes = output.data();
    if bytes.len() != DIM {
        return Err(format!("dim mismatch: {}", bytes.len()));
    }
    let mut arr = [0i8; DIM];
    for (i, &b) in bytes.iter().enumerate() {
        arr[i] = b as i8;
    }
    Ok(EmbVec(arr))
}

struct Component;

impl exports::nexus::semantic::ingest::Guest for Component {
    fn ingest(tweet_text: String) -> Result<u64, String> {
        let emb = embed(&tweet_text)?;
        Ok(STATE.with(|s| s.borrow_mut().insert(emb)))
    }
}

impl exports::nexus::semantic::query::Guest for Component {
    fn query(query_text: String, top_k: u32) -> Result<Vec<(u64, f32)>, String> {
        let emb = embed(&query_text)?;
        Ok(STATE.with(|s| s.borrow().query(&emb, top_k as usize)))
    }
}

impl exports::nexus::semantic::stats::Guest for Component {
    fn stats() -> exports::nexus::semantic::stats::IndexStats {
        STATE.with(|s| {
            let st = s.borrow();
            exports::nexus::semantic::stats::IndexStats {
                total_vectors: st.vectors.len() as u64,
                index_layers: 4,
                memory_bytes: st.memory_bytes(),
            }
        })
    }
}
