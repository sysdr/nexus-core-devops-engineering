mod csr_graph;

use csr_graph::CsrGraph;
use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};
use std::time::Instant;

const NODE_CAPACITY: u32 = 1_000_000;
const EDGE_CAPACITY: u32 = 50_000_000;
const BENCH_EDGES: u32 = 5_000_000;
const BATCH_SIZE: u32 = 10_000;
const HOT_THRESHOLD: usize = 1000;

fn main() -> anyhow::Result<()> {
    println!("\n{}", style("═══ NexusCore · Day 2 · Graph Engine Demo ═══").bold().cyan());
    println!("{}\n", style("CSR Graph in Linear Memory + Hot-Node Detection").dim());

    let mem_size = CsrGraph::required_bytes(NODE_CAPACITY, EDGE_CAPACITY);
    println!(
        "{} Allocating linear memory: {:.2} GB",
        style("→").green().bold(),
        mem_size as f64 / 1024.0 / 1024.0 / 1024.0
    );

    let mut memory: Vec<u8> = vec![0u8; mem_size];
    let graph = unsafe {
        CsrGraph::init(memory.as_mut_ptr(), mem_size, NODE_CAPACITY, EDGE_CAPACITY)
            .map_err(anyhow::Error::msg)?
    };

    run_benchmark(graph)?;
    Ok(())
}

fn run_benchmark(mut graph: CsrGraph) -> anyhow::Result<()> {
    let mp = MultiProgress::new();
    let style_str = "{spinner:.green} [{bar:50.cyan/blue}] {pos}/{len} {msg}";
    let pb_style = ProgressStyle::with_template(style_str)?.progress_chars("█▓░");

    println!(
        "\n{} Phase 1: Bulk Edge Insertion ({} edges, batch={})",
        style("▶").yellow().bold(),
        BENCH_EDGES,
        BATCH_SIZE
    );

    let pb = mp.add(ProgressBar::new((BENCH_EDGES / BATCH_SIZE) as u64));
    pb.set_style(pb_style.clone());
    pb.set_message("inserting edges...");

    let mut rng = SmallRng::seed_from_u64(0xDEAD_BEEF_C5B0_4E58);
    let mut total_inserted = 0u32;
    let insert_start = Instant::now();

    let n_batches = (BENCH_EDGES / BATCH_SIZE) as usize;
    for batch_i in 0..n_batches {
        let mut batch: Vec<(u32, u32)> = Vec::with_capacity(BATCH_SIZE as usize);
        for _ in 0..BATCH_SIZE {
            let src = if rng.gen_bool(0.20) {
                rng.gen_range(0..NODE_CAPACITY)
            } else {
                rng.gen_range(0..(NODE_CAPACITY / 100))
            };
            let dst = rng.gen_range(0..NODE_CAPACITY);
            if src != dst {
                batch.push((src, dst));
            }
        }

        let inserted = unsafe { graph.add_edge_batch(&batch).map_err(anyhow::Error::msg)? };
        total_inserted += inserted;

        if batch_i % 10 == 0 {
            pb.inc(10);
            let elapsed = insert_start.elapsed().as_secs_f64();
            let rate = total_inserted as f64 / elapsed;
            pb.set_message(format!("{:.1}M edges/sec", rate / 1_000_000.0));
        }
    }
    pb.finish_with_message("done");

    println!(
        "\n  {} Inserted {} edges in {:.2}s",
        style("✓").green().bold(),
        total_inserted,
        insert_start.elapsed().as_secs_f64()
    );
    println!(
        "  {} Generation: {} | Memory watermark: {:.1} MB",
        style("→").blue(),
        unsafe { graph.generation() },
        unsafe { graph.watermark() } as f64 / 1024.0 / 1024.0,
    );

    println!(
        "\n{} Phase 3: Hot-Node Detection (out-degree scan)",
        style("▶").yellow().bold()
    );

    let mut hot_nodes: Vec<(u32, usize)> = Vec::new();
    unsafe {
        let hdr_node_count = graph.node_count();
        for node in 0..hdr_node_count {
            let following = graph.get_following(node).unwrap_or(&[]);
            if following.len() >= HOT_THRESHOLD {
                hot_nodes.push((node, following.len()));
            }
        }
    }
    hot_nodes.sort_unstable_by_key(|&(_, deg)| std::cmp::Reverse(deg));

    println!("\n  {} Top 5 hot nodes:", style("→").blue());
    for &(node, degree) in hot_nodes.iter().take(5) {
        println!("  node={} out_degree={}", node, degree);
    }

    Ok(())
}
