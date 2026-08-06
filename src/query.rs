//! Query operations over a built Graph: a project map and targeted context.

use serde::Serialize;

use crate::graph::Graph;
use crate::model::{EdgeKind, Node, NodeId, NodeKind};
use crate::tokens::TokenCounter;

/// How a symbol in a context bundle relates to the queried target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Relation {
    Target,
    Caller,
    Callee,
    Implements,
    Implementor,
    Uses,
    UsedBy,
    Colocated,
}

impl Relation {
    fn priority(self) -> u8 {
        match self {
            Relation::Target => 4,
            Relation::Caller | Relation::Callee | Relation::Implements | Relation::Implementor => 3,
            Relation::Uses | Relation::UsedBy => 2,
            Relation::Colocated => 1,
        }
    }

    fn tag(self) -> &'static str {
        match self {
            Relation::Target => "target",
            Relation::Caller => "caller",
            Relation::Callee => "callee",
            Relation::Implements => "implements",
            Relation::Implementor => "implementor",
            Relation::Uses => "uses",
            Relation::UsedBy => "used-by",
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl QueryResult {
    /// Render the bundle as compact newline-delimited text for agents / MCP.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        if let Some(note) = &self.note {
            out.push_str(note);
            out.push('\n');
        }
        for item in &self.items {
            out.push_str(&item.render());
            out.push('\n');
        }
        if self.truncated {
            out.push_str("... (truncated to fit token budget)\n");
        }
        out
    }
}

/// Render a list of items (e.g. `search` results) as newline-delimited text.
pub fn render_items(items: &[ItemView]) -> String {
    if items.is_empty() {
        return "(no matches)\n".to_string();
    }
    let mut out = String::new();
    for item in items {
        out.push_str(&item.render());
        out.push('\n');
    }
    out
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
        note: None,
    }
}

pub fn map(graph: &Graph, budget: usize, counter: &dyn TokenCounter) -> QueryResult {
    let mut nodes: Vec<&Node> = graph.nodes().iter().collect();
    nodes.sort_by(|a, b| {
        graph
            .rank_of(b.id)
            .partial_cmp(&graph.rank_of(a.id))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.file.cmp(&b.file).then(a.line_start.cmp(&b.line_start)))
    });
    let views: Vec<ItemView> = nodes.into_iter().map(ItemView::from_node).collect();
    assemble(views, budget, counter)
}

/// Find symbols whose name or fqn contains `query` (case-insensitive), ranked by
/// match quality (exact name > name substring > fqn substring) then centrality.
pub fn search(graph: &Graph, query: &str, limit: usize) -> Vec<ItemView> {
    let q = query.to_lowercase();
    let mut scored: Vec<(u8, f64, &Node)> = graph
        .nodes()
        .iter()
        .filter_map(|n| {
            let name = n.name.to_lowercase();
            let score = if name == q {
                3
            } else if name.contains(&q) {
                2
            } else if n.fqn.to_lowercase().contains(&q) {
                1
            } else {
                return None;
            };
            Some((score, graph.rank_of(n.id), n))
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.2.fqn.cmp(&b.2.fqn))
    });
    scored
        .into_iter()
        .take(limit)
        .map(|(_, _, n)| ItemView::from_node(n))
        .collect()
}

/// Return the full source of each symbol matching `target` (by name or fqn), read
/// live from its file using the node's stored line range.
pub fn show(graph: &Graph, target: &str) -> String {
    let matches: Vec<&Node> = graph
        .nodes()
        .iter()
        .filter(|n| n.fqn == target || n.name == target)
        .collect();
    if matches.is_empty() {
        return format!("no symbol named '{target}' found; try `search`\n");
    }
    let mut out = String::new();
    for n in &matches {
        out.push_str(&format!("// {} ({}:{})\n", n.fqn, n.file, n.line_start));
        match read_span(&n.file, n.line_start, n.line_end) {
            Some(src) => out.push_str(&src),
            None => out.push_str("(source unavailable)"),
        }
        out.push_str("\n\n");
    }
    out
}

fn read_span(file: &str, start: usize, end: usize) -> Option<String> {
    let content = std::fs::read_to_string(file).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let s = start.saturating_sub(1);
    let e = end.min(lines.len());
    (s < e).then(|| lines[s..e].join("\n"))
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
        for callee in graph.out_edges(m.id, EdgeKind::Calls) {
            add_relation(&mut relations, callee, Relation::Callee);
        }
        for caller in graph.in_edges(m.id, EdgeKind::Calls) {
            add_relation(&mut relations, caller, Relation::Caller);
        }
        for implemented in graph.out_edges(m.id, EdgeKind::Implements) {
            add_relation(&mut relations, implemented, Relation::Implements);
        }
        for implementor in graph.in_edges(m.id, EdgeKind::Implements) {
            add_relation(&mut relations, implementor, Relation::Implementor);
        }
        for used in graph.out_edges(m.id, EdgeKind::Uses) {
            add_relation(&mut relations, used, Relation::Uses);
        }
        for user in graph.in_edges(m.id, EdgeKind::Uses) {
            add_relation(&mut relations, user, Relation::UsedBy);
        }
    }
    let match_files: Vec<String> = matches.iter().map(|m| m.file.clone()).collect();
    for node in graph.nodes() {
        if match_files.contains(&node.file) {
            add_relation(&mut relations, node.id, Relation::Colocated);
        }
    }

    relations.sort_by(|a, b| {
        b.1.priority()
            .cmp(&a.1.priority())
            .then_with(|| {
                graph
                    .rank_of(b.0)
                    .partial_cmp(&graph.rank_of(a.0))
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

    let mut result = assemble(views, budget, counter);
    if matches.is_empty() {
        result.note = Some(format!("no symbol named '{target}' found; try `search`"));
    }
    result
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
    fn test_search_by_substring() {
        let g = sample_graph();

        let hits = search(&g, "al", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].fqn, "alpha");

        let hits2 = search(&g, "gamma", 10);
        assert_eq!(hits2[0].fqn, "gamma");
    }

    #[test]
    fn test_context_not_found() {
        let g = sample_graph();
        let counter = HeuristicCounter;

        let res = context(&g, "does_not_exist", 100_000, &counter);
        assert!(res.items.is_empty());
        assert!(res.note.is_some());
    }

    #[test]
    fn test_context_uses_labels() {
        let mut g = Graph::new();
        g.add_node(node(0, "MyType", "a.rs", 1));
        g.add_node(node(1, "user_fn", "b.rs", 1));
        g.add_edge(Edge {
            src: NodeId(1),
            dst: NodeId(0),
            kind: EdgeKind::Uses,
            confidence: Confidence::Heuristic,
        });
        let counter = HeuristicCounter;

        let res = context(&g, "MyType", 100_000, &counter);
        let user = res
            .items
            .iter()
            .find(|i| i.fqn == "user_fn")
            .expect("user present");
        assert_eq!(user.relation, Some(Relation::UsedBy));

        let res2 = context(&g, "user_fn", 100_000, &counter);
        let ty = res2
            .items
            .iter()
            .find(|i| i.fqn == "MyType")
            .expect("type present");
        assert_eq!(ty.relation, Some(Relation::Uses));
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

    #[test]
    fn test_context_implements_labels() {
        let mut g = Graph::new();
        g.add_node(node(0, "MyType", "a.rs", 1));
        g.add_node(node(1, "MyTrait", "a.rs", 10));
        g.add_edge(Edge {
            src: NodeId(0),
            dst: NodeId(1),
            kind: EdgeKind::Implements,
            confidence: Confidence::Deterministic,
        });
        let counter = HeuristicCounter;

        let res = context(&g, "MyTrait", 100_000, &counter);
        let ty = res
            .items
            .iter()
            .find(|i| i.fqn == "MyType")
            .expect("type present");
        assert_eq!(ty.relation, Some(Relation::Implementor));

        let res2 = context(&g, "MyType", 100_000, &counter);
        let tr = res2
            .items
            .iter()
            .find(|i| i.fqn == "MyTrait")
            .expect("trait present");
        assert_eq!(tr.relation, Some(Relation::Implements));
    }
}
