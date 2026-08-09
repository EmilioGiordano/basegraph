//! Minimal stdio JSON-RPC server implementing the MCP protocol (2025-11-25).
//!
//! Exposes `map`, `context`, `search` and `show` as MCP tools over one codebase
//! so any MCP client (Claude Code, Cursor, ...) can query the graph natively.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::builder::build_graph;
use crate::cache::{Cache, JsonCache};
use crate::graph::Graph;
use crate::memory::anchor::{classify, Classification, ReanchorBasis};
use crate::memory::model::{AnchorKey, Kind, Memory, MemoryId, Provenance, Scope, Status};
use crate::memory::store::{Event, MemoryStore};
use crate::query;
use crate::tokens::HeuristicCounter;

const PROTOCOL_VERSION: &str = "2025-11-25";
const DEFAULT_BUDGET: usize = 4000;
const DEFAULT_SEARCH_LIMIT: usize = 20;

/// Runs the MCP server for the codebase at `dir`, reading JSON-RPC messages from
/// stdin and writing responses to stdout until the input stream closes.
pub fn serve(dir: PathBuf) -> Result<()> {
    let cache_path = dir.join("codegraph.json");
    let mut state = ServerState::load(&dir, &cache_path)?;
    eprintln!(
        "codegraph mcp: serving {} ({} nodes, {} edges)",
        dir.display(),
        state.graph.nodes().len(),
        state.graph.edges().len()
    );

    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                eprintln!("codegraph mcp: stdin read error: {e}");
                continue;
            }
        };
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(trimmed) {
            Ok(msg) => state.handle(&msg),
            Err(e) => Some(error_response(
                Value::Null,
                -32700,
                &format!("parse error: {e}"),
            )),
        };
        if let Some(response) = response {
            if let Err(e) = write_message(&mut out, &response) {
                eprintln!("codegraph mcp: write error: {e}");
            }
        }
    }
    Ok(())
}

struct ServerState {
    cache_path: PathBuf,
    graph: Graph,
    mtime: Option<SystemTime>,
    counter: HeuristicCounter,
    memory_store: MemoryStore,
}

impl ServerState {
    fn load(dir: &Path, cache_path: &Path) -> Result<Self> {
        let graph = if cache_path.exists() {
            JsonCache::new(cache_path).load().context("loading cache")?
        } else {
            let g = build_graph(dir).context("building graph")?;
            JsonCache::new(cache_path)
                .save(&g)
                .context("saving cache")?;
            g
        };
        Ok(Self {
            mtime: file_mtime(cache_path),
            cache_path: cache_path.to_path_buf(),
            graph,
            counter: HeuristicCounter,
            memory_store: MemoryStore::new(dir),
        })
    }

    /// Reload the graph if the cache file changed since we last read it (e.g. the
    /// agent rebuilt it while the server was running). A failed read (such as a
    /// half-written file) keeps the previous graph and is retried on the next call.
    fn reload_if_changed(&mut self) {
        let current = file_mtime(&self.cache_path);
        if current == self.mtime {
            return;
        }
        match JsonCache::new(&self.cache_path).load() {
            Ok(graph) => {
                self.graph = graph;
                self.mtime = current;
                eprintln!(
                    "codegraph mcp: reloaded graph ({} nodes)",
                    self.graph.nodes().len()
                );
            }
            Err(e) => {
                eprintln!("codegraph mcp: reload skipped, serving previous graph ({e})");
            }
        }
    }

    fn handle(&mut self, msg: &Value) -> Option<Value> {
        let method = msg
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        // Requests carry an `id`; notifications do not and get no response.
        let id = msg.get("id").cloned();
        match method {
            "initialize" => id.map(|id| self.initialize(id, msg)),
            "tools/list" => id.map(|id| success(id, self.tools_list())),
            "tools/call" => id.map(|id| self.tools_call(id, msg)),
            "ping" => id.map(|id| success(id, json!({}))),
            "notifications/initialized" => None,
            _ => id.map(|id| error_response(id, -32601, &format!("method not found: {method}"))),
        }
    }

    fn initialize(&self, id: Value, msg: &Value) -> Value {
        let version = msg
            .get("params")
            .and_then(|p| p.get("protocolVersion"))
            .and_then(Value::as_str)
            .unwrap_or(PROTOCOL_VERSION)
            .to_string();
        success(
            id,
            json!({
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "codegraph", "version": env!("CARGO_PKG_VERSION") },
                "instructions": "Query a codebase's structure cheaply: `map` to orient, `search` to find a symbol's name, `context` to see its callers/callees/impls/uses, and `show` to read a symbol's source. Use `recall` to fetch what's known about a file or symbol — each memory is tagged with how fresh its anchor is against the current code — and `remember` to record a decision, gotcha, invariant, or past bug anchored to a symbol."
            }),
        )
    }

    fn tools_list(&self) -> Value {
        json!({
            "tools": [
                {
                    "name": "map",
                    "description": "Project-wide map of the most central symbols (PageRank-ranked), capped at a token budget. Use first to orient in an unfamiliar codebase.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "budget": { "type": "integer", "description": "Approximate token budget (default 4000)" }
                        },
                        "additionalProperties": false
                    }
                },
                {
                    "name": "context",
                    "description": "Relevant neighborhood of a symbol, each line labeled: callers, callees, implemented traits, implementors, used and used-by types, and co-located symbols. Use to understand a symbol or assess change impact.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "symbol": { "type": "string", "description": "Symbol name or fully-qualified name" },
                            "budget": { "type": "integer", "description": "Approximate token budget (default 4000)" }
                        },
                        "required": ["symbol"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "search",
                    "description": "Find symbols by name or fully-qualified name (case-insensitive substring), ranked by match quality and centrality. Use to locate a symbol's exact name before calling context.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": { "type": "string", "description": "Substring to match in symbol names" },
                            "limit": { "type": "integer", "description": "Max results (default 20)" }
                        },
                        "required": ["query"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "show",
                    "description": "Read a symbol's source, live from the file. Default is a preview capped at 200 lines; pass one of full/range/grep/outline to control what you get. Use after `context` to read a body and act on it.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "symbol": { "type": "string", "description": "Symbol name or fully-qualified name" },
                            "full": { "type": "boolean", "description": "Return the entire body" },
                            "range": { "type": "string", "description": "Absolute file lines 'X:Y' (or 'X:' to the end)" },
                            "grep": { "type": "string", "description": "Return only lines matching this substring, with context" },
                            "outline": { "type": "boolean", "description": "Return a skeleton: signature plus control-flow headers and match arms" }
                        },
                        "required": ["symbol"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "recall",
                    "description": "Retrieve stored memories (decisions, gotchas, invariants, past bugs) about a file or symbol. Each memory is annotated with its freshness against the current index: intact, evolved (interface changed), or orphaned (anchor gone, with any uncertain re-anchor candidates). Use before changing a symbol to learn what past work recorded about it.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "target": { "type": "string", "description": "A file path or a symbol's fully-qualified name" }
                        },
                        "required": ["target"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "remember",
                    "description": "Record a memory (decision, gotcha, invariant, or past bug) anchored to a symbol. The anchor must be an existing symbol's fully-qualified name; its current signature is snapshotted so later recalls can tell whether the code has since changed.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "anchor": { "type": "string", "description": "Fully-qualified name of an existing symbol to anchor to" },
                            "kind": { "type": "string", "description": "One of: decision, gotcha, invariant, bughistory" },
                            "content": { "type": "string", "description": "The knowledge to store" },
                            "commit": { "type": "string", "description": "Optional provenance: commit this was learned at" },
                            "session": { "type": "string", "description": "Optional provenance: session id" }
                        },
                        "required": ["anchor", "kind", "content"],
                        "additionalProperties": false
                    }
                }
            ]
        })
    }

    fn tools_call(&mut self, id: Value, msg: &Value) -> Value {
        self.reload_if_changed();
        let params = msg.get("params");
        let name = params
            .and_then(|p| p.get("name"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = params
            .and_then(|p| p.get("arguments"))
            .cloned()
            .unwrap_or_else(|| json!({}));

        // Run the query inside catch_unwind so a bug on an untested graph becomes a
        // recoverable tool error rather than killing this long-lived server.
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run_tool(name, &args)));
        match outcome {
            Ok(Ok(text)) => content_result(id, &text, false),
            Ok(Err(ToolError::Unknown(tool))) => {
                error_response(id, -32602, &format!("unknown tool: {tool}"))
            }
            Ok(Err(ToolError::BadArg(message))) => content_result(id, &message, true),
            Err(_) => content_result(id, "internal error: the query panicked", true),
        }
    }

    fn run_tool(&self, name: &str, args: &Value) -> Result<String, ToolError> {
        match name {
            "map" => {
                let budget = arg_usize(args, "budget", DEFAULT_BUDGET);
                Ok(query::map(&self.graph, budget, &self.counter).to_text())
            }
            "context" => {
                let symbol = args
                    .get("symbol")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::BadArg("missing required argument: symbol".into()))?;
                let budget = arg_usize(args, "budget", DEFAULT_BUDGET);
                Ok(query::context(&self.graph, symbol, budget, &self.counter).to_text())
            }
            "search" => {
                let query = args
                    .get("query")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::BadArg("missing required argument: query".into()))?;
                let limit = arg_usize(args, "limit", DEFAULT_SEARCH_LIMIT);
                Ok(query::render_items(&query::search(
                    &self.graph,
                    query,
                    limit,
                )))
            }
            "show" => {
                let symbol = args
                    .get("symbol")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::BadArg("missing required argument: symbol".into()))?;
                Ok(query::show(&self.graph, symbol, &show_mode(args), true))
            }
            "recall" => {
                let target = args
                    .get("target")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::BadArg("missing required argument: target".into()))?;
                let memories = self
                    .memory_store
                    .materialize()
                    .map_err(|e| ToolError::BadArg(format!("reading memory log: {e}")))?;
                let views: Vec<Value> = memories
                    .iter()
                    .filter(|m| scope_matches(&m.scope, target))
                    .map(|m| memory_view(m, &self.graph))
                    .collect();
                let out = json!({ "target": target, "count": views.len(), "memories": views });
                Ok(serde_json::to_string_pretty(&out).unwrap_or_else(|_| out.to_string()))
            }
            "remember" => {
                let fqn = args
                    .get("anchor")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::BadArg("missing required argument: anchor".into()))?;
                let kind_str = args
                    .get("kind")
                    .and_then(Value::as_str)
                    .ok_or_else(|| ToolError::BadArg("missing required argument: kind".into()))?;
                let content = args.get("content").and_then(Value::as_str).ok_or_else(|| {
                    ToolError::BadArg("missing required argument: content".into())
                })?;
                let kind = parse_kind(kind_str).ok_or_else(|| {
                    ToolError::BadArg(format!(
                        "unknown kind '{kind_str}' (expected decision, gotcha, invariant, or bughistory)"
                    ))
                })?;
                let node = self
                    .graph
                    .nodes()
                    .iter()
                    .find(|n| n.fqn == fqn)
                    .ok_or_else(|| {
                        ToolError::BadArg(format!(
                            "no symbol '{fqn}' in the current index; the anchor must exist"
                        ))
                    })?;
                let anchor = AnchorKey {
                    fqn: node.fqn.clone(),
                    sig_hash: node.sig_hash.clone(),
                };
                let count = self
                    .memory_store
                    .load_events()
                    .map_err(|e| ToolError::BadArg(format!("reading memory log: {e}")))?
                    .len();
                let id = MemoryId(format!("mem-{count}"));
                let provenance = Provenance {
                    commit: args
                        .get("commit")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    session: args
                        .get("session")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                };
                let memory = Memory {
                    id: id.clone(),
                    content: content.to_string(),
                    anchor,
                    scope: Scope::Symbol(fqn.to_string()),
                    kind,
                    status: Status::Intact,
                    provenance,
                };
                self.memory_store
                    .append(&Event::Created { memory })
                    .map_err(|e| ToolError::BadArg(format!("writing memory log: {e}")))?;
                Ok(format!("remembered {} anchored to {fqn}", id.0))
            }
            other => Err(ToolError::Unknown(other.to_string())),
        }
    }
}

enum ToolError {
    Unknown(String),
    BadArg(String),
}

fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn content_result(id: Value, text: &str, is_error: bool) -> Value {
    success(
        id,
        json!({ "content": [ { "type": "text", "text": text } ], "isError": is_error }),
    )
}

fn arg_usize(args: &Value, key: &str, default: usize) -> usize {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(default)
}

fn parse_kind(s: &str) -> Option<Kind> {
    match s.to_ascii_lowercase().as_str() {
        "decision" => Some(Kind::Decision),
        "gotcha" => Some(Kind::Gotcha),
        "invariant" => Some(Kind::Invariant),
        "bughistory" | "bug_history" | "bug-history" => Some(Kind::BugHistory),
        _ => None,
    }
}

fn scope_matches(scope: &Scope, target: &str) -> bool {
    match scope {
        Scope::File(p) => p == target,
        Scope::Symbol(s) => s == target,
    }
}

fn scope_label(scope: &Scope) -> String {
    match scope {
        Scope::File(p) => format!("file {p}"),
        Scope::Symbol(s) => format!("symbol {s}"),
    }
}

fn status_label(classification: &Classification) -> &'static str {
    match classification {
        Classification::Intact => "intact",
        Classification::Evolved => "evolved",
        Classification::ReanchorCandidate { .. } | Classification::Orphaned => "orphaned",
    }
}

fn basis_label(basis: &ReanchorBasis) -> &'static str {
    match basis {
        ReanchorBasis::SigHash => "same signature hash",
        ReanchorBasis::TokenSimilarity => "similar name",
    }
}

/// Render one memory as JSON, always tagged with its freshness against the
/// current index so a memory is never served without saying how stale it is.
fn memory_view(memory: &Memory, graph: &Graph) -> Value {
    let classification = classify(&memory.anchor, graph);
    let mut view = json!({
        "id": memory.id.0,
        "kind": format!("{:?}", memory.kind),
        "scope": scope_label(&memory.scope),
        "content": memory.content,
        "anchor": { "fqn": memory.anchor.fqn, "sig_hash": memory.anchor.sig_hash },
        "status": status_label(&classification),
        "uncertain": classification.is_uncertain(),
        "provenance": {
            "commit": memory.provenance.commit,
            "session": memory.provenance.session,
        },
    });
    if let Classification::ReanchorCandidate { candidates, basis } = &classification {
        view["reanchor_candidates"] = json!(candidates);
        view["reanchor_basis"] = json!(basis_label(basis));
    }
    view
}

fn show_mode(args: &Value) -> query::ShowMode {
    if args
        .get("outline")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        query::ShowMode::Outline
    } else if let Some(pattern) = args.get("grep").and_then(Value::as_str) {
        query::ShowMode::Grep(pattern.to_string())
    } else if let Some(r) = args.get("range").and_then(Value::as_str) {
        let (a, b) = r.split_once(':').unwrap_or((r, ""));
        match a.trim().parse::<usize>() {
            Ok(start) => query::ShowMode::Range(start, b.trim().parse().ok()),
            Err(_) => query::ShowMode::Default,
        }
    } else if args.get("full").and_then(Value::as_bool).unwrap_or(false) {
        query::ShowMode::Full
    } else {
        query::ShowMode::Default
    }
}

fn file_mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn write_message(out: &mut impl Write, msg: &Value) -> Result<()> {
    let line = serde_json::to_string(msg)?;
    out.write_all(line.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Node, NodeId, NodeKind};
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("cg_mcp_{}_{n}", std::process::id()))
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let dir = temp_dir();
            std::fs::create_dir_all(&dir).expect("create temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn sample_graph() -> Graph {
        let mut g = Graph::new();
        g.add_node(Node {
            id: NodeId(0),
            kind: NodeKind::Function,
            name: "foo".into(),
            fqn: "foo".into(),
            signature: "fn foo()".into(),
            sig_hash: "aaaabbbbccccdddd".into(),
            file: "a.rs".into(),
            line_start: 1,
            line_end: 1,
            doc: None,
        });
        g
    }

    fn state_at(dir: PathBuf) -> ServerState {
        ServerState {
            cache_path: PathBuf::from("<test>"),
            graph: sample_graph(),
            mtime: None,
            counter: HeuristicCounter,
            memory_store: MemoryStore::new(dir),
        }
    }

    fn sample_state() -> ServerState {
        state_at(temp_dir())
    }

    #[test]
    fn test_initialize() {
        let mut s = sample_state();
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-11-25" }
            }))
            .expect("response");
        assert_eq!(resp["result"]["serverInfo"]["name"], "codegraph");
        assert!(resp["result"]["capabilities"]["tools"].is_object());
        assert_eq!(resp["result"]["protocolVersion"], "2025-11-25");
    }

    #[test]
    fn test_tools_list() {
        let mut s = sample_state();
        let resp = s
            .handle(&json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
            .expect("response");
        let tools = resp["result"]["tools"].as_array().expect("tools array");
        assert_eq!(tools.len(), 6);
    }

    #[test]
    fn test_tools_call_search() {
        let mut s = sample_state();
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": { "name": "search", "arguments": { "query": "foo" } }
            }))
            .expect("response");
        assert_eq!(resp["result"]["isError"], false);
        let text = resp["result"]["content"][0]["text"].as_str().expect("text");
        assert!(text.contains("foo"));
    }

    #[test]
    fn test_tools_call_show() {
        let mut s = sample_state();
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 6, "method": "tools/call",
                "params": { "name": "show", "arguments": { "symbol": "foo" } }
            }))
            .expect("response");
        assert_eq!(resp["result"]["isError"], false);
        let text = resp["result"]["content"][0]["text"].as_str().expect("text");
        assert!(text.contains("foo"));
    }

    #[test]
    fn test_missing_arg_is_tool_error() {
        let mut s = sample_state();
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 5, "method": "tools/call",
                "params": { "name": "context", "arguments": {} }
            }))
            .expect("response");
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn test_unknown_tool_is_error() {
        let mut s = sample_state();
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": { "name": "nope", "arguments": {} }
            }))
            .expect("response");
        assert!(resp["error"].is_object());
    }

    #[test]
    fn test_notification_has_no_response() {
        let mut s = sample_state();
        let resp = s.handle(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        assert!(resp.is_none());
    }

    #[test]
    fn test_recall_empty_returns_no_memories() {
        let mut s = sample_state();
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 10, "method": "tools/call",
                "params": { "name": "recall", "arguments": { "target": "foo" } }
            }))
            .expect("response");
        assert_eq!(resp["result"]["isError"], false);
        let text = resp["result"]["content"][0]["text"].as_str().expect("text");
        assert!(text.contains("\"count\": 0"));
    }

    #[test]
    fn test_recall_returns_memory_with_freshness() {
        let dir = TempDir::new();
        let mut s = state_at(dir.0.clone());
        s.memory_store
            .append(&Event::Created {
                memory: Memory {
                    id: MemoryId("m1".into()),
                    content: "foo must stay pure".into(),
                    anchor: AnchorKey {
                        fqn: "foo".into(),
                        sig_hash: "aaaabbbbccccdddd".into(),
                    },
                    scope: Scope::Symbol("foo".into()),
                    kind: Kind::Invariant,
                    status: Status::Intact,
                    provenance: Provenance::default(),
                },
            })
            .expect("append");

        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 11, "method": "tools/call",
                "params": { "name": "recall", "arguments": { "target": "foo" } }
            }))
            .expect("response");
        assert_eq!(resp["result"]["isError"], false);
        let text = resp["result"]["content"][0]["text"].as_str().expect("text");
        assert!(text.contains("foo must stay pure"));
        // The anchor still matches, so the memory is served as intact.
        assert!(text.contains("\"status\": \"intact\""));
    }

    #[test]
    fn test_recall_missing_target_is_error() {
        let mut s = sample_state();
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 12, "method": "tools/call",
                "params": { "name": "recall", "arguments": {} }
            }))
            .expect("response");
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn test_remember_appends_and_is_recallable() {
        let dir = TempDir::new();
        let mut s = state_at(dir.0.clone());
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 20, "method": "tools/call",
                "params": { "name": "remember", "arguments": {
                    "anchor": "foo", "kind": "decision", "content": "chose FNV for stability"
                } }
            }))
            .expect("response");
        assert_eq!(resp["result"]["isError"], false);

        let recalled = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 21, "method": "tools/call",
                "params": { "name": "recall", "arguments": { "target": "foo" } }
            }))
            .expect("response");
        let text = recalled["result"]["content"][0]["text"]
            .as_str()
            .expect("text");
        assert!(text.contains("chose FNV for stability"));
        // Anchored to the live signature, so it recalls as intact.
        assert!(text.contains("\"status\": \"intact\""));
    }

    #[test]
    fn test_remember_unknown_anchor_is_error() {
        let mut s = sample_state();
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 22, "method": "tools/call",
                "params": { "name": "remember", "arguments": {
                    "anchor": "does_not_exist", "kind": "gotcha", "content": "x"
                } }
            }))
            .expect("response");
        assert_eq!(resp["result"]["isError"], true);
    }

    #[test]
    fn test_remember_unknown_kind_is_error() {
        let mut s = sample_state();
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 23, "method": "tools/call",
                "params": { "name": "remember", "arguments": {
                    "anchor": "foo", "kind": "nonsense", "content": "x"
                } }
            }))
            .expect("response");
        assert_eq!(resp["result"]["isError"], true);
    }
}
