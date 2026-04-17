#!/usr/bin/env bash
echo "=== NexusCore Verification ==="
echo ""
echo "1. Rust toolchain:"
  rustc --version 2>/dev/null && echo "   ✓ rustc found" || echo "   ✗ rustc missing"
  cargo --version 2>/dev/null && echo "   ✓ cargo found" || echo "   ✗ cargo missing"

echo ""
echo "2. WASI target:"
  rustup target list --installed 2>/dev/null | grep -q wasm32-wasip2 \
    && echo "   ✓ wasm32-wasip2 installed" \
    || echo "   ✗ wasm32-wasip2 missing — run: rustup target add wasm32-wasip2"

echo ""
echo "3. eBPF toolchain:"
  clang --version 2>/dev/null | head -1 || echo "   ✗ clang missing"
  bpftool version 2>/dev/null | head -1 || echo "   ✗ bpftool missing"

echo ""
echo "4. Go:"
  go version 2>/dev/null || echo "   ✗ go missing"

echo ""
echo "5. Corpus:"
  test -f data/corpus.jsonl \
    && echo "   ✓ corpus.jsonl: $(wc -l < data/corpus.jsonl) chunks" \
    || echo "   ✗ corpus.jsonl missing — run: python3 scripts/gen_corpus.py"
  test -f data/embeddings.bin \
    && echo "   ✓ embeddings.bin: $(wc -c < data/embeddings.bin) bytes" \
    || echo "   ✗ embeddings.bin missing"

echo ""
echo "6. WIT interface:"
  test -f wit/rag-synthesis.wit \
    && echo "   ✓ rag-synthesis.wit found" \
    || echo "   ✗ WIT interface missing"

echo ""
echo "7. eBPF probe:"
  test -f ebpf/rag_probe.bpf.c \
    && echo "   ✓ rag_probe.bpf.c found" \
    || echo "   ✗ eBPF probe missing"
