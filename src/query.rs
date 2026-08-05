//! Query operations over a built [`Graph`]: a project map and targeted context.

use serde::Serialize;

use crate::graph::Graph;
use crate::model::{Node, NodeId, NodeKind};
use crate::tokens::TokenCounter;

/// A compact, body-free view of a single symbol, suitable for an LLM agent.
#[derive(Debug, Clone, Serialize)]
pub struct ItemView {
    /// Fully-qualified name of the symbol.
    pub fqn: String,
    /// The kind of symbol.
    pub kind: NodeKind,
    /// The declaration/signature (without body).
    pub signature: String,
    /// Source file the symbol comes from.
    pub file: String,
    /// First source line of the symbol.
    pub line_start: usize,
    /// Last source line of the symbol.
    pub line_end: usize,
}

impl ItemView {
    fn from_node(node: &Node) -> Self {
        Self {
            fqn: node.fqn.clone(),
            kind: node.kind,
            signature: node.signature.clone(),
            file: node.file.clone(),
            line_start: node.line_start,
            line_end: node.line_end,
        }
    }

    /// Render this item as a single compact line of text.
    fn render(&self) -> String {
        format!(
            "[{:?}] {} :: {}  ({}:{})",
            self.kind, self.fqn, self.signature, self.file, self.line_start
        )
    }
}

/// A report on how many tokens the returned bundle saved versus reading the
/// full source of the included symbols.
#[derive(Debug, Clone, Serialize)]
pub struct TokenReport {
    /// Tokens used by the returned bundle.
    pub bundle_tokens: usize,
    /// Estimated tokens of the full source of the included symbols.
    pub full_source_tokens: usize,
    /// `full_source_tokens / bundle_tokens` (0.0 when the bundle is empty).
    pub savings_ratio: f64,
}

/// The result of a query: the selected items plus a token report.
#[derive(Debug, Clone, Serialize)]
pub struct QueryResult {
    /// The selected symbol views, in order.
    pub items: Vec<ItemView>,
    /// Whether some items were dropped to respect the token budget.
    pub truncated: bool,
    /// Token savings report for this bundle.
    pub token_report: TokenReport,
}

// Tokens an agent would spend reading the full source of every file the bundle
// touches. Falls back to a per-line estimate when a file cannot be read.
fn estimate_full_tokens(views: &[ItemView], counter: &dyn TokenCounter) -> usize {
    let mut files: Vec<&str> = views.iter().map(|v| v.file.as_str()).collect();
    files.sort_unstable();
    files.dedup();
    files
        .iter()
        .map(|f| match std::fs::read_to_string(f) {
            Ok(content) => counter.count(&content),
            Err(_) => views
                .iter()
                .filter(|v| v.file == *f)
                .map(|v| (v.line_end.saturating_sub(v.line_start) + 1) * 8)
                .sum(),
        })
        .sum()
}

/// Push a node id into `ids` only if it is not already present.
fn push_unique(ids: &mut Vec<NodeId>, id: NodeId) {
    if !ids.contains(&id) {
        ids.push(id);
    }
}

/// Assemble a [`QueryResult`] from candidate views, honoring the token budget.
/// The first candidate is always included so a result is never empty when
/// candidates exist.
fn assemble(views: Vec<ItemView>, budget: usize, counter: &dyn TokenCounter) -> QueryResult {
    let mut selected: Vec<ItemView> = Vec::new();
    let mut bundle_tokens = 0usize;
    let mut truncated = false;

    for view in views {
        let cost = counter.count(&view.render());
        if !selected.is_empty() && bundle_tokens + cost > budget {
            truncated = true;
            break;
        }
        bundle_tokens += cost;
        selected.push(view);
    }

    let full_source_tokens = estimate_full_tokens(&selected, counter);
    let savings_ratio = if bundle_tokens == 0 {
        0.0
    } else {
        full_source_tokens as f64 / bundle_tokens as f64
    };

    QueryResult {
        items: selected,
        truncated,
        token_report: TokenReport {
            bundle_tokens,
            full_source_tokens,
            savings_ratio,
        },
    }
}

/// Produce a compressed, project-wide map: every symbol's signature (no bodies),
/// ordered by file and line, truncated to fit `budget` tokens.
pub fn map(graph: &Graph, budget: usize, counter: &dyn TokenCounter) -> QueryResult {
    let mut views: Vec<ItemView> = graph.nodes().iter().map(ItemView::from_node).collect();
    views.sort_by(|a, b| a.file.cmp(&b.file).then(a.line_start.cmp(&b.line_start)));
    assemble(views, budget, counter)
}

/// Produce a targeted context bundle for the symbol(s) matching `target` (by
/// name or fully-qualified name), including their graph neighbours and the
/// symbols co-located in the same file, truncated to fit `budget` tokens.
pub fn context(
    graph: &Graph,
    target: &str,
    budget: usize,
    counter: &dyn TokenCounter,
) -> QueryResult {
    let matches: Vec<&Node> = graph
        .nodes()
        .iter()
        .filter(|n| n.fqn == target || n.name == target)
        .collect();

    let mut selected_ids: Vec<NodeId> = Vec::new();
    for m in &matches {
        push_unique(&mut selected_ids, m.id);
        for neighbour in graph.neighbors(m.id) {
            push_unique(&mut selected_ids, neighbour);
        }
    }

    let match_files: Vec<String> = matches.iter().map(|m| m.file.clone()).collect();
    for node in graph.nodes() {
        if match_files.contains(&node.file) {
            push_unique(&mut selected_ids, node.id);
        }
    }

    let mut views: Vec<ItemView> = selected_ids
        .iter()
        .filter_map(|id| graph.node_by_id(*id))
        .map(ItemView::from_node)
        .collect();
    views.sort_by(|a, b| a.file.cmp(&b.file).then(a.line_start.cmp(&b.line_start)));
    assemble(views, budget, counter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Confidence, Edge, EdgeKind, Node, NodeId, NodeKind};
    use crate::tokens::HeuristicCounter;

    fn node(id: u32, name: &str, file: &str, line: usize) -> Node {
        Node {
            id: NodeId(id),
            kind: NodeKind::Function,
            name: name.into(),
            fqn: name.into(),
            signature: format!("fn {name}()"),
            file: file.into(),
            line_start: line,
            line_end: line + 2,
            doc: None,
        }
    }

    fn sample_graph() -> Graph {
        let mut g = Graph::new();
        g.add_node(node(0, "alpha", "a.rs", 1));
        g.add_node(node(1, "beta", "a.rs", 5));
        g.add_node(node(2, "gamma", "b.rs", 1));
        g.add_edge(Edge {
            src: NodeId(0),
            dst: NodeId(2),
            kind: EdgeKind::Calls,
            confidence: Confidence::Deterministic,
        });
        g
    }

    #[test]
    fn test_map_respects_budget() {
        let g = sample_graph();
        let counter = HeuristicCounter;

        let full = map(&g, 100_000, &counter);
        assert_eq!(full.items.len(), 3);
        assert!(!full.truncated);
        assert!(full.token_report.savings_ratio > 0.0);

        let tiny = map(&g, 1, &counter);
        assert!(tiny.truncated);
        assert_eq!(tiny.items.len(), 1);
    }

    #[test]
    fn test_context_includes_neighbour_and_colocated() {
        let g = sample_graph();
        let counter = HeuristicCounter;

        let res = context(&g, "alpha", 100_000, &counter);
        let names: Vec<&str> = res.items.iter().map(|i| i.fqn.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"gamma"));
        assert!(names.contains(&"beta"));
    }
}
