//! Domain model for CodeGraph.

use serde::{Deserialize, Serialize};
use std::hash::Hash;

/// A unique identifier for a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

/// The kind of a node in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Module,
    Struct,
    Enum,
    Trait,
    Function,
    Method,
    Const,
}

/// The kind of an edge in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    Defines,
    Uses,
    Calls,
    Implements,
    References,
}

/// The confidence level of an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    Deterministic,
    Heuristic,
}

/// A node in the code graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub name: String,
    pub fqn: String,
    pub signature: String,
    pub file: String,
    pub line_start: usize,
    pub line_end: usize,
    pub doc: Option<String>,
}

/// An edge between two nodes in the code graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edge {
    pub src: NodeId,
    pub dst: NodeId,
    pub kind: EdgeKind,
    pub confidence: Confidence,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn test_node_serde_roundtrip() {
        let node = Node {
            id: NodeId(1),
            kind: NodeKind::Struct,
            name: "MyStruct".to_string(),
            fqn: "crate::MyStruct".to_string(),
            signature: "".to_string(),
            file: "src/lib.rs".to_string(),
            line_start: 10,
            line_end: 20,
            doc: Some("Example struct".to_string()),
        };
        let json = serde_json::to_string(&node).unwrap();
        let deserialized: Node = serde_json::from_str(&json).unwrap();
        assert_eq!(node, deserialized);
    }

    #[test]
    fn test_edge_construction() {
        let edge = Edge {
            src: NodeId(1),
            dst: NodeId(2),
            kind: EdgeKind::Uses,
            confidence: Confidence::Deterministic,
        };
        assert_eq!(edge.kind, EdgeKind::Uses);
    }
}
