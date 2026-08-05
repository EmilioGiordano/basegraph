//! JSON file implementation of the [`Cache`] trait.

use std::path::PathBuf;

use crate::cache::{Cache, CacheError};
use crate::graph::Graph;

/// A cache that persists the graph as a pretty-printed JSON file.
pub struct JsonCache {
    path: PathBuf,
}

impl JsonCache {
    /// Creates a new JSON cache backed by the file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl Cache for JsonCache {
    fn save(&self, graph: &Graph) -> Result<(), CacheError> {
        let data = serde_json::to_string_pretty(graph)?;
        std::fs::write(&self.path, data)?;
        Ok(())
    }

    fn load(&self) -> Result<Graph, CacheError> {
        let data = std::fs::read_to_string(&self.path)?;
        let graph = serde_json::from_str(&data)?;
        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Confidence, Edge, EdgeKind, Node, NodeId, NodeKind};

    #[test]
    fn test_json_cache_roundtrip() {
        let mut graph = Graph::new();
        graph.add_node(Node {
            id: NodeId(1),
            kind: NodeKind::Function,
            name: "foo".into(),
            fqn: "foo".into(),
            signature: String::new(),
            file: "src/foo.rs".into(),
            line_start: 1,
            line_end: 1,
            doc: None,
        });
        graph.add_edge(Edge {
            src: NodeId(1),
            dst: NodeId(1),
            kind: EdgeKind::Calls,
            confidence: Confidence::Deterministic,
        });

        let path = std::env::temp_dir().join("codegraph_cache_roundtrip_test.json");
        let cache = JsonCache::new(&path);
        cache.save(&graph).expect("save failed");
        let loaded = cache.load().expect("load failed");

        assert_eq!(loaded.nodes().len(), graph.nodes().len());
        assert_eq!(loaded.edges().len(), graph.edges().len());

        std::fs::remove_file(&path).expect("cleanup failed");
    }
}
