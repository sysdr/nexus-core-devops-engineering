//! CSR (Compressed Sparse Row) graph in Wasm linear memory.
//!
//! Memory layout (single linear memory region):
//!
//!   [0..64]         Header block (node_count, edge_count, offsets, generation)
//!   [64..H]         row_ptr: [u32; node_capacity + 1]  — row start indices
//!   [H..H+E*4]      col_idx: [u32; edge_capacity]       — sorted edge targets
//!   [rest]          Bump allocator free space
//!
//! All pointer arithmetic is explicit. No Box<T>, no Vec<T> in hot paths.

use core::slice;

const HEADER_MAGIC: u64 = 0xC5_B0_4E_58_55_53_00_01; // NEXUS CSR v1

#[repr(C, packed)]
pub struct CsrHeader {
    pub magic:             u64,
    pub node_capacity:     u32,
    pub edge_capacity:     u32,
    pub node_count:        u32,
    pub edge_count:        u32,
    pub row_ptr_offset:    u32,
    pub col_idx_offset:    u32,
    pub bump_watermark:    u32,
    pub generation:        u64,
    pub _pad:              [u8; 16],
}

pub struct CsrGraph {
    mem: *mut u8,
    mem_len: usize,
}

impl CsrGraph {
    pub unsafe fn init(memory: *mut u8, mem_len: usize, node_cap: u32, edge_cap: u32)
        -> Result<Self, &'static str>
    {
        let required = Self::required_bytes(node_cap, edge_cap);
        if mem_len < required {
            return Err("insufficient linear memory for requested capacity");
        }

        let g = CsrGraph { mem: memory, mem_len };
        let hdr = g.header_mut();

        hdr.magic          = HEADER_MAGIC;
        hdr.node_capacity  = node_cap;
        hdr.edge_capacity  = edge_cap;
        hdr.node_count     = 0;
        hdr.edge_count     = 0;
        hdr.generation     = 0;

        hdr.row_ptr_offset = 64;
        hdr.col_idx_offset = 64 + (node_cap + 1) * 4;
        hdr.bump_watermark = hdr.col_idx_offset + edge_cap * 4;
        hdr._pad           = [0u8; 16];

        let row_ptr_slice = slice::from_raw_parts_mut(
            memory.add(hdr.row_ptr_offset as usize) as *mut u32,
            (node_cap + 1) as usize,
        );
        row_ptr_slice.fill(0);

        Ok(g)
    }

    pub fn required_bytes(node_cap: u32, edge_cap: u32) -> usize {
        64
        + (node_cap as usize + 1) * 4
        + edge_cap as usize * 4
    }

    #[inline(always)]
    unsafe fn header(&self) -> &CsrHeader {
        &*(self.mem as *const CsrHeader)
    }

    #[inline(always)]
    unsafe fn header_mut(&self) -> &mut CsrHeader {
        &mut *(self.mem as *mut CsrHeader)
    }

    unsafe fn row_ptr_slice(&self) -> &[u32] {
        let hdr = self.header();
        slice::from_raw_parts(
            self.mem.add(hdr.row_ptr_offset as usize) as *const u32,
            hdr.node_capacity as usize + 1,
        )
    }

    unsafe fn col_idx_slice(&self) -> &[u32] {
        let hdr = self.header();
        slice::from_raw_parts(
            self.mem.add(hdr.col_idx_offset as usize) as *const u32,
            hdr.edge_count as usize,
        )
    }

    unsafe fn col_idx_slice_mut(&self) -> &mut [u32] {
        let hdr = self.header();
        slice::from_raw_parts_mut(
            self.mem.add(hdr.col_idx_offset as usize) as *mut u32,
            hdr.edge_capacity as usize,
        )
    }

    pub unsafe fn add_edge_batch(&mut self, edges: &[(u32, u32)])
        -> Result<u32, &'static str>
    {
        if edges.is_empty() { return Ok(0); }

        let hdr  = self.header_mut();
        if hdr.magic != HEADER_MAGIC { return Err("corrupted CSR header"); }

        let new_total_edges = hdr.edge_count as usize + edges.len();
        if new_total_edges > hdr.edge_capacity as usize {
            return Err("edge capacity exhausted");
        }

        for &(src, dst) in edges {
            if src >= hdr.node_capacity || dst >= hdr.node_capacity {
                return Err("node ID exceeds capacity");
            }
        }

        // Safe (but not allocation-free) implementation:
        // rebuild CSR each batch by merging existing edges with new edges.
        let node_cap = hdr.node_capacity as usize;

        // Collect existing edges from current CSR.
        let row_ptr_existing = self.row_ptr_slice();
        let col_existing = self.col_idx_slice();
        let mut all_edges: Vec<(u32, u32)> = Vec::with_capacity(new_total_edges);

        // node_count can be 0 while row_ptr is still allocated; iterate full capacity.
        for src in 0..node_cap {
            let start = row_ptr_existing[src] as usize;
            let end = row_ptr_existing[src + 1] as usize;
            if end > col_existing.len() {
                return Err("corrupted CSR (row_ptr out of range)");
            }
            for &dst in &col_existing[start..end] {
                all_edges.push((src as u32, dst));
            }
        }

        // Append new edges.
        all_edges.extend_from_slice(edges);

        // Sort by (src, dst) and rebuild.
        all_edges.sort_unstable();

        // Determine node_count as (max src + 1), but keep within capacity.
        let mut max_src = 0u32;
        for &(src, _) in &all_edges {
            if src > max_src { max_src = src; }
        }
        hdr.node_count = (max_src + 1).min(hdr.node_capacity);

        let row_ptr = slice::from_raw_parts_mut(
            self.mem.add(hdr.row_ptr_offset as usize) as *mut u32,
            node_cap + 1,
        );
        row_ptr.fill(0);

        // Degree counts.
        for &(src, _) in &all_edges {
            row_ptr[src as usize] += 1;
        }

        // Prefix sums.
        let mut prefix: u32 = 0;
        for i in 0..node_cap {
            let deg = row_ptr[i];
            row_ptr[i] = prefix;
            prefix = prefix.saturating_add(deg);
        }
        row_ptr[node_cap] = prefix;

        // Scatter into col_idx.
        let col_idx = self.col_idx_slice_mut();
        let mut cursors: Vec<u32> = Vec::with_capacity(node_cap);
        cursors.extend_from_slice(&row_ptr[..node_cap]);

        for &(_src, _dst) in &all_edges {
            // filled below
        }
        for &(src, dst) in &all_edges {
            let pos = cursors[src as usize] as usize;
            if pos >= hdr.edge_capacity as usize {
                return Err("corrupted CSR (scatter out of capacity)");
            }
            col_idx[pos] = dst;
            cursors[src as usize] += 1;
        }

        // Update counts/watermark/generation.
        hdr.edge_count = all_edges.len() as u32;
        hdr.generation += 1;
        Ok(edges.len() as u32)
    }

    pub unsafe fn get_following(&self, node_id: u32) -> Result<&[u32], &'static str> {
        let hdr = self.header();
        if hdr.magic != HEADER_MAGIC { return Err("corrupted header"); }
        if node_id >= hdr.node_capacity { return Err("invalid node ID"); }

        let row_ptr = self.row_ptr_slice();
        let col_idx = self.col_idx_slice();

        let start = row_ptr[node_id as usize] as usize;
        let end   = row_ptr[node_id as usize + 1] as usize;

        if start == end { return Ok(&[]); }
        Ok(&col_idx[start..end])
    }

    pub unsafe fn edge_exists(&self, src: u32, dst: u32) -> bool {
        match self.get_following(src) {
            Ok(neighbors) => neighbors.binary_search(&dst).is_ok(),
            Err(_) => false,
        }
    }

    pub unsafe fn get_followers(&self, node_id: u32) -> Result<Vec<u32>, &'static str> {
        let hdr = self.header();
        if node_id >= hdr.node_capacity { return Err("invalid node ID"); }

        let mut followers = Vec::new();
        for src in 0..hdr.node_count {
            if self.edge_exists(src, node_id) {
                followers.push(src);
            }
        }
        Ok(followers)
    }

    pub unsafe fn generation(&self) -> u64    { self.header().generation }
    pub unsafe fn edge_count(&self) -> u32    { self.header().edge_count }
    pub unsafe fn node_count(&self) -> u32    { self.header().node_count }
    pub unsafe fn watermark(&self) -> usize   { self.header().bump_watermark as usize }
    pub unsafe fn memory_len(&self) -> usize  { self.mem_len }
}

unsafe impl Send for CsrGraph {}
unsafe impl Sync for CsrGraph {}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_graph(nodes: u32, edges: u32) -> (Vec<u8>, CsrGraph) {
        let sz = CsrGraph::required_bytes(nodes, edges);
        let mut mem = vec![0u8; sz];
        let g = unsafe { CsrGraph::init(mem.as_mut_ptr(), sz, nodes, edges).unwrap() };
        (mem, g)
    }

    #[test]
    fn test_basic_edge_insert() {
        let (_mem, mut g) = make_graph(100, 1000);
        let edges = vec![(0u32, 1u32), (0, 2), (0, 5), (1, 3), (2, 0)];
        unsafe {
            g.add_edge_batch(&edges).unwrap();
            let following_0 = g.get_following(0).unwrap();
            assert_eq!(following_0, &[1, 2, 5]);
            assert!(g.edge_exists(0, 2));
            assert!(!g.edge_exists(0, 99));
            assert_eq!(g.generation(), 1);
        }
    }

    #[test]
    fn test_csr_sorted_invariant() {
        let (_mem, mut g) = make_graph(1000, 100_000);
        let edges: Vec<(u32, u32)> = (0..100u32)
            .flat_map(|src| (0..100u32).rev().map(move |dst| (src, dst)))
            .collect();
        unsafe {
            g.add_edge_batch(&edges).unwrap();
            for src in 0..100u32 {
                let row = g.get_following(src).unwrap();
                for w in row.windows(2) {
                    assert!(w[0] < w[1], "CSR row not sorted at src={}", src);
                }
            }
        }
    }
}
