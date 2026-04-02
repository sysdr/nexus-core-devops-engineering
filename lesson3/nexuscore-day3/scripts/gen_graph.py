#!/usr/bin/env python3
"""Generate a synthetic NexusCore CSR graph binary blob for testing."""
import random
import struct
import sys


def gen(n_nodes: int = 1000, avg_degree: int = 8, seed: int = 42) -> None:
    rng = random.Random(seed)
    edges: dict[int, list[int]] = {}
    for i in range(n_nodes):
        k = rng.randint(1, avg_degree * 2)
        edges[i] = [rng.randint(0, n_nodes - 1) for _ in range(k)]

    row_ptr: list[int] = [0]
    col_idx: list[int] = []
    for i in range(n_nodes):
        col_idx.extend(edges[i])
        row_ptr.append(len(col_idx))
    nnz = len(col_idx)

    arena = bytearray()
    doc_offsets: list[int] = []
    for i in range(nnz):
        body = (
            f'{{"id":{i},"title":"Post {i}","body":"content_{rng.randbytes(32).hex()}"}}'
        ).encode()
        off = len(arena)
        arena.extend(body)
        packed = (off << 20) | (len(body) & 0xFFFFF)
        doc_offsets.append(packed)

    buf = bytearray()
    buf += struct.pack("<II", n_nodes, nnz)
    for v in row_ptr:
        buf += struct.pack("<I", v)
    for v in col_idx:
        buf += struct.pack("<I", v)
    for v in doc_offsets:
        buf += struct.pack("<Q", v)
    buf += arena

    out = "data/graph.blob"
    with open(out, "wb") as f:
        f.write(buf)
    print(
        f"[gen] nodes={n_nodes} edges={nnz} arena={len(arena)}B → {out} ({len(buf)} B)"
    )


if __name__ == "__main__":
    n = int(sys.argv[1]) if len(sys.argv) > 1 else 1000
    gen(n_nodes=n)
