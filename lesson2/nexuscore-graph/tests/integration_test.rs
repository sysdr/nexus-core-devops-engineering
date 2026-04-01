use nexuscore_graph::csr_graph::CsrGraph;

#[test]
fn test_million_edge_consistency() {
    const NODES: u32 = 10_000;
    const EDGES: u32 = 500_000;
    let sz = CsrGraph::required_bytes(NODES, EDGES);
    let mut mem = vec![0u8; sz];

    let mut g = unsafe { CsrGraph::init(mem.as_mut_ptr(), sz, NODES, EDGES).unwrap() };

    use rand::{Rng, SeedableRng};
    let mut rng = rand::rngs::SmallRng::seed_from_u64(42);
    let mut total = 0u32;

    for _ in 0..50 {
        let batch: Vec<(u32, u32)> = (0..1000)
            .map(|_| (rng.gen_range(0..NODES), rng.gen_range(0..NODES)))
            .filter(|&(s, d)| s != d)
            .collect();
        unsafe { total += g.add_edge_batch(&batch).unwrap(); }
    }

    unsafe {
        for node in 0..g.node_count() {
            let row = g.get_following(node).unwrap();
            for w in row.windows(2) {
                assert!(w[0] <= w[1], "CSR row {} not sorted: {} > {}", node, w[0], w[1]);
            }
        }
    }

    assert_eq!(unsafe { g.generation() }, 50, "generation mismatch");
    println!("Total edges inserted: {}", total);
}
