//! Query operations over a built Graph: a project map and targeted context.

use serde::Serialize;

use std::collections::HashMap;

use crate::graph::Graph;
use crate::model::{Node, NodeId, NodeKind};
use crate::rank;
use crate::tokens::TokenCounter;

fn rank_of(ranks: &HashMap<NodeId, f64>, id: NodeId) -> f64 {
    ranks.get(&id).copied().unwrap_or(0.0)
}

/// How a symbol in a context bundle relates to the queried target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Relation {
    Target,
    Caller,
    Callee,
    Colocated,
}

impl Relation {
    fn priority(self) -> u8 {
        match self {
            Relation::Target => 3,
            Relation::Caller | Relation::Callee => 2,
            Relation::Colocated => 1,
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Relation::Target => "target",
            Relation::Caller => "caller",
            Relation::Callee => "callee",
            Relation::Colocated => "co-located",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemView {
    pub fqn: String,
    pub kind: NodeKind,
    pub signature: String,
    pub file: String,
    pub line_start: usize,
    pub line_end: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation: Option<Relation>,
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
            relation: None,
        }
    }

    /// Compact single-line rendering used for token counting and text output.
    pub fn render(&self) -> String {
        let prefix = match self.relation {
            Some(rel) => format!("[{}] ", rel.tag()),
            None => String::new(),
        };
        format!(
            "{prefix}[{:?}] {} :: {}  ({}:{})",
            self.kind, self.fqn, self.signature, self.file, self.line_start
        )
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TokenReport {
    pub bundle_tokens: usize,
    pub full_source_tokens: usize,
    pub savings_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct QueryResult {
    pub items: Vec<ItemView>,
    pub truncated: bool,
    pub token_report: TokenReport,
}

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

pub fn map(graph: &Graph, budget: usize, counter: &dyn TokenCounter) -> QueryResult {
    let ranks = rank::pagerank(graph);
    let mut nodes: Vec<&Node> = graph.nodes().iter().collect();
    nodes.sort_by(|a, b| {
        rank_of(&ranks, b.id)
            .partial_cmp(&rank_of(&ranks, a.id))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file.cmp(&b.file).then(a.line_start.cmp(&b.line_start)))
    });
    let views: Vec<ItemView> = nodes.into_iter().map(ItemView::from_node).collect();
    assemble(views, budget, counter)
}

fn add_relation(acc: &mut Vec<(NodeId, Relation)>, id: NodeId, rel: Relation) {
    if let Some(entry) = acc.iter_mut().find(|(eid, _)| *eid == id) {
        if rel.priority() > entry.1.priority() {
            entry.1 = rel;
        }
    } else {
        acc.push((id, rel));
    }
}

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

    let mut relations: Vec<(NodeId, Relation)> = Vec::new();
    for m in &matches {
        add_relation(&mut relations, m.id, Relation::Target);
    }
    for m in &matches {
        for callee in graph.neighbors(m.id) {
            add_relation(&mut relations, callee, Relation::Callee);
        }
        for caller in graph.callers(m.id) {
            add_relation(&mut relations, caller, Relation::Caller);
        }
    }
    let match_files: Vec<String> = matches.iter().map(|m| m.file.clone()).collect();
    for node in graph.nodes() {
        if match_files.contains(&node.file) {
            add_relation(&mut relations, node.id, Relation::Colocated);
        }
    }

    let ranks = rank::pagerank(graph);
    relations.sort_by(|a, b| {
        b.1.priority()
            .cmp(&a.1.priority())
            .then_with(|| {
                rank_of(&ranks, b.0)
                    .partial_cmp(&rank_of(&ranks, a.0))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| match (graph.node_by_id(a.0), graph.node_by_id(b.0)) {
                (Some(x), Some(y)) => x.file.cmp(&y.file).then(x.line_start.cmp(&y.line_start)),
                _ => std::cmp::Ordering::Equal,
            })
    });

    let views: Vec<ItemView> = relations
        .iter()
        .filter_map(|(id, rel)| {
            graph.node_by_id(*id).map(|n| {
                let mut v = ItemView::from_node(n);
                v.relation = Some(*rel);
                v
            })
        })
        .collect();

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
    fn test_context_labels() {
        let g = sample_graph();
        let counter = HeuristicCounter;

        let res = context(&g, "alpha", 100_000, &counter);
        let rel = |fqn: &str| {
            res.items
                .iter()
                .find(|i| i.fqn == fqn)
                .and_then(|i| i.relation)
        };

        assert_eq!(rel("alpha"), Some(Relation::Target));
        assert_eq!(rel("gamma"), Some(Relation::Callee));
        assert_eq!(rel("beta"), Some(Relation::Colocated));
    }

    #[test]
    fn test_context_reverse_lookup() {
        let g = sample_graph();
        let counter = HeuristicCounter;

        // alpha calls gamma, so a query on gamma must surface alpha as a caller.
        let res = context(&g, "gamma", 100_000, &counter);
        let alpha = res
            .items
            .iter()
            .find(|i| i.fqn == "alpha")
            .expect("alpha present in gamma context");
        assert_eq!(alpha.relation, Some(Relation::Caller));
    }
}
