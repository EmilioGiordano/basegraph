//! Minimal stdio JSON-RPC server implementing the MCP protocol (2025-11-25).
//!
//! Exposes `map`, `context` and `search` as MCP tools over one codebase so any
//! MCP client (Claude Code, Cursor, ...) can query the graph natively.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::builder::build_graph;
use crate::cache::{Cache, JsonCache};
use crate::graph::Graph;
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
        let line = line.context("reading stdin")?;
        let trimmed = line.trim_start_matches('\u{feff}').trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(trimmed) {
            Ok(msg) => {
                if let Some(response) = state.handle(&msg) {
                    write_message(&mut out, &response)?;
                }
            }
            Err(e) => {
                let err = error_response(Value::Null, -32700, &format!("parse error: {e}"));
                write_message(&mut out, &err)?;
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
        })
    }

    /// Reload the graph if the cache file changed since we last read it (e.g. the
    /// agent rebuilt it while the server was running).
    fn reload_if_changed(&mut self) {
        let current = file_mtime(&self.cache_path);
        if current != self.mtime {
            if let Ok(g) = JsonCache::new(&self.cache_path).load() {
                self.graph = g;
                self.mtime = current;
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
                "instructions": "Query a codebase's structure cheaply: `search` to find a symbol's name, `context` to see its callers/callees/impls/uses, `map` to orient in the whole project."
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

        let text = match name {
            "map" => {
                let budget = arg_usize(&args, "budget", DEFAULT_BUDGET);
                query::map(&self.graph, budget, &self.counter).to_text()
            }
            "context" => {
                let Some(symbol) = args.get("symbol").and_then(Value::as_str) else {
                    return tool_error(id, "missing required argument: symbol");
                };
                let budget = arg_usize(&args, "budget", DEFAULT_BUDGET);
                query::context(&self.graph, symbol, budget, &self.counter).to_text()
            }
            "search" => {
                let Some(query) = args.get("query").and_then(Value::as_str) else {
                    return tool_error(id, "missing required argument: query");
                };
                let limit = arg_usize(&args, "limit", DEFAULT_SEARCH_LIMIT);
                query::render_items(&query::search(&self.graph, query, limit))
            }
            other => return error_response(id, -32602, &format!("unknown tool: {other}")),
        };

        success(
            id,
            json!({
                "content": [ { "type": "text", "text": text } ],
                "isError": false
            }),
        )
    }
}

fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_error(id: Value, message: &str) -> Value {
    success(
        id,
        json!({ "content": [ { "type": "text", "text": message } ], "isError": true }),
    )
}

fn arg_usize(args: &Value, key: &str, default: usize) -> usize {
    args.get(key)
        .and_then(Value::as_u64)
        .map(|n| n as usize)
        .unwrap_or(default)
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

    fn sample_state() -> ServerState {
        let mut g = Graph::new();
        g.add_node(Node {
            id: NodeId(0),
            kind: NodeKind::Function,
            name: "foo".into(),
            fqn: "foo".into(),
            signature: "fn foo()".into(),
            file: "a.rs".into(),
            line_start: 1,
            line_end: 1,
            doc: None,
        });
        ServerState {
            cache_path: PathBuf::from("<test>"),
            graph: g,
            mtime: None,
            counter: HeuristicCounter,
        }
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
        assert_eq!(tools.len(), 3);
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
}
