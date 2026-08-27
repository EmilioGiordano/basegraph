//! Minimal client for the codegraph MCP server over stdio, used by the
//! scripted agent and the memory-seeding pipeline.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

pub struct McpSession {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpSession {
    pub fn start(codegraph_bin: &Path, dir: &Path) -> Result<Self> {
        let mut child = Command::new(codegraph_bin)
            .arg("mcp")
            .arg(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("spawning {} mcp", codegraph_bin.display()))?;
        let stdin = child.stdin.take().context("mcp stdin")?;
        let stdout = BufReader::new(child.stdout.take().context("mcp stdout")?);
        let mut session = Self {
            child,
            stdin: Some(stdin),
            stdout,
            next_id: 1,
        };
        let init = session.request("initialize", json!({ "protocolVersion": "2025-11-25" }))?;
        if init["result"]["serverInfo"]["name"] != "codegraph" {
            bail!("unexpected initialize response: {init}");
        }
        Ok(session)
    }

    pub fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let stdin = self.stdin.as_mut().context("mcp session closed")?;
        writeln!(stdin, "{msg}")?;
        stdin.flush()?;
        let mut line = String::new();
        if self.stdout.read_line(&mut line)? == 0 {
            bail!("mcp server closed stdout while answering {method}");
        }
        let response: Value = serde_json::from_str(line.trim()).context("mcp response")?;
        if response["id"] != id {
            bail!("mcp response id mismatch: {response}");
        }
        Ok(response)
    }

    /// Call a tool; returns `(is_error, text)`.
    pub fn tool(&mut self, name: &str, arguments: Value) -> Result<(bool, String)> {
        let resp = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )?;
        let result = &resp["result"];
        if !result.is_object() {
            bail!("{name}: protocol error: {resp}");
        }
        let is_error = result["isError"].as_bool().unwrap_or(false);
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        Ok((is_error, text))
    }

    pub fn close(mut self) -> Result<()> {
        self.stdin.take();
        let status = self.child.wait()?;
        if !status.success() {
            bail!("mcp server exited with {status}");
        }
        Ok(())
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
