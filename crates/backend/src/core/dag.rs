use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum DagError {
    #[error("Cycle detected in dependency graph")]
    CycleDetected,
    #[error("Database error: {0}")]
    Database(String),
}

/// A Directed Acyclic Graph for component dependencies.
#[derive(Debug, Clone)]
pub struct Dag {
    /// component_id -> set of dependencies (component_ids that must start first)
    pub adjacency: HashMap<Uuid, HashSet<Uuid>>,
    /// All component IDs in the graph
    pub nodes: HashSet<Uuid>,
}

impl Dag {
    pub fn new() -> Self {
        Self {
            adjacency: HashMap::new(),
            nodes: HashSet::new(),
        }
    }

    pub fn add_node(&mut self, id: Uuid) {
        self.nodes.insert(id);
        self.adjacency.entry(id).or_default();
    }

    /// Add an edge: `from` depends on `to` (to must start before from).
    pub fn add_edge(&mut self, from: Uuid, to: Uuid) {
        self.nodes.insert(from);
        self.nodes.insert(to);
        self.adjacency.entry(from).or_default().insert(to);
        self.adjacency.entry(to).or_default();
    }

    /// Topological sort using Kahn's algorithm. Returns levels for parallel execution.
    /// Level 0 = no dependencies, Level 1 = depends only on Level 0, etc.
    pub fn topological_levels(&self) -> Result<Vec<Vec<Uuid>>, DagError> {
        let mut in_degree: HashMap<Uuid, usize> = HashMap::new();
        let mut reverse: HashMap<Uuid, HashSet<Uuid>> = HashMap::new();

        for &node in &self.nodes {
            in_degree.entry(node).or_insert(0);
        }

        for (&node, deps) in &self.adjacency {
            for &dep in deps {
                *in_degree.entry(node).or_insert(0) += 1;
                reverse.entry(dep).or_default().insert(node);
            }
        }

        let mut queue: VecDeque<Uuid> = VecDeque::new();
        for (&node, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(node);
            }
        }

        let mut levels: Vec<Vec<Uuid>> = Vec::new();
        let mut processed = 0;

        while !queue.is_empty() {
            let mut current_level: Vec<Uuid> = Vec::new();
            let level_size = queue.len();

            for _ in 0..level_size {
                let node = queue.pop_front().unwrap();
                current_level.push(node);
                processed += 1;

                if let Some(dependents) = reverse.get(&node) {
                    for &dependent in dependents {
                        let degree = in_degree.get_mut(&dependent).unwrap();
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent);
                        }
                    }
                }
            }

            current_level.sort(); // deterministic ordering within a level
            levels.push(current_level);
        }

        if processed != self.nodes.len() {
            return Err(DagError::CycleDetected);
        }

        Ok(levels)
    }

    /// Check if adding edge from->to (from depends on to) would create a cycle.
    /// A cycle exists if `to` can already reach `from` through existing dependency edges.
    pub fn would_create_cycle(&self, from: Uuid, to: Uuid) -> bool {
        // Check: can we reach `from` starting from `to` via the dependency graph?
        let mut visited = HashSet::new();
        let mut stack = vec![to];

        while let Some(current) = stack.pop() {
            if current == from {
                return true;
            }
            if visited.insert(current) {
                if let Some(deps) = self.adjacency.get(&current) {
                    for &dep in deps {
                        stack.push(dep);
                    }
                }
            }
        }

        false
    }
}

/// Build a DAG from the dependencies table for a given application.
pub async fn build_dag(pool: &sqlx::PgPool, app_id: Uuid) -> Result<Dag, DagError> {
    let components = sqlx::query_as::<_, (Uuid,)>(
        "SELECT id FROM components WHERE application_id = $1",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await
    .map_err(|e| DagError::Database(e.to_string()))?;

    let deps = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT from_component_id, to_component_id FROM dependencies WHERE application_id = $1",
    )
    .bind(app_id)
    .fetch_all(pool)
    .await
    .map_err(|e| DagError::Database(e.to_string()))?;

    let mut dag = Dag::new();

    for (id,) in components {
        dag.add_node(id);
    }

    for (from, to) in deps {
        dag.add_edge(from, to);
    }

    // Validate no cycles
    dag.topological_levels()?;

    Ok(dag)
}

/// Validate that adding a new edge won't create a cycle.
pub async fn validate_no_cycle(
    pool: &sqlx::PgPool,
    app_id: Uuid,
    from: Uuid,
    to: Uuid,
) -> Result<(), DagError> {
    let dag = build_dag(pool, app_id).await?;

    if dag.would_create_cycle(from, to) {
        return Err(DagError::CycleDetected);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_dag_levels() {
        let mut dag = Dag::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);

        dag.add_node(a);
        dag.add_node(b);
        dag.add_node(c);
        // c depends on b, b depends on a → start order: a, b, c
        dag.add_edge(c, b);
        dag.add_edge(b, a);

        let levels = dag.topological_levels().unwrap();
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec![a]);
        assert_eq!(levels[1], vec![b]);
        assert_eq!(levels[2], vec![c]);
    }

    #[test]
    fn test_parallel_dag_levels() {
        let mut dag = Dag::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);
        let d = Uuid::from_u128(4);

        dag.add_node(a);
        dag.add_node(b);
        dag.add_node(c);
        dag.add_node(d);
        // b and c depend on a, d depends on both b and c
        dag.add_edge(b, a);
        dag.add_edge(c, a);
        dag.add_edge(d, b);
        dag.add_edge(d, c);

        let levels = dag.topological_levels().unwrap();
        assert_eq!(levels.len(), 3);
        assert_eq!(levels[0], vec![a]);
        assert!(levels[1].contains(&b) && levels[1].contains(&c));
        assert_eq!(levels[2], vec![d]);
    }

    #[test]
    fn test_cycle_detection() {
        let mut dag = Dag::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);

        dag.add_node(a);
        dag.add_node(b);
        dag.add_node(c);
        dag.add_edge(b, a);
        dag.add_edge(c, b);
        dag.add_edge(a, c); // creates cycle

        assert!(matches!(dag.topological_levels(), Err(DagError::CycleDetected)));
    }

    #[test]
    fn test_would_create_cycle() {
        let mut dag = Dag::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);

        dag.add_node(a);
        dag.add_node(b);
        dag.add_node(c);
        dag.add_edge(b, a);
        dag.add_edge(c, b);

        // a → c would create a cycle (c→b→a→c)
        assert!(dag.would_create_cycle(a, c));
        // c → a is fine (already exists via b)
        assert!(!dag.would_create_cycle(c, a));
    }

    #[test]
    fn test_no_dependencies() {
        let mut dag = Dag::new();
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);

        dag.add_node(a);
        dag.add_node(b);
        dag.add_node(c);

        let levels = dag.topological_levels().unwrap();
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0].len(), 3);
    }

    #[test]
    fn test_single_node() {
        let mut dag = Dag::new();
        let a = Uuid::from_u128(1);
        dag.add_node(a);

        let levels = dag.topological_levels().unwrap();
        assert_eq!(levels.len(), 1);
        assert_eq!(levels[0], vec![a]);
    }
}
