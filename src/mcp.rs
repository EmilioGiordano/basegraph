//! Minimal stdio JSON-RPC server implementing the MCP protocol (2025-11-25).
//!
//! Exposes the graph queries (`map`, `context`, `search`, `show`) and the memory
//! lifecycle (`recall`, `remember`, `reanchor`, `supersede`, `generate_test`) as
//! MCP tools over one codebase, so any MCP client (Claude Code, Cursor, ...)
//! can use them natively.

use std::ffi::OsStr;
use std::io::{BufRead, Write};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use serde_json::{json, Value};

use crate::builder::build_graph;
use crate::cache::{Cache, JsonCache};
use crate::graph::Graph;
use crate::memory::anchor::{self, classify, Classification, ReanchorBasis};
use crate::memory::model::{AnchorKey, Kind, Memory, MemoryId, Provenance, Scope};
use crate::memory::store::{Event, MemoryStore};
use crate::memory::testgen::{self, Assertion};
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
    /// The indexed directory; generated files are only ever written under it.
    dir: PathBuf,
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
            dir: dir.to_path_buf(),
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
                "instructions": "Query a codebase's structure cheaply: `map` to orient, `search` to find a symbol's name, `context` to see its callers/callees/impls/uses, and `show` to read a symbol's source. Use `recall` to fetch what's known about a file or symbol — each memory is tagged with how fresh its anchor is against the current code — and `remember` to record a decision, gotcha, invariant, or past bug anchored to a symbol. When recall reports a memory as evolved or with re-anchor candidates, `reanchor` confirms where it now belongs, and `supersede` retires a memory that no longer applies. `generate_test` turns an intact invariant memory into an executable test."
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
                    "description": "Retrieve stored memories (decisions, gotchas, invariants, past bugs) about a file or symbol. Each memory is annotated with its freshness against the current index: intact, evolved (interface changed), or orphaned (anchor gone, with any uncertain re-anchor candidates). A memory is found by its own name, by the file its anchored symbol lives in (so querying the file you are about to edit works), and by the current name of a symbol that was renamed — an indirect hit says so in `reached_via` and is never presented as a direct one. Use before changing a symbol or a file to learn what past work recorded about it.",
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
                },
                {
                    "name": "reanchor",
                    "description": "Confirm where a drifted memory now belongs. `chosen_fqn` must be one of the re-anchor candidates recall proposed for it, or the memory's own anchor when recall reported it as evolved (accepting the changed interface). The symbol's current signature is snapshotted and a Reanchored event is appended; nothing is ever re-anchored implicitly.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "memory_id": { "type": "string", "description": "Id of the memory to re-anchor" },
                            "chosen_fqn": { "type": "string", "description": "Fully-qualified name to anchor to, taken from recall's candidates" }
                        },
                        "required": ["memory_id", "chosen_fqn"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "supersede",
                    "description": "Retire a memory that no longer applies. Appends a Superseded event: recall stops serving the memory, while the log keeps its history.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "memory_id": { "type": "string", "description": "Id of the memory to retire" }
                        },
                        "required": ["memory_id"],
                        "additionalProperties": false
                    }
                },
                {
                    "name": "generate_test",
                    "description": "Turn an invariant memory into an executable Rust test. Writes a #[test] that imports the anchored pub free function from its crate, calls it with Default::default() inputs and asserts the property inferred from the memory's wording (sorted / not null / non-empty / positive; anything else panics until encoded by hand). The anchor must be intact: confirm with reanchor first if the code drifted. Writes <output_path>/invariant_<memory_id>.rs (or output_path itself when it ends in .rs) inside the indexed directory; a file placed directly in tests/ runs with `cargo test --test invariant_<memory_id>`.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "memory_id": { "type": "string", "description": "Id of an invariant memory" },
                            "output_path": { "type": "string", "description": "Directory (e.g. tests) or .rs file path, relative to the indexed directory" }
                        },
                        "required": ["memory_id", "output_path"],
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
                    .filter_map(|m| {
                        let classification = classify(&m.anchor, &self.graph);
                        reach(m, target, &classification, &self.graph)
                            .map(|reach| memory_view(m, &classification, reach))
                    })
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
                        let suggestions = anchor_suggestions(&self.graph, fqn);
                        let hint = if suggestions.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "; did you mean {}? (anchors use the indexed name: free functions bare, methods as Type::method)",
                                suggestions
                                    .iter()
                                    .map(|s| format!("'{s}'"))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        };
                        ToolError::BadArg(format!(
                            "no symbol '{fqn}' in the current index; the anchor must exist{hint}"
                        ))
                    })?;
                let anchor = anchor::anchor_of(node);
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
                    provenance,
                };
                self.memory_store
                    .append(&Event::Created { memory })
                    .map_err(|e| ToolError::BadArg(format!("writing memory log: {e}")))?;
                Ok(format!("remembered {} anchored to {fqn}", id.0))
            }
            "reanchor" => {
                let memory_id = required_str(args, "memory_id")?;
                let chosen = required_str(args, "chosen_fqn")?;
                let memory = self.find_memory(memory_id)?;
                let anchor = anchor::confirm(&memory.anchor, chosen, &self.graph)
                    .map_err(|e| ToolError::BadArg(format!("cannot reanchor {memory_id}: {e}")))?;
                self.memory_store
                    .append(&Event::Reanchored {
                        id: memory.id.clone(),
                        anchor: anchor.clone(),
                    })
                    .map_err(|e| ToolError::BadArg(format!("writing memory log: {e}")))?;
                Ok(format!(
                    "reanchored {} from {} @ {} to {} @ {}",
                    memory.id.0,
                    memory.anchor.fqn,
                    memory.anchor.sig_hash,
                    anchor.fqn,
                    anchor.sig_hash
                ))
            }
            "supersede" => {
                let memory_id = required_str(args, "memory_id")?;
                let memory = self.find_memory(memory_id)?;
                self.memory_store
                    .append(&Event::Superseded {
                        id: memory.id.clone(),
                    })
                    .map_err(|e| ToolError::BadArg(format!("writing memory log: {e}")))?;
                Ok(format!(
                    "superseded {} (was anchored to {})",
                    memory.id.0, memory.anchor.fqn
                ))
            }
            "generate_test" => {
                let memory_id = required_str(args, "memory_id")?;
                let requested = relative_output_path(required_str(args, "output_path")?)?;
                let memory = self.find_memory(memory_id)?;
                let generated = testgen::generate(&memory, &self.graph).map_err(|e| {
                    ToolError::BadArg(format!("cannot generate a test for {memory_id}: {e}"))
                })?;
                let path = if requested.extension() == Some(OsStr::new("rs")) {
                    self.dir.join(requested)
                } else {
                    self.dir.join(requested).join(&generated.file_name)
                };
                let overwritten = path.is_file();
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| {
                        ToolError::BadArg(format!("creating {}: {e}", parent.display()))
                    })?;
                }
                std::fs::write(&path, &generated.source)
                    .map_err(|e| ToolError::BadArg(format!("writing {}: {e}", path.display())))?;
                let in_tests_dir =
                    path.parent().and_then(Path::file_name) == Some(OsStr::new("tests"));
                let mut notes = Vec::new();
                if !in_tests_dir {
                    notes.push("cargo auto-discovers only tests/*.rs; add a [[test]] target for this file or write it into tests/");
                }
                if generated.assertion == Assertion::Unencoded {
                    notes.push("no assertion was inferred from the memory's wording; the test panics until the invariant is encoded by hand");
                }
                let report = json!({
                    "memory_id": memory.id.0,
                    "symbol": memory.anchor.fqn,
                    "test": generated.test_name,
                    "assertion": generated.assertion.label(),
                    "condition": generated.assertion.condition(),
                    "imports": generated.import_path,
                    "path": path.display().to_string(),
                    "overwritten": overwritten,
                    "run": in_tests_dir.then(|| format!("cargo test --test {}", generated.test_name)),
                    "notes": notes,
                });
                Ok(serde_json::to_string_pretty(&report).unwrap_or_else(|_| report.to_string()))
            }
            other => Err(ToolError::Unknown(other.to_string())),
        }
    }

    /// The current memory with this id; superseded ids are gone from the fold.
    fn find_memory(&self, id: &str) -> Result<Memory, ToolError> {
        self.memory_store
            .materialize()
            .map_err(|e| ToolError::BadArg(format!("reading memory log: {e}")))?
            .into_iter()
            .find(|m| m.id.0 == id)
            .ok_or_else(|| {
                ToolError::BadArg(format!(
                    "no memory `{id}` (unknown id, or already superseded)"
                ))
            })
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

/// Indexed fqns a mistyped anchor probably meant: agents often qualify with
/// the crate/module path, while the index uses bare names for free functions
/// and `Type::method` for methods.
fn anchor_suggestions(graph: &Graph, requested: &str) -> Vec<String> {
    let mut out: Vec<String> = graph
        .nodes()
        .iter()
        .map(|n| n.fqn.as_str())
        .filter(|fqn| {
            requested.ends_with(&format!("::{fqn}")) || fqn.ends_with(&format!("::{requested}"))
        })
        .map(str::to_string)
        .collect();
    out.sort();
    out.dedup();
    out.truncate(3);
    out
}

fn required_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| ToolError::BadArg(format!("missing required argument: {key}")))
}

/// Generated files stay inside the indexed directory: the path must be
/// relative and free of `..`.
fn relative_output_path(output_path: &str) -> Result<&Path, ToolError> {
    let path = Path::new(output_path);
    let inside = !output_path.trim().is_empty()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if inside {
        Ok(path)
    } else {
        Err(ToolError::BadArg(
            "output_path must be a relative path inside the indexed directory (no `..`, no absolute paths)".into(),
        ))
    }
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

/// How a `recall` query reached a memory. Anything but an exact scope match is
/// reported back, so an indirect hit is never mistaken for a direct one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    Scope,
    File,
    Candidate,
}

/// A memory is reachable by its scope, by the file its anchored symbol lives in
/// (today or when it was written), or by a re-anchor candidate — a symbol whose
/// name the agent can actually know after a rename.
fn reach(
    memory: &Memory,
    target: &str,
    classification: &Classification,
    graph: &Graph,
) -> Option<Reach> {
    if scope_matches(&memory.scope, target) {
        Some(Reach::Scope)
    } else if matches_file(&memory.anchor, target, graph) {
        Some(Reach::File)
    } else if proposes(classification, target) {
        Some(Reach::Candidate)
    } else {
        None
    }
}

/// The file the anchored symbol occupies in the current index, or the one
/// recorded with the anchor. An empty recorded file matches nothing.
fn matches_file(anchor: &AnchorKey, target: &str, graph: &Graph) -> bool {
    if target.is_empty() {
        return false;
    }
    let current = graph
        .nodes()
        .iter()
        .find(|n| n.fqn == anchor.fqn)
        .map(|n| n.file.as_str());
    current == Some(target) || (!anchor.file.is_empty() && anchor.file == target)
}

/// Only hash-based candidates are searchable. Token similarity is a 0.5
/// threshold: good enough to propose a re-anchor for review, far too loose to
/// decide what a lookup returns.
fn proposes(classification: &Classification, target: &str) -> bool {
    match classification {
        Classification::ReanchorCandidate { candidates, basis } => {
            matches!(basis, ReanchorBasis::SigHash | ReanchorBasis::ShapeHash)
                && candidates.iter().any(|c| c == target)
        }
        _ => false,
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
        ReanchorBasis::ShapeHash => "same signature shape (renamed)",
        ReanchorBasis::TokenSimilarity => "similar name",
    }
}

/// Render one memory as JSON, always tagged with its freshness against the
/// current index so a memory is never served without saying how stale it is.
fn memory_view(memory: &Memory, classification: &Classification, reach: Reach) -> Value {
    let mut view = json!({
        "id": memory.id.0,
        "kind": format!("{:?}", memory.kind),
        "scope": scope_label(&memory.scope),
        "content": memory.content,
        "anchor": {
            "fqn": memory.anchor.fqn,
            "sig_hash": memory.anchor.sig_hash,
            "file": memory.anchor.file,
        },
        "status": status_label(classification),
        "uncertain": classification.is_uncertain(),
        "provenance": {
            "commit": memory.provenance.commit,
            "session": memory.provenance.session,
        },
    });
    match reach {
        Reach::Scope => {}
        Reach::File => {
            view["reached_via"] = json!(format!("file of anchored symbol {}", memory.anchor.fqn));
        }
        Reach::Candidate => {
            view["reached_via"] = json!(format!("reanchor candidate for {}", memory.anchor.fqn));
        }
    }
    if let Classification::ReanchorCandidate { candidates, basis } = classification {
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
    use crate::memory::model::AnchorKey;
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
            memory_store: MemoryStore::new(&dir),
            dir,
        }
    }

    fn node_of(id: u32, name: &str, fqn: &str, signature: &str, file: &str) -> Node {
        Node {
            id: NodeId(id),
            kind: NodeKind::Function,
            name: name.into(),
            fqn: fqn.into(),
            signature: signature.into(),
            sig_hash: crate::parser::sig::sig_hash(name, signature),
            file: file.into(),
            line_start: 1,
            line_end: 1,
            doc: None,
        }
    }

    fn graph_of(nodes: Vec<Node>) -> Graph {
        let mut g = Graph::new();
        for n in nodes {
            g.add_node(n);
        }
        g
    }

    /// A state over `graph`, with one memory anchored as captured from `before`
    /// (the node as it stood when the memory was written).
    fn state_with_anchor(dir: &TempDir, graph: Graph, before: &Node) -> ServerState {
        let mut s = state_at(dir.0.clone());
        s.graph = graph;
        s.memory_store
            .append(&Event::Created {
                memory: anchored_memory(anchor::anchor_of(before)),
            })
            .expect("append");
        s
    }

    fn recall_json(s: &mut ServerState, target: &str) -> Value {
        let (is_error, text) = call(s, "recall", json!({ "target": target }));
        assert!(!is_error, "recall({target}) failed: {text}");
        serde_json::from_str(&text).expect("recall returns json")
    }

    /// A pure rename: same signature shape, new name, same file.
    fn renamed_pair() -> (Node, Node) {
        (
            node_of(0, "f", "f", "fn f (x : i32) -> i32", "src/a.rs"),
            node_of(0, "g", "g", "fn g (x : i32) -> i32", "src/a.rs"),
        )
    }

    #[test]
    fn test_recall_finds_a_renamed_symbol_by_its_new_name() {
        let dir = TempDir::new();
        let (before, after) = renamed_pair();
        let mut s = state_with_anchor(&dir, graph_of(vec![after]), &before);

        let out = recall_json(&mut s, "g");
        assert_eq!(out["count"], 1, "{out}");
        let m = &out["memories"][0];
        assert_eq!(m["status"], "orphaned");
        assert_eq!(m["uncertain"], true);
        assert_eq!(m["reached_via"], "reanchor candidate for f");
        assert_eq!(m["anchor"]["fqn"], "f");
    }

    #[test]
    fn test_recall_finds_a_dead_anchor_by_its_recorded_file() {
        let dir = TempDir::new();
        let (before, after) = renamed_pair();
        let mut s = state_with_anchor(&dir, graph_of(vec![after]), &before);

        let out = recall_json(&mut s, "src/a.rs");
        assert_eq!(out["count"], 1, "{out}");
        assert_eq!(out["memories"][0]["status"], "orphaned");
        assert_eq!(
            out["memories"][0]["reached_via"],
            "file of anchored symbol f"
        );
    }

    #[test]
    fn test_recall_finds_an_intact_memory_by_its_file() {
        let dir = TempDir::new();
        let node = node_of(0, "keep", "keep", "fn keep (x : i32) -> i32", "src/keep.rs");
        let mut s = state_with_anchor(&dir, graph_of(vec![node.clone()]), &node);

        let out = recall_json(&mut s, "src/keep.rs");
        assert_eq!(out["count"], 1, "{out}");
        assert_eq!(out["memories"][0]["status"], "intact");
        assert_eq!(
            out["memories"][0]["reached_via"],
            "file of anchored symbol keep"
        );
        // The direct hit stays direct: no indirection is reported for it.
        let direct = recall_json(&mut s, "keep");
        assert_eq!(direct["count"], 1);
        assert!(direct["memories"][0]["reached_via"].is_null(), "{direct}");
    }

    #[test]
    fn test_recall_never_retrieves_by_token_similarity() {
        let dir = TempDir::new();
        // Renamed AND reshaped: only the fqn tokens overlap, so the classifier
        // proposes it by similarity — a proposal must not become a lookup.
        let before = node_of(
            0,
            "compute_total",
            "Foo::compute_total",
            "fn compute_total (& self) -> i32",
            "src/foo.rs",
        );
        let after = node_of(
            0,
            "compute_total_sum",
            "Foo::compute_total_sum",
            "fn compute_total_sum (& self , extra : i32) -> i64",
            "src/foo.rs",
        );
        let mut s = state_with_anchor(&dir, graph_of(vec![after]), &before);

        let similar = recall_json(&mut s, "Foo::compute_total_sum");
        assert_eq!(similar["count"], 0, "{similar}");

        // It is still reachable by its file, and still *proposes* the
        // similar name once retrieved.
        let by_file = recall_json(&mut s, "src/foo.rs");
        assert_eq!(by_file["count"], 1, "{by_file}");
        assert_eq!(
            by_file["memories"][0]["reanchor_candidates"][0],
            "Foo::compute_total_sum"
        );
    }

    #[test]
    fn test_recall_loads_legacy_memories_and_an_empty_file_never_matches() {
        let dir = TempDir::new();
        let mut s = state_at(dir.0.clone());
        s.graph = graph_of(vec![node_of(
            0,
            "foo",
            "foo",
            "fn foo (x : i32) -> i32",
            "src/live.rs",
        )]);
        // Two memories as written before `file` existed on the anchor: one
        // whose symbol is still indexed, one whose symbol is gone.
        let legacy = |id: &str, fqn: &str, sig_hash: &str| {
            format!(
                "{{\"version\":1,\"event\":{{\"Created\":{{\"memory\":{{\"id\":\"{id}\",\
                 \"content\":\"legacy note\",\"anchor\":{{\"fqn\":\"{fqn}\",\
                 \"sig_hash\":\"{sig_hash}\",\"shape_hash\":\"\"}},\
                 \"scope\":{{\"Symbol\":\"{fqn}\"}},\"kind\":\"Invariant\",\
                 \"provenance\":{{\"commit\":null,\"session\":null}}}}}}}}}}\n"
            )
        };
        let live_hash = crate::parser::sig::sig_hash("foo", "fn foo (x : i32) -> i32");
        std::fs::write(
            dir.0.join("codegraph-memory.jsonl"),
            format!(
                "{}{}",
                legacy("m-live", "foo", &live_hash),
                legacy("m-gone", "vanished", "0000000000000000")
            ),
        )
        .expect("write legacy log");

        // Both load: the missing field defaults, nothing errors.
        assert_eq!(recall_json(&mut s, "foo")["count"], 1);
        assert_eq!(recall_json(&mut s, "vanished")["count"], 1);

        // The live one is reachable by the file its symbol occupies today,
        // resolved from the graph rather than from the (absent) stored field.
        let by_file = recall_json(&mut s, "src/live.rs");
        assert_eq!(by_file["count"], 1, "{by_file}");
        assert_eq!(by_file["memories"][0]["id"], "m-live");

        // The orphaned one has no recorded file, and an empty file matches
        // nothing — not even an empty query.
        assert_eq!(recall_json(&mut s, "")["count"], 0);
    }

    #[test]
    fn test_recall_regression_orphaned_rename_is_reachable() {
        // Pilot 2, repo_03: `render_invoice` was renamed to `format_invoice`,
        // and every name the agent could think of returned nothing.
        let dir = TempDir::new();
        let before = node_of(
            0,
            "render_invoice",
            "render_invoice",
            "fn render_invoice (invoice : & Invoice) -> String",
            "src/billing.rs",
        );
        let after = node_of(
            0,
            "format_invoice",
            "format_invoice",
            "fn format_invoice (invoice : & Invoice) -> String",
            "src/billing.rs",
        );
        let mut s = state_with_anchor(&dir, graph_of(vec![after]), &before);

        for target in ["format_invoice", "src/billing.rs"] {
            let out = recall_json(&mut s, target);
            assert_eq!(out["count"], 1, "recall({target}): {out}");
            assert_eq!(out["memories"][0]["status"], "orphaned");
            assert!(
                !out["memories"][0]["reached_via"].is_null(),
                "an indirect hit must say so: {out}"
            );
        }
    }

    /// A state over a real one-file crate on disk, indexed for real.
    fn crate_state(dir: &TempDir, lib: &str) -> ServerState {
        std::fs::create_dir_all(dir.0.join("src")).expect("src dir");
        std::fs::write(
            dir.0.join("Cargo.toml"),
            "[package]\nname = \"demo-crate\"\nversion = \"0.1.0\"\n",
        )
        .expect("manifest");
        std::fs::write(dir.0.join("src").join("lib.rs"), lib).expect("lib.rs");
        let mut s = state_at(dir.0.clone());
        s.graph = build_graph(&dir.0).expect("build graph");
        s
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
        assert_eq!(tools.len(), 9);
    }

    #[test]
    fn test_generate_test_missing_args_is_error() {
        let mut s = sample_state();
        let (is_error, text) = call(&mut s, "generate_test", json!({ "memory_id": "m1" }));
        assert!(is_error);
        assert!(text.contains("output_path"), "{text}");
    }

    #[test]
    fn test_generate_test_rejects_paths_outside_the_repo() {
        let mut s = sample_state();
        for bad in ["../elsewhere", "tests/../../x.rs", "/abs/tests", "", "  "] {
            let (is_error, text) = call(
                &mut s,
                "generate_test",
                json!({ "memory_id": "m1", "output_path": bad }),
            );
            assert!(is_error, "{bad:?}");
            assert!(text.contains("output_path must be"), "{bad:?}: {text}");
        }
    }

    #[test]
    fn test_generate_test_unknown_memory_is_error() {
        let mut s = sample_state();
        let (is_error, text) = call(
            &mut s,
            "generate_test",
            json!({ "memory_id": "nope", "output_path": "tests" }),
        );
        assert!(is_error);
        assert!(text.contains("no memory `nope`"), "{text}");
    }

    #[test]
    fn test_generate_test_rejects_non_invariants() {
        let dir = TempDir::new();
        let mut s = state_at(dir.0.clone());
        s.memory_store
            .append(&Event::Created {
                memory: Memory {
                    kind: Kind::Decision,
                    ..anchored_memory(AnchorKey {
                        fqn: "foo".into(),
                        sig_hash: "aaaabbbbccccdddd".into(),
                        shape_hash: String::new(),
                        file: String::new(),
                    })
                },
            })
            .expect("append");
        let (is_error, text) = call(
            &mut s,
            "generate_test",
            json!({ "memory_id": "m1", "output_path": "tests" }),
        );
        assert!(is_error);
        assert!(text.contains("not an invariant"), "{text}");
    }

    #[test]
    fn test_generate_test_refuses_a_drifted_anchor() {
        let dir = TempDir::new();
        let mut s = crate_state(&dir, "pub fn compute(x: i32) -> i32 { x + 1 }\n");
        s.memory_store
            .append(&Event::Created {
                memory: anchored_memory(AnchorKey {
                    fqn: "compute".into(),
                    sig_hash: "0000000000000000".into(),
                    shape_hash: String::new(),
                    file: String::new(),
                }),
            })
            .expect("append");
        let (is_error, text) = call(
            &mut s,
            "generate_test",
            json!({ "memory_id": "m1", "output_path": "tests" }),
        );
        assert!(is_error);
        assert!(
            text.contains("evolved") && text.contains("reanchor"),
            "{text}"
        );
        assert!(!dir.0.join("tests").exists(), "nothing should be written");
    }

    #[test]
    fn test_generate_test_writes_the_file_and_reports() {
        let dir = TempDir::new();
        let mut s = crate_state(&dir, "pub fn compute(x: i32) -> i32 { x + 1 }\n");
        let (is_error, text) = call(
            &mut s,
            "remember",
            json!({ "anchor": "compute", "kind": "invariant", "content": "always positive" }),
        );
        assert!(!is_error, "{text}");

        let (is_error, text) = call(
            &mut s,
            "generate_test",
            json!({ "memory_id": "mem-0", "output_path": "tests" }),
        );
        assert!(!is_error, "{text}");
        let report: Value = serde_json::from_str(&text).expect("json report");
        assert_eq!(report["symbol"], "compute");
        assert_eq!(report["test"], "invariant_mem_0");
        assert_eq!(report["assertion"], "positive");
        assert_eq!(report["condition"], "result > Default::default()");
        assert_eq!(report["imports"], "demo_crate::compute");
        assert_eq!(report["run"], "cargo test --test invariant_mem_0");
        assert_eq!(report["overwritten"], false);
        assert_eq!(report["notes"].as_array().map(Vec::len), Some(0));
        let path = dir.0.join("tests").join("invariant_mem_0.rs");
        assert_eq!(report["path"], path.display().to_string());
        let source = std::fs::read_to_string(&path).expect("generated file");
        assert!(source.contains("use demo_crate::compute;"), "{source}");
        assert!(source.contains("fn invariant_mem_0()"), "{source}");

        // Regenerating overwrites, and says so.
        let (_, text) = call(
            &mut s,
            "generate_test",
            json!({ "memory_id": "mem-0", "output_path": "tests" }),
        );
        let report: Value = serde_json::from_str(&text).expect("json report");
        assert_eq!(report["overwritten"], true);
    }

    #[test]
    fn test_generate_test_explicit_file_outside_tests_gets_notes() {
        let dir = TempDir::new();
        let mut s = crate_state(&dir, "pub fn touch() {}\n");
        let (is_error, text) = call(
            &mut s,
            "remember",
            json!({ "anchor": "touch", "kind": "invariant", "content": "must be idempotent" }),
        );
        assert!(!is_error, "{text}");
        let (is_error, text) = call(
            &mut s,
            "generate_test",
            json!({ "memory_id": "mem-0", "output_path": "checks/generated/touch_check.rs" }),
        );
        assert!(!is_error, "{text}");
        let report: Value = serde_json::from_str(&text).expect("json report");
        assert_eq!(report["assertion"], "unencoded");
        assert!(report["condition"].is_null());
        assert!(report["run"].is_null());
        assert_eq!(report["notes"].as_array().map(Vec::len), Some(2));
        assert!(dir
            .0
            .join("checks")
            .join("generated")
            .join("touch_check.rs")
            .is_file());
    }

    #[test]
    fn test_supersede_missing_arg_is_error() {
        let mut s = sample_state();
        let (is_error, text) = call(&mut s, "supersede", json!({}));
        assert!(is_error);
        assert!(text.contains("memory_id"), "{text}");
    }

    #[test]
    fn test_supersede_unknown_memory_is_error() {
        let mut s = sample_state();
        let (is_error, text) = call(&mut s, "supersede", json!({ "memory_id": "nope" }));
        assert!(is_error);
        assert!(text.contains("no memory `nope`"), "{text}");
    }

    #[test]
    fn test_supersede_hides_the_memory_once() {
        let dir = TempDir::new();
        let mut s = state_with_memory(
            &dir,
            AnchorKey {
                fqn: "foo".into(),
                sig_hash: "aaaabbbbccccdddd".into(),
                shape_hash: String::new(),
                file: String::new(),
            },
        );
        let (is_error, text) = call(&mut s, "supersede", json!({ "memory_id": "m1" }));
        assert!(!is_error, "{text}");
        assert!(text.contains("superseded m1"), "{text}");

        let (_, recalled) = call(&mut s, "recall", json!({ "target": "foo" }));
        assert!(recalled.contains("\"count\": 0"), "{recalled}");

        // Gone from the fold, so it cannot be superseded or re-anchored again.
        let (is_error, _) = call(&mut s, "supersede", json!({ "memory_id": "m1" }));
        assert!(is_error);
        let (is_error, _) = call(
            &mut s,
            "reanchor",
            json!({ "memory_id": "m1", "chosen_fqn": "foo" }),
        );
        assert!(is_error);

        // The log keeps the history.
        let events = s.memory_store.load_events().expect("events");
        assert_eq!(events.len(), 2);
        assert!(matches!(events[1], Event::Superseded { .. }));
    }

    /// An invariant memory scoped to and anchored at `anchor.fqn`.
    fn anchored_memory(anchor: AnchorKey) -> Memory {
        Memory {
            id: MemoryId("m1".into()),
            content: "foo must stay pure".into(),
            scope: Scope::Symbol(anchor.fqn.clone()),
            anchor,
            kind: Kind::Invariant,
            provenance: Provenance::default(),
        }
    }

    fn state_with_memory(dir: &TempDir, anchor: AnchorKey) -> ServerState {
        let s = state_at(dir.0.clone());
        s.memory_store
            .append(&Event::Created {
                memory: anchored_memory(anchor),
            })
            .expect("append");
        s
    }

    fn call(s: &mut ServerState, name: &str, arguments: Value) -> (bool, String) {
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 99, "method": "tools/call",
                "params": { "name": name, "arguments": arguments }
            }))
            .expect("response");
        let is_error = resp["result"]["isError"].as_bool().expect("isError");
        let text = resp["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .to_string();
        (is_error, text)
    }

    #[test]
    fn test_reanchor_missing_args_is_error() {
        let mut s = sample_state();
        let (is_error, text) = call(&mut s, "reanchor", json!({ "memory_id": "m1" }));
        assert!(is_error);
        assert!(text.contains("chosen_fqn"), "{text}");
    }

    #[test]
    fn test_reanchor_unknown_memory_is_error() {
        let mut s = sample_state();
        let (is_error, text) = call(
            &mut s,
            "reanchor",
            json!({ "memory_id": "nope", "chosen_fqn": "foo" }),
        );
        assert!(is_error);
        assert!(text.contains("no memory `nope`"), "{text}");
    }

    #[test]
    fn test_reanchor_intact_memory_is_error() {
        let dir = TempDir::new();
        let mut s = state_with_memory(
            &dir,
            AnchorKey {
                fqn: "foo".into(),
                sig_hash: "aaaabbbbccccdddd".into(),
                shape_hash: String::new(),
                file: String::new(),
            },
        );
        let (is_error, text) = call(
            &mut s,
            "reanchor",
            json!({ "memory_id": "m1", "chosen_fqn": "foo" }),
        );
        assert!(is_error);
        assert!(text.contains("intact"), "{text}");
    }

    #[test]
    fn test_reanchor_rejects_an_unproposed_fqn() {
        let dir = TempDir::new();
        // Anchored to a symbol that is gone; `foo` carries the same signature
        // hash, so recall proposes it — and only it.
        let mut s = state_with_memory(
            &dir,
            AnchorKey {
                fqn: "bar".into(),
                sig_hash: "aaaabbbbccccdddd".into(),
                shape_hash: String::new(),
                file: String::new(),
            },
        );
        let (is_error, text) = call(
            &mut s,
            "reanchor",
            json!({ "memory_id": "m1", "chosen_fqn": "somewhere" }),
        );
        assert!(is_error);
        assert!(text.contains("not a proposed candidate"), "{text}");
        assert!(text.contains("foo"), "{text}");
        // Nothing was written.
        let (_, recalled) = call(&mut s, "recall", json!({ "target": "bar" }));
        assert!(recalled.contains("\"status\": \"orphaned\""), "{recalled}");
    }

    #[test]
    fn test_reanchor_confirms_a_candidate_and_recall_follows() {
        let dir = TempDir::new();
        let mut s = state_with_memory(
            &dir,
            AnchorKey {
                fqn: "bar".into(),
                sig_hash: "aaaabbbbccccdddd".into(),
                shape_hash: String::new(),
                file: String::new(),
            },
        );
        let (_, recalled) = call(&mut s, "recall", json!({ "target": "bar" }));
        assert!(recalled.contains("\"uncertain\": true"), "{recalled}");
        assert!(recalled.contains("\"foo\""), "{recalled}");

        let (is_error, text) = call(
            &mut s,
            "reanchor",
            json!({ "memory_id": "m1", "chosen_fqn": "foo" }),
        );
        assert!(!is_error, "{text}");
        assert!(text.contains("reanchored m1 from bar"), "{text}");

        // The memory is now about `foo`, served as intact, and no longer
        // found under the old name.
        let (_, recalled) = call(&mut s, "recall", json!({ "target": "foo" }));
        assert!(recalled.contains("\"count\": 1"), "{recalled}");
        assert!(recalled.contains("\"status\": \"intact\""), "{recalled}");
        let (_, old) = call(&mut s, "recall", json!({ "target": "bar" }));
        assert!(old.contains("\"count\": 0"), "{old}");

        // Confirming again has nothing to do.
        let (is_error, text) = call(
            &mut s,
            "reanchor",
            json!({ "memory_id": "m1", "chosen_fqn": "foo" }),
        );
        assert!(is_error);
        assert!(text.contains("intact"), "{text}");
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
                        shape_hash: String::new(),
                        file: String::new(),
                    },
                    scope: Scope::Symbol("foo".into()),
                    kind: Kind::Invariant,
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
    fn test_remember_qualified_anchor_suggests_the_indexed_name() {
        let mut s = sample_state();
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 24, "method": "tools/call",
                "params": { "name": "remember", "arguments": {
                    "anchor": "demo::app::foo", "kind": "gotcha", "content": "x"
                } }
            }))
            .expect("response");
        assert_eq!(resp["result"]["isError"], true);
        let text = resp["result"]["content"][0]["text"].as_str().expect("text");
        assert!(text.contains("did you mean 'foo'"), "{text}");

        // A name nothing matches gets the plain error, no suggestions.
        let resp = s
            .handle(&json!({
                "jsonrpc": "2.0", "id": 25, "method": "tools/call",
                "params": { "name": "remember", "arguments": {
                    "anchor": "zzz::nope", "kind": "gotcha", "content": "x"
                } }
            }))
            .expect("response");
        let text = resp["result"]["content"][0]["text"].as_str().expect("text");
        assert!(!text.contains("did you mean"), "{text}");
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
