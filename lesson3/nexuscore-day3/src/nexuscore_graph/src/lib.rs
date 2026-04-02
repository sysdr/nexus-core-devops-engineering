//! NexusCore Day 3 — CSR graph engine (native library for tests and tooling).
//! Packed blob format: [u32-le n_nodes][u32-le nnz][row_ptr u32*][col_idx u32*][doc_offsets u64*][arena u8*]

use std::collections::BTreeSet;

/// Compressed sparse row graph with document arena.
pub struct GraphEngine {
    row_ptr: Vec<u32>,
    col_idx: Vec<u32>,
    doc_offsets: Vec<u64>,
    arena: Vec<u8>,
}

impl GraphEngine {
    pub fn from_blob(blob: &[u8]) -> Result<Self, &'static str> {
        if blob.len() < 8 {
            return Err("blob too short");
        }
        let n_nodes = u32::from_le_bytes(blob[0..4].try_into().unwrap()) as usize;
        let nnz = u32::from_le_bytes(blob[4..8].try_into().unwrap()) as usize;

        let mut off = 8usize;
        let row_ptr_bytes = (n_nodes + 1) * 4;
        let col_idx_bytes = nnz * 4;
        let doc_off_bytes = nnz * 8;

        if blob.len() < off + row_ptr_bytes + col_idx_bytes + doc_off_bytes {
            return Err("blob truncated");
        }

        let row_ptr: Vec<u32> = blob[off..off + row_ptr_bytes]
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        off += row_ptr_bytes;

        let col_idx: Vec<u32> = blob[off..off + col_idx_bytes]
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        off += col_idx_bytes;

        let doc_offsets: Vec<u64> = blob[off..off + doc_off_bytes]
            .chunks_exact(8)
            .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
            .collect();
        off += doc_off_bytes;

        let arena = blob[off..].to_vec();

        Ok(Self {
            row_ptr,
            col_idx,
            doc_offsets,
            arena,
        })
    }

    /// BFS over CSR; returns owned document payloads (up to `max_results`, 0 = unlimited).
    pub fn bfs_posts(&self, root: u32, max_depth: u8, max_results: u32) -> Vec<Vec<u8>> {
        let mut visited: BTreeSet<u32> = BTreeSet::new();
        let mut queue: Vec<(u32, u8)> = vec![(root, 0)];
        let mut results: Vec<Vec<u8>> = Vec::new();
        let limit = if max_results == 0 {
            u32::MAX
        } else {
            max_results
        };

        while let Some((node, depth)) = queue.first().cloned() {
            queue.remove(0);
            if depth >= max_depth || !visited.insert(node) {
                continue;
            }
            let n = self.row_ptr.len();
            if node as usize + 1 >= n {
                continue;
            }

            let start = self.row_ptr[node as usize] as usize;
            let end = self.row_ptr[node as usize + 1] as usize;

            for i in start..end {
                let neighbor = self.col_idx[i];
                let packed = self.doc_offsets[i];
                let offset = (packed >> 20) as usize;
                let len = (packed & 0x000F_FFFF) as usize;
                if offset + len <= self.arena.len() {
                    results.push(self.arena[offset..offset + len].to_vec());
                }
                if (results.len() as u32) >= limit {
                    return results;
                }
                queue.push((neighbor, depth + 1));
            }
        }
        results
    }

    /// CSR node count (from `row_ptr` length).
    pub fn n_nodes(&self) -> u32 {
        self.row_ptr.len().saturating_sub(1) as u32
    }

    /// Number of directed edges / document slots in CSR (`col_idx.len()`).
    pub fn nnz(&self) -> u32 {
        self.col_idx.len() as u32
    }

    /// Arena size in bytes.
    pub fn arena_len(&self) -> u64 {
        self.arena.len() as u64
    }

    /// Share of arena not covered by packed doc lengths (slab waste / internal holes proxy).
    pub fn arena_fragmentation_pct(&self) -> f64 {
        let arena = self.arena.len() as f64;
        if arena <= 0.0 {
            return 0.0;
        }
        let mut used: u64 = 0;
        for &packed in &self.doc_offsets {
            used += (packed & 0x000F_FFFF) as u64;
        }
        let waste = (self.arena.len() as u64).saturating_sub(used);
        100.0 * (waste as f64) / arena
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny_blob() -> Vec<u8> {
        // 2 nodes, 1 edge 0->1, one doc on that edge
        let n_nodes: u32 = 2;
        let nnz: u32 = 1;
        let row_ptr = vec![0u32, 1, 1];
        let col_idx = vec![1u32];
        let doc = b"{\"id\":1}".to_vec();
        let packed: u64 = ((0u64) << 20) | ((doc.len() as u64) & 0xFFFFF);
        let mut buf = Vec::new();
        buf.extend_from_slice(&n_nodes.to_le_bytes());
        buf.extend_from_slice(&nnz.to_le_bytes());
        for v in &row_ptr {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        for v in &col_idx {
            buf.extend_from_slice(&v.to_le_bytes());
        }
        buf.extend_from_slice(&packed.to_le_bytes());
        buf.extend_from_slice(&doc);
        buf
    }

    #[test]
    fn from_blob_and_bfs() {
        let g = GraphEngine::from_blob(&tiny_blob()).expect("parse");
        let posts = g.bfs_posts(0, 4, 10);
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0], b"{\"id\":1}");
        assert_eq!(g.n_nodes(), 2);
        assert_eq!(g.nnz(), 1);
        assert!(g.arena_fragmentation_pct() >= 0.0);
    }
}
