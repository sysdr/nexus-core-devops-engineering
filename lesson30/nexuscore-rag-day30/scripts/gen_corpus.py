#!/usr/bin/env python3
"""
Generates a synthetic RAG corpus:
  - corpus.jsonl: 1024 text chunks
  - embeddings.bin: packed f32[384] per chunk (64-byte aligned)
No external dependencies beyond stdlib.
"""
import json, struct, math, random, pathlib, sys

random.seed(42)
DIM = 384
N_CHUNKS = 1024
OUT_DIR = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else pathlib.Path("data")
OUT_DIR.mkdir(parents=True, exist_ok=True)

TOPICS = [
    "kernel networking", "eBPF observability", "WASI component model",
    "wasmtime runtime", "cosine similarity", "grounding score",
    "multi-tenant isolation", "zero-copy I/O", "TLB thrashing",
    "synthesis pipeline", "Rust linear memory", "BPF maps",
]

def random_unit_vec(seed):
    random.seed(seed)
    v = [random.gauss(0, 1) for _ in range(DIM)]
    norm = math.sqrt(sum(x*x for x in v))
    return [x/norm for x in v]

jsonl_path = OUT_DIR / "corpus.jsonl"
emb_path   = OUT_DIR / "embeddings.bin"

with open(jsonl_path, "w") as jf, open(emb_path, "wb") as ef:
    for i in range(N_CHUNKS):
        topic = TOPICS[i % len(TOPICS)]
        chunk = {
            "id": i,
            "text": (
                f"[Chunk {i:04d}] NexusCore technical note on {topic}. "
                f"This passage covers the implementation details of {topic} "
                f"in the context of high-performance distributed systems at "
                f"hyperscale (100M+ RPS). Key concepts: zero-copy, WASI P3, "
                f"eBPF CO-RE, tenant isolation, grounding score computation. "
                f"Reference ID: NC-{i:04d}-{hash(topic) & 0xFFFF:04X}."
            ),
            "offset": i * 512,
            "length": 512,
        }
        jf.write(json.dumps(chunk) + "\n")
        vec = random_unit_vec(i)
        ef.write(struct.pack(f"{DIM}f", *vec))

print(f"Generated {N_CHUNKS} chunks → {jsonl_path} ({jsonl_path.stat().st_size} bytes)")
print(f"Embeddings          → {emb_path} ({emb_path.stat().st_size} bytes)")
