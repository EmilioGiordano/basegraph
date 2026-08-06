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

/// Current on-disk cache format version. Bump when the graph schema changes.
const CURRENT_VERSION: u32 = 2;

#[derive(serde::Serialize)]
struct CacheEnvelopeRef<'a> {
    version: u32,
    graph: &'a Graph,
}

impl Cache for JsonCache {
    fn save(&self, graph: &Graph) -> Result<(), CacheError> {
        let envelope = CacheEnvelopeRef {
            version: CURRENT_VERSION,
            graph,
        };
        let data = serde_json::to_string_pretty(&envelope)?;
        std::fs::write(&self.path, data)?;
        Ok(())
    }

    fn load(&self) -> Result<Graph, CacheError> {
        let data = std::fs::read_to_string(&self.path)?;
        let mut value: serde_json::Value = serde_json::from_str(&data)?;
        let version = value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32;
        if version != CURRENT_VERSION {
            return Err(CacheError::Incompatible {
                found: version,
                expected: CURRENT_VERSION,
            });
        }
        let graph_value = value
            .get_mut("graph")
            .map(serde_json::Value::take)
            .unwrap_or(serde_json::Value::Null);
        let graph = serde_json::from_value(graph_value)?;
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

    #[test]
    fn test_rejects_incompatible_cache() {
        let path = std::env::temp_dir().join("codegraph_cache_incompat_test.json");
        std::fs::write(&path, r#"{"nodes":[],"edges":[]}"#).expect("write");

        let err = JsonCache::new(&path)
            .load()
            .expect_err("should reject old format");
        assert!(matches!(err, CacheError::Incompatible { .. }));

        std::fs::remove_file(&path).ok();
    }
}
