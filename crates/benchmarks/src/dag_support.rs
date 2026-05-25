//! Synthetic DAG builders used by the dag_topo benchmark.
//!
//! Three shapes are exposed:
//!  - `linear_chain(n)`: 5..50 nodes, every node depends on the previous one.
//!    The narrowest possible DAG — worst-case for parallelism, best-case for
//!    Kahn's algorithm cache locality.
//!  - `diamond(width)`: classic split-merge with one root, `width` parallel
//!    middle nodes, one sink. Stresses the "many edges per node" path.
//!  - `wide_dag(levels, per_level)`: `levels` layers of `per_level` nodes
//!    each, every node in layer L depends on every node in layer L-1.
//!    O(levels × per_level²) edges — the most realistic shape for a large
//!    enterprise application graph (think: 50 components per app, 10 layers).

use appcontrol_backend::core::dag::Dag;
use uuid::Uuid;

/// Build a linear chain of `n` nodes: node_i depends on node_{i-1}.
pub fn linear_chain(n: usize) -> Dag {
    let mut dag = Dag::new();
    let ids: Vec<Uuid> = (0..n).map(|i| Uuid::from_u128(i as u128 + 1)).collect();
    for &id in &ids {
        dag.add_node(id);
    }
    for w in ids.windows(2) {
        // w[1] depends on w[0]
        dag.add_edge(w[1], w[0]);
    }
    dag
}

/// Build a diamond-shaped DAG: 1 root → `width` middle nodes → 1 sink.
/// Useful as a small-fan-out smoke test.
pub fn diamond(width: usize) -> Dag {
    let mut dag = Dag::new();
    let root = Uuid::from_u128(1);
    let sink = Uuid::from_u128(2);
    dag.add_node(root);
    dag.add_node(sink);
    for i in 0..width {
        let mid = Uuid::from_u128((i as u128) + 10);
        dag.add_node(mid);
        // mid depends on root
        dag.add_edge(mid, root);
        // sink depends on mid
        dag.add_edge(sink, mid);
    }
    dag
}

/// Build a wide layered DAG: `levels` layers of `per_level` nodes each,
/// fully connected between adjacent layers (every node in layer L depends
/// on every node in layer L-1). For levels=10, per_level=50: 500 nodes and
/// 50 × 50 × 9 = 22 500 edges.
pub fn wide_dag(levels: usize, per_level: usize) -> Dag {
    let mut dag = Dag::new();
    // Assign one UUID per (layer, slot).
    let id = |layer: usize, slot: usize| -> Uuid {
        Uuid::from_u128(((layer as u128) << 32) | (slot as u128 + 1))
    };
    for layer in 0..levels {
        for slot in 0..per_level {
            dag.add_node(id(layer, slot));
        }
    }
    for layer in 1..levels {
        for slot in 0..per_level {
            let from = id(layer, slot);
            for prev in 0..per_level {
                let to = id(layer - 1, prev);
                dag.add_edge(from, to);
            }
        }
    }
    dag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_chain_has_expected_size() {
        let dag = linear_chain(5);
        assert_eq!(dag.nodes.len(), 5);
        let levels = dag.topological_levels().unwrap();
        assert_eq!(levels.len(), 5);
    }

    #[test]
    fn diamond_has_three_levels() {
        let dag = diamond(3);
        let levels = dag.topological_levels().unwrap();
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0].len(), 1); // root
        assert_eq!(levels[1].len(), 3); // middle
        assert_eq!(levels[2].len(), 1); // sink
    }

    #[test]
    fn wide_dag_500_components() {
        let dag = wide_dag(10, 50);
        assert_eq!(dag.nodes.len(), 500);
        let levels = dag.topological_levels().unwrap();
        assert_eq!(levels.len(), 10);
        for level in &levels {
            assert_eq!(level.len(), 50);
        }
    }
}
