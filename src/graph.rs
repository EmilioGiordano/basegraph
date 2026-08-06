//! Graph structure and basic operations.

use std::collections::HashMap;

use crate::model::{Edge, EdgeKind, Node, NodeId};

/// A directed graph of code elements (nodes) and their relationships (edges).
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct Graph {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    #[serde(default)]
    ranks: Vec<f64>,
}

impl Graph {
    /// Creates a new, empty graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node to the graph.
    pub fn add_node(&mut self, node: Node) {
        self.nodes.push(node);
    }

    /// Adds an edge to the graph.
    pub fn add_edge(&mut self, edge: Edge) {
        self.edges.push(edge);
    }

    /// Returns a slice of all nodes.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Returns a slice of all edges.
    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    /// Looks up a node by its id.
    pub fn node_by_id(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Returns the ids of all nodes directly reachable from the given node.
    pub fn neighbors(&self, id: NodeId) -> Vec<NodeId> {
        self.edges
            .iter()
            .filter(|e| e.src == id)
            .map(|e| e.dst)
            .collect()
    }

    /// Reverse of `neighbors`: ids of all nodes that reach the given node.
    pub fn callers(&self, id: NodeId) -> Vec<NodeId> {
        self.edges
            .iter()
            .filter(|e| e.dst == id)
            .map(|e| e.src)
            .collect()
    }

    /// Ids reachable from `id` via an edge of the given kind.
    pub fn out_edges(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.edges
            .iter()
            .filter(|e| e.src == id && e.kind == kind)
            .map(|e| e.dst)
            .collect()
    }

    /// Ids that reach `id` via an edge of the given kind.
    pub fn in_edges(&self, id: NodeId, kind: EdgeKind) -> Vec<NodeId> {
        self.edges
            .iter()
            .filter(|e| e.dst == id && e.kind == kind)
            .map(|e| e.src)
            .collect()
    }

    /// Store precomputed PageRank scores (indexed by node id) so queries look up
    /// centrality in O(1) instead of recomputing the whole ranking each call.
    pub fn set_ranks(&mut self, ranks: &HashMap<NodeId, f64>) {
        let len = self
            .nodes
            .iter()
            .map(|n| n.id.0 as usize + 1)
            .max()
            .unwrap_or(0);
        let mut scores = vec![0.0; len];
        for (id, rank) in ranks {
            if let Some(slot) = scores.get_mut(id.0 as usize) {
                *slot = *rank;
            }
        }
        self.ranks = scores;
    }

    /// PageRank score for a node id, or 0.0 if the graph was never ranked.
    pub fn rank_of(&self, id: NodeId) -> f64 {
        self.ranks.get(id.0 as usize).copied().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Confidence, EdgeKind, NodeKind};

    fn sample_node(id: u32, name: &str) -> Node {
        Node {
            id: NodeId(id),
            kind: NodeKind::Function,
            name: name.into(),
            fqn: name.into(),
            signature: String::new(),
            file: "src/lib.rs".into(),
            line_start: 1,
            line_end: 1,
            doc: None,
        }
    }

    #[test]
    fn test_graph_operations() {
        let mut graph = Graph::new();
        graph.add_node(sample_node(1, "foo"));
        graph.add_node(sample_node(2, "bar"));
        graph.add_edge(Edge {
            src: NodeId(1),
            dst: NodeId(2),
            kind: EdgeKind::Calls,
            confidence: Confidence::Deterministic,
        });

        assert_eq!(graph.nodes().len(), 2);
        assert_eq!(graph.edges().len(), 1);
        assert_eq!(graph.node_by_id(NodeId(1)).expect("node 1").name, "foo");
        assert_eq!(graph.neighbors(NodeId(1)), vec![NodeId(2)]);
    }
}
