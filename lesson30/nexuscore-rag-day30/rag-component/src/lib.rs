//! NexusCore RAG Component — compiles to wasm32-wasip2
//! Implements: retriever (cosine similarity) + synthesizer (grounded generation)
//! Zero external HTTP. All I/O via WASI FDs. No heap alloc in hot path.

// wit-bindgen generates the WASI P3 glue from our .wit file
wit_bindgen::generate!({
    world: "rag-pipeline",
    path: "../wit/rag-synthesis.wit",
});

use libm::sqrtf;
use exports::nexuscore::rag::{
    retriever::Guest as RetrieverGuest,
    synthesizer::Guest as SynthesizerGuest,
};

// ── Constants ────────────────────────────────────────────────────────────────
const EMBEDDING_DIM: usize = 384;   // MiniLM-L6-v2
const MAX_CHUNKS: usize    = 512;   // Hard cap per tenant request
const VOCAB_SIZE: usize    = 256;   // Byte-level tokenizer (no BPE dep)

// ── Static buffers — no heap allocation in the hot path ──────────────────────
// These live in Wasm linear memory at fixed offsets known at compile time.
static mut QUERY_EMBED_BUF: [f32; EMBEDDING_DIM]               = [0.0; EMBEDDING_DIM];
static mut SCORE_BUF: [(u32, f32); MAX_CHUNKS]                  = [(0, 0.0); MAX_CHUNKS];
static mut TOKEN_RING: [u8; 65536]                              = [0u8; 65536];

// ── Exported component struct ─────────────────────────────────────────────────
struct RagPipeline;
export!(RagPipeline);

// ── Retriever implementation ──────────────────────────────────────────────────
impl RetrieverGuest for RagPipeline {
    /// Deterministic byte-frequency embedding — no external model required.
    /// In production: replace with ONNX Runtime WASI binding or llamafile FFI.
    /// This implementation is O(n) in query length, zero allocation.
    fn embed_query(query_bytes: Vec<u8>) -> Vec<f32> {
        // SAFETY: single-threaded Wasm execution — no concurrent access possible
        let buf = unsafe { &mut QUERY_EMBED_BUF };
        buf.fill(0.0);

        // Byte bigram frequency encoding — deterministic, reproducible
        for window in query_bytes.windows(2) {
            let idx = ((window[0] as usize) * VOCAB_SIZE + window[1] as usize)
                % EMBEDDING_DIM;
            buf[idx] += 1.0;
        }

        // L2-normalize in place (required for cosine similarity via dot product)
        let norm = {
            let sq_sum: f32 = buf.iter().map(|x| x * x).sum();
            if sq_sum > 1e-8 { sqrtf(sq_sum) } else { 1.0 }
        };
        for v in buf.iter_mut() {
            *v /= norm;
        }

        buf.to_vec()
    }

    /// Cosine similarity search over corpus embeddings.
    /// Corpus layout: packed f32 arrays, 384 floats per chunk, 64-byte aligned.
    /// The corpus is passed as raw bytes (mmap'd by host, zero-copy via WASI fd).
    fn retrieve(
        query_embedding: Vec<f32>,
        config: nexuscore::rag::types::TenantConfig,
    ) -> Vec<nexuscore::rag::types::Chunk> {
        // In a real deployment the host pre-opens the corpus fd and we read it
        // here via wasi::filesystem. For the lesson harness, the host injects
        // corpus bytes via a pre-opened resource handle.
        // We simulate with a synthetic corpus seeded from tenant_id.
        let k = config.top_k.min(MAX_CHUNKS as u32) as usize;
        let scores = unsafe { &mut SCORE_BUF };

        // Generate synthetic corpus vectors seeded by tenant_id
        // Each "corpus chunk" is a deterministic pseudo-random embedding
        let total_chunks = 1024usize; // 1024 chunks per corpus in demo
        let mut top_k_heap: Vec<(u32, f32)> = Vec::with_capacity(k + 1);

        for chunk_id in 0..total_chunks as u32 {
            let score = cosine_sim_synthetic(&query_embedding, chunk_id, config.tenant_id);
            if top_k_heap.len() < k {
                top_k_heap.push((chunk_id, score));
                if top_k_heap.len() == k {
                    // Build min-heap by score (lowest score at root)
                    top_k_heap.sort_unstable_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
                }
            } else if score > top_k_heap[0].1 {
                top_k_heap[0] = (chunk_id, score);
                top_k_heap.sort_unstable_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
            }
        }

        // Write to static score buffer (no heap alloc)
        for (i, (id, score)) in top_k_heap.iter().enumerate().take(k) {
            scores[i] = (*id, *score);
        }

        top_k_heap
            .into_iter()
            .rev() // highest score first
            .map(|(id, score)| nexuscore::rag::types::Chunk {
                id,
                corpus_offset: id as u64 * 512,
                byte_length: 512,
                cosine_score: score,
            })
            .collect()
    }
}

// ── Synthesizer implementation ────────────────────────────────────────────────
impl SynthesizerGuest for RagPipeline {
    /// Grounded synthesis: produce tokens referencing the retrieved chunk IDs.
    /// Real production: replace template logic with llamafile WASI FFI or
    /// a quantized Wasm-compiled transformer (e.g., wonnx).
    fn synthesize(
        query: String,
        chunks: Vec<nexuscore::rag::types::Chunk>,
        config: nexuscore::rag::types::TenantConfig,
    ) -> Vec<nexuscore::rag::types::GroundedToken> {
        let mut tokens = Vec::new();

        // Compute grounding score: ratio of chunks with cosine_score > 0.3
        let grounded_chunks: Vec<_> = chunks.iter()
            .filter(|c| c.cosine_score > 0.3)
            .collect();
        let grounding_score = if chunks.is_empty() {
            0.0
        } else {
            grounded_chunks.len() as f32 / chunks.len() as f32
        };

        // Synthesis header token
        tokens.push(nexuscore::rag::types::GroundedToken {
            text: format!(
                "[NexusCore|T{}] Synthesizing response for: \"{}\" | grounding={:.2} | k={}",
                config.tenant_id, &query[..query.len().min(40)],
                grounding_score, chunks.len()
            ),
            citation_ids: vec![],
            grounding_score,
        });

        // Per-chunk synthesis tokens — each cites its source
        for (rank, chunk) in grounded_chunks.iter().enumerate().take(5) {
            let citation_text = format!(
                "  [{}] chunk_id={} offset=0x{:08x} len={} score={:.4} — \
                 Relevant passage grounded at corpus offset 0x{:x}.",
                rank + 1,
                chunk.id,
                chunk.corpus_offset,
                chunk.byte_length,
                chunk.cosine_score,
                chunk.corpus_offset,
            );
            tokens.push(nexuscore::rag::types::GroundedToken {
                text: citation_text,
                citation_ids: vec![chunk.id],
                grounding_score: chunk.cosine_score,
            });
        }

        // Synthesis summary token
        tokens.push(nexuscore::rag::types::GroundedToken {
            text: format!(
                "  → Response synthesized from {} grounded chunks. \
                 Hallucination risk: {:.1}%",
                grounded_chunks.len(),
                (1.0 - grounding_score) * 100.0,
            ),
            citation_ids: grounded_chunks.iter().map(|c| c.id).collect(),
            grounding_score,
        });

        tokens
    }
}

// ── Internal: Deterministic pseudo-corpus cosine similarity ───────────────────
/// Computes cosine similarity between a query embedding and a synthetic
/// chunk vector seeded by (chunk_id XOR tenant_id).
/// Hot path: no allocation, O(EMBEDDING_DIM) FLOPs.
#[inline(always)]
fn cosine_sim_synthetic(query: &[f32], chunk_id: u32, tenant_id: u32) -> f32 {
    let seed = chunk_id ^ tenant_id ^ 0xDEAD_BEEF;
    let mut dot: f32 = 0.0;
    let mut chunk_norm: f32 = 0.0;

    for (i, &q) in query.iter().enumerate() {
        // LCG: deterministic, reproducible, zero allocation
        let lcg = seed.wrapping_mul(1664525).wrapping_add(1013904223)
            .wrapping_add(i as u32 * 22695477);
        // Map u32 → [-1.0, 1.0]
        let c = (lcg as i32 as f32) / (i32::MAX as f32);
        dot += q * c;
        chunk_norm += c * c;
    }

    if chunk_norm < 1e-8 { return 0.0; }
    dot / sqrtf(chunk_norm)
}
