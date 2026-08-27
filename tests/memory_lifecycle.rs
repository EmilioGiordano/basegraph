//! End-to-end lifecycle of a symbol-anchored memory, first through the library
//! API and then through the real binary's MCP server over stdio:
//! remember → drift → recall reports it → reanchor confirmed → the generated
//! test detects the broken invariant.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio};

use codegraph::builder::build_graph;
use codegraph::graph::Graph;
use codegraph::memory::anchor::{self, classify, Classification, ConfirmError};
use codegraph::memory::model::{Kind, Memory, MemoryId, Provenance, Scope};
use codegraph::memory::store::{Event, MemoryStore};
use codegraph::memory::testgen::{self, Assertion};
use codegraph::model::Node;
use serde_json::{json, Value};

const BEFORE: &str = "pub fn compute(x: i32) -> i32 {\n    x + 1\n}\n";
// The drift changes the interface as well as the behaviour: `sig_hash` covers
// only the signature, so a body-only change would still classify as Intact.
const AFTER: &str = "pub fn compute(x: i64) -> i64 {\n    -1\n}\n";
const INVARIANT: &str = "return value is always positive";
const VIOLATION: &str =
    "invariant violated for `compute`: return value is always positive (got -1)";

struct TempCrate(PathBuf);

impl TempCrate {
    fn new(tag: &str, lib: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cg_lifecycle_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("src")).expect("create crate dir");
        fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"demo-crate\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
        )
        .expect("write manifest");
        let temp = Self(dir);
        temp.write_lib(lib);
        temp
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write_lib(&self, source: &str) {
        fs::write(self.0.join("src").join("lib.rs"), source).expect("write lib.rs");
    }

    fn write_test(&self, file_name: &str, source: &str) {
        let tests = self.0.join("tests");
        fs::create_dir_all(&tests).expect("create tests dir");
        fs::write(tests.join(file_name), source).expect("write generated test");
    }

    /// Run one integration test target of the temp crate. Its target dir is
    /// isolated: sharing the outer build's would deadlock on cargo's lock.
    fn run_test(&self, stem: &str) -> Output {
        Command::new(cargo())
            .args(["test", "--offline", "--test", stem])
            .current_dir(&self.0)
            .env("CARGO_TARGET_DIR", self.0.join("target"))
            .output()
            .expect("run cargo test")
    }
}

impl Drop for TempCrate {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

fn node<'a>(graph: &'a Graph, fqn: &str) -> &'a Node {
    graph
        .nodes()
        .iter()
        .find(|n| n.fqn == fqn)
        .unwrap_or_else(|| panic!("`{fqn}` should be in the index"))
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

#[test]
fn full_memory_lifecycle() {
    let repo = TempCrate::new("lib", BEFORE);

    // 1. Index the repo.
    let graph = build_graph(repo.path()).expect("index");
    let compute = node(&graph, "compute");

    // 2. Remember an invariant anchored to `compute`.
    let store = MemoryStore::new(repo.path());
    let memory = Memory {
        id: MemoryId("test-inv".into()),
        content: INVARIANT.into(),
        anchor: anchor::anchor_of(compute),
        scope: Scope::Symbol("compute".into()),
        kind: Kind::Invariant,
        provenance: Provenance::default(),
    };
    store
        .append(&Event::Created {
            memory: memory.clone(),
        })
        .expect("remember");
    assert_eq!(classify(&memory.anchor, &graph), Classification::Intact);

    // Control: the generated test passes on the code the invariant was
    // recorded against, so a later failure is attributable to the drift.
    let generated = testgen::generate(&memory, &graph).expect("generate before drift");
    assert_eq!(generated.assertion, Assertion::Positive);
    assert_eq!(generated.test_name, "invariant_test_inv");
    repo.write_test(&generated.file_name, &generated.source);
    let before = repo.run_test(&generated.test_name);
    assert!(
        before.status.success(),
        "the generated test should pass before the drift:\n{}",
        combined(&before)
    );

    // 3. Drift: the interface and the behaviour change; re-index.
    repo.write_lib(AFTER);
    let drifted = build_graph(repo.path()).expect("re-index");

    // 4. Recall classifies live: the anchor evolved, and no test can be
    // generated from it until the re-anchor is confirmed.
    let recalled = store.materialize().expect("materialize");
    assert_eq!(recalled.len(), 1);
    assert_eq!(
        classify(&recalled[0].anchor, &drifted),
        Classification::Evolved
    );
    assert!(testgen::generate(&recalled[0], &drifted).is_err());

    // 5. Confirm the re-anchor onto the evolved interface. Only what the
    // classifier proposed is accepted.
    assert_eq!(
        anchor::confirm(&recalled[0].anchor, "other", &drifted),
        Err(ConfirmError::NotProposed {
            fqn: "compute".into(),
            chosen: "other".into(),
            candidates: vec!["compute".into()],
        })
    );
    let new_anchor = anchor::confirm(&recalled[0].anchor, "compute", &drifted).expect("confirm");
    assert_ne!(new_anchor.sig_hash, memory.anchor.sig_hash);
    store
        .append(&Event::Reanchored {
            id: memory.id.clone(),
            anchor: new_anchor.clone(),
        })
        .expect("reanchor");
    let updated = store.materialize().expect("materialize");
    assert_eq!(updated[0].anchor, new_anchor);
    assert_eq!(
        classify(&updated[0].anchor, &drifted),
        Classification::Intact
    );

    // 6. Generate the test from the re-anchored memory.
    let generated = testgen::generate(&updated[0], &drifted).expect("generate after reanchor");
    repo.write_test(&generated.file_name, &generated.source);

    // 7. Run it: it must fail, because the drift broke the invariant.
    let after = repo.run_test(&generated.test_name);
    let log = combined(&after);
    assert!(
        !after.status.success(),
        "the generated test should fail after the drift:\n{log}"
    );
    assert!(log.contains(VIOLATION), "unexpected failure output:\n{log}");
}

struct McpSession {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpSession {
    fn start(dir: &Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_codegraph"))
            .arg("mcp")
            .arg(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn mcp server");
        let stdin = child.stdin.take().expect("stdin");
        let stdout = BufReader::new(child.stdout.take().expect("stdout"));
        let mut session = Self {
            child,
            stdin: Some(stdin),
            stdout,
            next_id: 1,
        };
        let init = session.request("initialize", json!({ "protocolVersion": "2025-11-25" }));
        assert_eq!(init["result"]["serverInfo"]["name"], "codegraph");
        session
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let stdin = self.stdin.as_mut().expect("session is open");
        writeln!(stdin, "{msg}").expect("write request");
        stdin.flush().expect("flush request");
        let mut line = String::new();
        let read = self.stdout.read_line(&mut line).expect("read response");
        assert!(read > 0, "server closed stdout while answering {method}");
        let response: Value = serde_json::from_str(line.trim()).expect("json response");
        assert_eq!(response["id"], id, "response id mismatch");
        response
    }

    /// Call a tool and return `(is_error, text)`.
    fn tool(&mut self, name: &str, arguments: Value) -> (bool, String) {
        let resp = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        );
        let result = &resp["result"];
        assert!(result.is_object(), "{name} hit a protocol error: {resp}");
        let is_error = result["isError"].as_bool().unwrap_or(false);
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        (is_error, text)
    }

    fn ok(&mut self, name: &str, arguments: Value) -> String {
        let (is_error, text) = self.tool(name, arguments);
        assert!(!is_error, "{name} should succeed, got: {text}");
        text
    }

    fn err(&mut self, name: &str, arguments: Value) -> String {
        let (is_error, text) = self.tool(name, arguments);
        assert!(is_error, "{name} should be refused, got: {text}");
        text
    }

    fn close(mut self) {
        // EOF on stdin ends the server loop.
        self.stdin.take();
        let status = self.child.wait().expect("wait for server");
        assert!(status.success(), "server exited with {status}");
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        if let Ok(None) = self.child.try_wait() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn rebuild(dir: &Path) {
    let build = Command::new(env!("CARGO_BIN_EXE_codegraph"))
        .arg("build")
        .arg(dir)
        .output()
        .expect("run codegraph build");
    assert!(build.status.success(), "{}", combined(&build));
}

#[test]
fn lifecycle_over_mcp() {
    let repo = TempCrate::new("mcp", BEFORE);

    // 1. codegraph build ./test-repo
    rebuild(repo.path());
    let mut mcp = McpSession::start(repo.path());
    let tools = mcp.request("tools/list", json!({}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    for expected in ["reanchor", "supersede", "generate_test"] {
        assert!(
            names.contains(&expected),
            "missing tool {expected}: {names:?}"
        );
    }

    // 2. remember an invariant anchored to `compute`
    let text = mcp.ok(
        "remember",
        json!({ "anchor": "compute", "kind": "invariant", "content": INVARIANT }),
    );
    assert!(text.contains("remembered mem-0"), "{text}");
    let recalled = mcp.ok("recall", json!({ "target": "compute" }));
    assert!(recalled.contains("\"status\": \"intact\""), "{recalled}");

    // 3. drift, then re-index while the server is running (it reloads the
    // cache when the file changes)
    repo.write_lib(AFTER);
    rebuild(repo.path());

    // 4. recall reports the drift; a test cannot be generated until confirmed
    let recalled = mcp.ok("recall", json!({ "target": "compute" }));
    assert!(recalled.contains("\"status\": \"evolved\""), "{recalled}");
    let refused = mcp.err(
        "generate_test",
        json!({ "memory_id": "mem-0", "output_path": "tests" }),
    );
    assert!(refused.contains("reanchor"), "{refused}");

    // 5. reanchor: only the proposal is accepted, and only once
    let refused = mcp.err(
        "reanchor",
        json!({ "memory_id": "mem-0", "chosen_fqn": "somewhere_else" }),
    );
    assert!(refused.contains("not a proposed candidate"), "{refused}");
    let confirmed = mcp.ok(
        "reanchor",
        json!({ "memory_id": "mem-0", "chosen_fqn": "compute" }),
    );
    assert!(confirmed.contains("reanchored mem-0"), "{confirmed}");
    let recalled = mcp.ok("recall", json!({ "target": "compute" }));
    assert!(recalled.contains("\"status\": \"intact\""), "{recalled}");
    let refused = mcp.err(
        "reanchor",
        json!({ "memory_id": "mem-0", "chosen_fqn": "compute" }),
    );
    assert!(refused.contains("intact"), "{refused}");

    // 6. generate the test from the invariant memory
    let report = mcp.ok(
        "generate_test",
        json!({ "memory_id": "mem-0", "output_path": "tests" }),
    );
    let report: Value = serde_json::from_str(&report).expect("json report");
    assert_eq!(report["assertion"], "positive");
    assert_eq!(report["test"], "invariant_mem_0");
    assert_eq!(report["run"], "cargo test --test invariant_mem_0");
    let path = repo.path().join("tests").join("invariant_mem_0.rs");
    assert!(
        path.is_file(),
        "generated test missing at {}",
        path.display()
    );
    let source = fs::read_to_string(&path).expect("read generated test");
    assert!(source.contains("use demo_crate::compute;"), "{source}");

    // 7. run it: it must fail because the drift broke the invariant
    let run = repo.run_test("invariant_mem_0");
    let log = combined(&run);
    assert!(!run.status.success(), "should fail after the drift:\n{log}");
    assert!(log.contains(VIOLATION), "unexpected failure output:\n{log}");

    // 8. supersede: the memory leaves recall and cannot be reused
    let text = mcp.ok("supersede", json!({ "memory_id": "mem-0" }));
    assert!(text.contains("superseded mem-0"), "{text}");
    let recalled = mcp.ok("recall", json!({ "target": "compute" }));
    assert!(recalled.contains("\"count\": 0"), "{recalled}");
    mcp.err("supersede", json!({ "memory_id": "mem-0" }));
    mcp.err(
        "generate_test",
        json!({ "memory_id": "mem-0", "output_path": "tests" }),
    );
    mcp.close();

    // The log keeps the whole history in order.
    let events = MemoryStore::new(repo.path())
        .load_events()
        .expect("load events");
    assert_eq!(events.len(), 3);
    assert!(matches!(events[0], Event::Created { .. }));
    assert!(matches!(events[1], Event::Reanchored { .. }));
    assert!(matches!(events[2], Event::Superseded { .. }));
}
