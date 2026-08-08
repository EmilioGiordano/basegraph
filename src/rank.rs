//! PageRank centrality over the call graph.

use std::collections::HashMap;

use crate::graph::Graph;
use crate::model::NodeId;

const DAMPING: f64 = 0.85;
const ITERATIONS: usize = 30;

/// Deterministic PageRank score per node id (higher = more central).
pub fn pagerank(graph: &Graph) -> HashMap<NodeId, f64> {
    let ids: Vec<NodeId> = graph.nodes().iter().map(|n| n.id).collect();
    let n = ids.len();
    if n == 0 {
        return HashMap::new();
    }

    let out: HashMap<NodeId, Vec<NodeId>> =
        ids.iter().map(|id| (*id, graph.neighbors(*id))).collect();
    let base = (1.0 - DAMPING) / n as f64;
    let mut rank: HashMap<NodeId, f64> = ids.iter().map(|id| (*id, 1.0 / n as f64)).collect();

    for _ in 0..ITERATIONS {
        let mut next: HashMap<NodeId, f64> = ids.iter().map(|id| (*id, base)).collect();
        let mut dangling = 0.0;
        for id in &ids {
            let neighbors = &out[id];
            let r = rank[id];
            if neighbors.is_empty() {
                dangling += r;
            } else {
                let share = DAMPING * r / neighbors.len() as f64;
                for nb in neighbors {
                    if let Some(v) = next.get_mut(nb) {
                        *v += share;
                    }
                }
            }
        }
        let dangling_share = DAMPING * dangling / n as f64;
        for v in next.values_mut() {
            *v += dangling_share;
        }
        rank = next;
    }
    rank
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Confidence, Edge, EdgeKind, Node, NodeKind};

    fn node(id: u32) -> Node {
        Node {
            id: NodeId(id),
            kind: NodeKind::Function,
            name: format!("f{id}"),
            fqn: format!("f{id}"),
            signature: String::new(),
            sig_hash: String::new(),
            file: "x.rs".into(),
            line_start: id as usize,
            line_end: id as usize,
            doc: None,
        }
    }

    fn edge(src: u32, dst: u32) -> Edge {
        Edge {
            src: NodeId(src),
            dst: NodeId(dst),
            kind: EdgeKind::Calls,
            confidence: Confidence::Heuristic,
        }
    }

    #[test]
    fn test_more_incoming_ranks_higher() {
        let mut g = Graph::new();
        for i in 0..3 {
            g.add_node(node(i));
        }
        g.add_edge(edge(0, 2));
        g.add_edge(edge(1, 2));
        let ranks = pagerank(&g);
        assert!(ranks[&NodeId(2)] > ranks[&NodeId(1)]);
    }
}
