//! Graph builder: parses every Rust file under a directory into a [`Graph`].

use std::path::{Path, PathBuf};

use std::collections::HashMap;

use crate::graph::Graph;
use crate::model::{Confidence, Edge, EdgeKind, NodeId};
use crate::parser::rust::RustParser;
use crate::parser::LanguageParser;

/// Errors that can occur while building the graph from a directory.
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// An I/O error while walking the directory tree.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Recursively collects the paths of all `.rs` files under `root`, skipping
/// directories named `target` and hidden directories (those starting with `.`).
fn collect_rust_files(root: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if root.is_file() {
        if root.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(root.to_path_buf());
        }
        return Ok(());
    }
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect_rust_files(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
    Ok(())
}

/// Parses every Rust file under `root` and builds a [`Graph`] of the discovered
/// symbols. Files that cannot be read or parsed are skipped. Node ids are
/// reassigned to be globally sequential in a deterministic (sorted) file order.
pub fn build_graph(root: &Path) -> Result<Graph, BuildError> {
    let mut files = Vec::new();
    collect_rust_files(root, &mut files)?;
    files.sort();

    let parser = RustParser;
    let mut graph = Graph::new();
    let mut counter: u32 = 0;

    for file in &files {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let file_str = file.to_string_lossy();
        let nodes = match parser.parse_source(&content, &file_str) {
            Ok(n) => n,
            Err(_) => continue,
        };
        for mut node in nodes {
            node.id = NodeId(counter);
            counter += 1;
            graph.add_node(node);
        }
    }

    let mut by_name: HashMap<String, Vec<(NodeId, String)>> = HashMap::new();
    for node in graph.nodes() {
        by_name
            .entry(node.name.clone())
            .or_default()
            .push((node.id, node.file.clone()));
    }
    // Prefer a definition in the calling file; otherwise accept the name only if
    // it is globally unique. Ambiguous names (`map`, `new`, ...) yield no edge
    // rather than a phantom one to an unrelated definition.
    let resolve = |name: &str, file: &str| -> Option<NodeId> {
        let candidates = by_name.get(name)?;
        let mut same_file = candidates.iter().filter(|(_, f)| f.as_str() == file);
        match (same_file.next(), same_file.next()) {
            (Some((id, _)), None) => Some(*id),
            (None, _) if candidates.len() == 1 => Some(candidates[0].0),
            _ => None,
        }
    };
    for file in &files {
        let Ok(content) = std::fs::read_to_string(file) else {
            continue;
        };
        let file_str = file.to_string_lossy();
        for (caller, callee) in RustParser::parse_calls(&content) {
            if let (Some(src), Some(dst)) =
                (resolve(&caller, &file_str), resolve(&callee, &file_str))
            {
                if src != dst {
                    graph.add_edge(Edge {
                        src,
                        dst,
                        kind: EdgeKind::Calls,
                        confidence: Confidence::Heuristic,
                    });
                }
            }
        }
        for (type_name, trait_name) in RustParser::parse_impls(&content) {
            if let (Some(src), Some(dst)) = (
                resolve(&type_name, &file_str),
                resolve(&trait_name, &file_str),
            ) {
                if src != dst {
                    graph.add_edge(Edge {
                        src,
                        dst,
                        kind: EdgeKind::Implements,
                        confidence: Confidence::Heuristic,
                    });
                }
            }
        }
        for (owner, type_name) in RustParser::parse_uses(&content) {
            if let (Some(src), Some(dst)) =
                (resolve(&owner, &file_str), resolve(&type_name, &file_str))
            {
                if src != dst {
                    graph.add_edge(Edge {
                        src,
                        dst,
                        kind: EdgeKind::Uses,
                        confidence: Confidence::Heuristic,
                    });
                }
            }
        }
    }

    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_graph() {
        let dir = std::env::temp_dir().join("codegraph_builder_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(dir.join("sample.rs"), "fn a() {}\nfn b() { a(); }\n").expect("write file");

        let graph = build_graph(&dir).expect("build failed");
        assert_eq!(graph.nodes().len(), 2);
        assert!(!graph.edges().is_empty());

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn test_ambiguous_calls_are_skipped() {
        let dir = std::env::temp_dir().join("codegraph_ambiguous_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(dir.join("one.rs"), "fn shared() {}\n").expect("w1");
        std::fs::write(dir.join("two.rs"), "fn shared() {}\n").expect("w2");
        std::fs::write(
            dir.join("caller.rs"),
            "fn unique() {}\nfn go() { shared(); unique(); }\n",
        )
        .expect("w3");

        let graph = build_graph(&dir).expect("build failed");
        // Only `go -> unique` survives; the ambiguous `shared()` call is dropped.
        assert_eq!(graph.edges().len(), 1);

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn test_implements_edges() {
        let dir = std::env::temp_dir().join("codegraph_impl_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(
            dir.join("m.rs"),
            "trait Greet { fn hi(&self); }\nstruct Robot;\nimpl Greet for Robot { fn hi(&self) {} }\n",
        )
        .expect("w");

        let graph = build_graph(&dir).expect("build failed");
        assert!(
            graph.edges().iter().any(|e| e.kind == EdgeKind::Implements),
            "expected an Implements edge"
        );

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn test_same_file_ambiguity_is_skipped() {
        let dir = std::env::temp_dir().join("codegraph_samefile_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create dir");
        std::fs::write(
            dir.join("s.rs"),
            "struct A;\nstruct B;\nimpl A { fn make() {} }\nimpl B { fn make() {} }\nfn go() { A::make(); }\n",
        )
        .expect("w");

        let graph = build_graph(&dir).expect("build failed");
        // `make` is defined twice in the same file, so the call cannot resolve.
        let calls = graph
            .edges()
            .iter()
            .filter(|e| e.kind == EdgeKind::Calls)
            .count();
        assert_eq!(calls, 0);

        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
