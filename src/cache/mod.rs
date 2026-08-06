//! Cache abstraction for persisting and loading the graph.

pub mod json;

pub use json::JsonCache;

use crate::graph::Graph;

/// Trait for graph cache implementations (persist and load a [`Graph`]).
pub trait Cache {
    /// Saves the given graph to the cache.
    fn save(&self, graph: &Graph) -> Result<(), CacheError>;

    /// Loads a graph from the cache.
    fn load(&self) -> Result<Graph, CacheError>;
}

/// Errors that can occur while using a cache.
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    /// An underlying I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A (de)serialization error.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    /// The cache was written by an incompatible format version.
    #[error("cache format version {found}, expected {expected}; rebuild with `codegraph build`")]
    Incompatible { found: u32, expected: u32 },
}
