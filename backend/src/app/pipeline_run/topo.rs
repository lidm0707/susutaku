//! Kahn topological ordering over the validated acyclic spec.

use std::collections::VecDeque;

use piplines::graph::{NodeDef, PipelineSpec};

/// Falls back to spec order if the graph is unexpectedly cyclic.
pub fn topo_order(spec: &PipelineSpec) -> Vec<&NodeDef> {
    let idx = |id: &str| spec.nodes.iter().position(|n| n.id == id);
    let mut indegree = vec![0usize; spec.nodes.len()];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); spec.nodes.len()];
    for link in &spec.links {
        if let (Some(from), Some(to)) = (idx(&link.from), idx(&link.to)) {
            adj[from].push(to);
            indegree[to] += 1;
        }
    }
    let mut queue: VecDeque<usize> = (0..spec.nodes.len())
        .filter(|&i| indegree[i] == 0)
        .collect();
    let mut ordered: Vec<&NodeDef> = Vec::with_capacity(spec.nodes.len());
    while let Some(i) = queue.pop_front() {
        ordered.push(&spec.nodes[i]);
        for &next in &adj[i] {
            indegree[next] -= 1;
            if indegree[next] == 0 {
                queue.push_back(next);
            }
        }
    }
    if ordered.len() == spec.nodes.len() {
        ordered
    } else {
        spec.nodes.iter().collect()
    }
}
