//! DAG topological sort benchmarks.
//!
//! Why this matters: the sequencer rebuilds the DAG and runs `topological_levels`
//! every time an operator clicks "Start application". For a small app of 30
//! components it should be invisible; for a 500-component flagship application
//! it has to stay sub-millisecond so the UI feels instant. These benches
//! measure three representative shapes:
//!
//!  - Linear 5-node chain (a tiny tutorial app)
//!  - Diamond with 50 parallel middle nodes (web tier behind one load balancer)
//!  - Wide 10-level × 50-per-level (500 nodes, ~22k edges) — the realistic
//!    upper bound for a single enterprise application
//!
//! All three call `Dag::topological_levels()` from the production crate
//! (`appcontrol_backend::core::dag`), so any regression here is a real
//! regression in the production hot path.

use appcontrol_benchmarks::dag_support::{diamond, linear_chain, wide_dag};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

fn bench_topological_levels(c: &mut Criterion) {
    let mut group = c.benchmark_group("dag::topological_levels");

    // Each iteration runs one sort. Throughput is "1 element" = "1 sort".
    group.throughput(Throughput::Elements(1));

    // ---- Shape 1: linear chain (5 nodes)
    {
        let dag = linear_chain(5);
        let n = dag.nodes.len();
        group.bench_with_input(BenchmarkId::new("linear", n), &dag, |b, dag| {
            b.iter(|| {
                let lvls = black_box(dag).topological_levels().unwrap();
                black_box(lvls)
            });
        });
    }

    // ---- Shape 2: diamond (50 parallel middle nodes, 52 nodes total)
    {
        let dag = diamond(50);
        let n = dag.nodes.len();
        group.bench_with_input(BenchmarkId::new("diamond", n), &dag, |b, dag| {
            b.iter(|| {
                let lvls = black_box(dag).topological_levels().unwrap();
                black_box(lvls)
            });
        });
    }

    // ---- Shape 3: wide 10×50 (500 nodes, 22 500 edges)
    {
        let dag = wide_dag(10, 50);
        let n = dag.nodes.len();
        group.bench_with_input(BenchmarkId::new("wide_10x50", n), &dag, |b, dag| {
            b.iter(|| {
                let lvls = black_box(dag).topological_levels().unwrap();
                black_box(lvls)
            });
        });
    }

    group.finish();
}

fn bench_find_all_dependencies(c: &mut Criterion) {
    // Common operation when computing "stop everything that depends on X".
    let mut group = c.benchmark_group("dag::find_all_dependencies");
    group.throughput(Throughput::Elements(1));

    let dag = wide_dag(10, 50);
    // The "deepest" node — last layer, slot 0 — has 9 layers × 50 deps
    // transitively.
    let deepest = uuid::Uuid::from_u128(((9_u128) << 32) | 1);
    group.bench_function("wide_10x50_deepest_node", |b| {
        b.iter(|| {
            let deps = black_box(&dag).find_all_dependencies(black_box(deepest));
            black_box(deps)
        });
    });

    group.finish();
}

criterion_group!(benches, bench_topological_levels, bench_find_all_dependencies);
criterion_main!(benches);
