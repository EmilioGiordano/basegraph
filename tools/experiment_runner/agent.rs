//! The agent behind every run: the real Claude Code CLI in headless mode, or
//! a scripted stand-in that applies the reference fixes so the whole pipeline
//! can be exercised without an LLM.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::files;
use crate::git;
use crate::mcp_client::McpSession;
use crate::schema::{Arm, RepoSpec, TaskSpec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Memory seeding at C1 (go-no-go.md §4).
    Capture,
    /// A task run at C3 (§5).
    Task,
}

#[derive(Debug, Clone)]
pub struct McpServer {
    pub name: String,
    pub command: PathBuf,
    pub args: Vec<String>,
}

impl McpServer {
    pub fn config_json(&self) -> String {
        json!({
            "mcpServers": {
                &self.name: {
                    "type": "stdio",
                    "command": self.command.to_string_lossy(),
                    "args": self.args,
                }
            }
        })
        .to_string()
    }
}

pub struct AgentRequest<'a> {
    pub phase: Phase,
    pub arm: Arm,
    pub seed: u32,
    pub cwd: &'a Path,
    pub prompt: String,
    pub allowed_tools: Vec<String>,
    pub mcp: Option<McpServer>,
    pub max_turns: u32,
    pub time_cap: Duration,
    pub model: Option<String>,
    pub budget_usd: Option<f64>,
    /// The source repo (ground truth lives next to it).
    pub repo: &'a RepoSpec,
    pub repo_dir: &'a Path,
    pub task: Option<&'a TaskSpec>,
    pub codegraph_bin: &'a Path,
}

#[derive(Debug, Clone, Default)]
pub struct AgentOutcome {
    /// stream-json lines (or the scripted imitation of them).
    pub transcript: String,
    pub timed_out: bool,
    pub exit_ok: bool,
    pub elapsed: Duration,
}

pub trait Agent {
    fn describe(&self) -> String;
    fn run(&self, req: &AgentRequest) -> Result<AgentOutcome>;
}

/// `claude -p` with the arm's tools, no auto-discovered context (`--bare`)
/// and a permission mode that denies anything not pre-allowed.
pub struct ClaudeCli {
    pub bin: String,
}

impl ClaudeCli {
    pub fn command_args(req: &AgentRequest) -> Vec<String> {
        let mut args: Vec<String> = vec![
            "-p".into(),
            req.prompt.clone(),
            "--output-format".into(),
            "stream-json".into(),
            "--verbose".into(),
            "--bare".into(),
            "--no-session-persistence".into(),
            "--permission-mode".into(),
            "dontAsk".into(),
            "--max-turns".into(),
            req.max_turns.to_string(),
        ];
        if let Some(model) = &req.model {
            args.extend(["--model".into(), model.clone()]);
        }
        if let Some(budget) = req.budget_usd {
            args.extend(["--max-budget-usd".into(), format!("{budget}")]);
        }
        if let Some(mcp) = &req.mcp {
            args.extend([
                "--mcp-config".into(),
                mcp.config_json(),
                "--strict-mcp-config".into(),
            ]);
        }
        // Variadic: keep it last so it cannot swallow other arguments.
        args.push("--allowedTools".into());
        args.extend(req.allowed_tools.iter().cloned());
        args
    }
}

impl Agent for ClaudeCli {
    fn describe(&self) -> String {
        format!("claude cli ({})", self.bin)
    }

    fn run(&self, req: &AgentRequest) -> Result<AgentOutcome> {
        let start = Instant::now();
        let mut child = Command::new(&self.bin)
            .args(Self::command_args(req))
            .current_dir(req.cwd)
            .env("DISABLE_TELEMETRY", "1")
            .env("DISABLE_ERROR_REPORTING", "1")
            .env("DO_NOT_TRACK", "1")
            .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| format!("spawning {}", self.bin))?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let out_thread = std::thread::spawn(move || read_all(stdout));
        let err_thread = std::thread::spawn(move || read_all(stderr));

        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait()? {
                break Some(status);
            }
            if start.elapsed() > req.time_cap {
                timed_out = true;
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        let mut transcript = out_thread.join().unwrap_or_default();
        let stderr_text = err_thread.join().unwrap_or_default();
        if !stderr_text.trim().is_empty() {
            transcript.push_str(&format!(
                "\n{}\n",
                json!({ "type": "runner_stderr", "text": stderr_text })
            ));
        }
        Ok(AgentOutcome {
            transcript,
            timed_out,
            exit_ok: status.map(|s| s.success()).unwrap_or(false),
            elapsed: start.elapsed(),
        })
    }
}

fn read_all(stream: Option<impl Read>) -> String {
    let mut text = String::new();
    if let Some(mut s) = stream {
        let _ = s.read_to_string(&mut text);
    }
    text
}

/// What the scripted agent does with a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fix {
    Correct,
    Wrong,
    /// The wrong fix, without looking at the current code first — a
    /// false-confidence run by construction.
    WrongBlind,
    Noop,
}

impl Fix {
    fn parse(s: &str) -> Option<Fix> {
        match s.trim().to_ascii_lowercase().as_str() {
            "correct" => Some(Fix::Correct),
            "wrong" => Some(Fix::Wrong),
            "wrong-blind" | "wrong_blind" => Some(Fix::WrongBlind),
            "noop" => Some(Fix::Noop),
            _ => None,
        }
    }
}

/// Deterministic stand-in for an LLM: applies the reference fixes and goes
/// through the real memory pipeline (`codegraph mcp` for A2) so the
/// instrumentation sees genuine tool results.
pub struct Scripted {
    pub policy: BTreeMap<Arm, Fix>,
}

impl Scripted {
    /// `a0=wrong,a1=wrong,a2=correct`, or `all=correct`.
    pub fn parse(spec: &str) -> Result<Self> {
        let mut policy = BTreeMap::new();
        for entry in spec.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let (arm, fix) = entry
                .split_once('=')
                .with_context(|| format!("scripted policy entry `{entry}` is not arm=fix"))?;
            let fix = Fix::parse(fix)
                .with_context(|| format!("unknown fix `{fix}` (correct|wrong|wrong-blind|noop)"))?;
            if arm.trim().eq_ignore_ascii_case("all") {
                for arm in Arm::ALL {
                    policy.insert(arm, fix);
                }
            } else {
                let arm = Arm::parse(arm).with_context(|| format!("unknown arm `{arm}`"))?;
                policy.insert(arm, fix);
            }
        }
        if policy.is_empty() {
            bail!("empty scripted policy");
        }
        Ok(Self { policy })
    }

    fn fix_for(&self, arm: Arm) -> Fix {
        self.policy.get(&arm).copied().unwrap_or(Fix::Noop)
    }
}

/// Builds a transcript in the shape of Claude Code's stream-json events.
struct Script {
    lines: Vec<String>,
    next: usize,
}

impl Script {
    fn new() -> Self {
        Self {
            lines: Vec::new(),
            next: 1,
        }
    }

    fn call(&mut self, name: &str, input: Value, result: &str) {
        let id = format!("scripted_{}", self.next);
        self.next += 1;
        self.lines.push(
            json!({ "type": "assistant", "message": { "content": [
                { "type": "tool_use", "id": id, "name": name, "input": input }
            ] } })
            .to_string(),
        );
        self.lines.push(
            json!({ "type": "user", "message": { "content": [
                { "type": "tool_result", "tool_use_id": id, "content": result }
            ] } })
            .to_string(),
        );
    }

    fn finish(mut self, seed: u32, text: &str) -> String {
        let turns = self.next as u64;
        self.lines.push(
            json!({
                "type": "result", "subtype": "success", "is_error": false,
                "num_turns": turns, "duration_ms": 10,
                "total_cost_usd": 0.0,
                "usage": { "input_tokens": 1000 + u64::from(seed), "output_tokens": 200 },
                "result": text
            })
            .to_string(),
        );
        self.lines.join("\n") + "\n"
    }
}

fn read_or(path: &Path, fallback: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| fallback.to_string())
}

impl Scripted {
    fn capture(&self, req: &AgentRequest) -> Result<String> {
        let repo = req.repo;
        let mut script = Script::new();
        script.call(
            "Read",
            json!({ "file_path": repo.anchor_file_c2 }),
            &read_or(&req.cwd.join(&repo.anchor_file_c2), ""),
        );
        // "Solve" the C2 bug by taking the fix from history.
        git::run(
            req.cwd,
            &["checkout", &repo.commits.c2, "--", &repo.anchor_file_c2],
            &[],
        )?;
        script.call(
            "Edit",
            json!({ "file_path": repo.anchor_file_c2 }),
            "applied the fix",
        );
        match req.arm {
            Arm::A1 => {
                let note = format!(
                    "# Gotchas\n\n- `{}` ({}): {}\n",
                    repo.anchor_fqn_c2, repo.anchor_file_c2, repo.invariant_text
                );
                std::fs::write(req.cwd.join("gotchas.md"), &note)?;
                script.call(
                    "Write",
                    json!({ "file_path": "gotchas.md", "content": note }),
                    "ok",
                );
            }
            Arm::A2 => {
                let mut mcp = McpSession::start(req.codegraph_bin, req.cwd)?;
                let args = json!({
                    "anchor": repo.anchor_fqn_c2,
                    "kind": "invariant",
                    "content": repo.invariant_text,
                    "commit": repo.commits.c2,
                });
                let (is_error, text) = mcp.tool("remember", args.clone())?;
                mcp.close()?;
                if is_error {
                    bail!("scripted remember failed: {text}");
                }
                script.call("mcp__codegraph__remember", args, &text);
            }
            Arm::A0 => {}
        }
        Ok(script.finish(req.seed, "Fixed the bug and recorded what I learned."))
    }

    fn task(&self, req: &AgentRequest) -> Result<String> {
        let repo = req.repo;
        let task = req.task.context("scripted task run without a task")?;
        let fix = self.fix_for(req.arm);
        let mut script = Script::new();
        match req.arm {
            Arm::A0 => {
                let log = git::run(req.cwd, &["log", "--oneline", "-8"], &[])?;
                script.call("Bash", json!({ "command": "git log --oneline -8" }), &log);
            }
            Arm::A1 => {
                let path = req.cwd.join("gotchas.md");
                script.call(
                    "Read",
                    json!({ "file_path": path.to_string_lossy() }),
                    &read_or(&path, "(no gotchas.md)"),
                );
            }
            Arm::A2 => {
                let mut mcp = McpSession::start(req.codegraph_bin, req.cwd)?;
                for target in [&repo.anchor_fqn_c3, &repo.anchor_fqn_c2] {
                    let args = json!({ "target": target });
                    let (_, text) = mcp.tool("recall", args.clone())?;
                    script.call("mcp__codegraph__recall", args, &text);
                    if text.contains("\"count\": 1") {
                        break;
                    }
                }
                mcp.close()?;
            }
        }
        if fix != Fix::WrongBlind {
            script.call(
                "Read",
                json!({ "file_path": task.fix_target }),
                &read_or(&req.cwd.join(&task.fix_target), ""),
            );
        }
        let source = match fix {
            Fix::Correct => Some(&task.fix_correct),
            Fix::Wrong | Fix::WrongBlind => Some(&task.fix_wrong),
            Fix::Noop => None,
        };
        if let Some(source) = source {
            files::copy_fresh(&req.repo_dir.join(source), &req.cwd.join(&task.fix_target))?;
            script.call(
                "Write",
                json!({ "file_path": task.fix_target, "content": "(reference fix)" }),
                "ok",
            );
        }
        script.call(
            "Bash",
            json!({ "command": "cargo test" }),
            "(skipped by the scripted agent)",
        );
        Ok(script.finish(req.seed, "Done."))
    }
}

impl Agent for Scripted {
    fn describe(&self) -> String {
        let policy: Vec<String> = self
            .policy
            .iter()
            .map(|(arm, fix)| format!("{}={:?}", arm.label(), fix).to_lowercase())
            .collect();
        format!("scripted ({})", policy.join(","))
    }

    fn run(&self, req: &AgentRequest) -> Result<AgentOutcome> {
        let start = Instant::now();
        let transcript = match req.phase {
            Phase::Capture => self.capture(req)?,
            Phase::Task => self.task(req)?,
        };
        Ok(AgentOutcome {
            transcript,
            timed_out: false,
            exit_ok: true,
            elapsed: start.elapsed(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Commits;

    fn repo() -> RepoSpec {
        RepoSpec {
            repo_id: "repo_01".into(),
            path: "repo_01".into(),
            crate_name: "demo".into(),
            scenario: "sorted_output".into(),
            invariant_type: "sorted_output".into(),
            invariant_text: "sorted".into(),
            anchor_fqn_c2: "merge_windows".into(),
            anchor_fqn_c3: "merge_windows".into(),
            anchor_file_c2: "src/scheduling.rs".into(),
            anchor_file_c3: "src/scheduling.rs".into(),
            drift: false,
            drift_kind: None,
            file_count: 20,
            commits: Commits {
                c1: "a".into(),
                c2: "b".into(),
                c3: "c".into(),
            },
            capture_task: "capture_task.md".into(),
            tasks: vec![],
        }
    }

    #[test]
    fn claude_args_put_the_prompt_first_and_tools_last() {
        let repo = repo();
        let req = AgentRequest {
            phase: Phase::Task,
            arm: Arm::A2,
            seed: 0,
            cwd: Path::new("."),
            prompt: "do the thing".into(),
            allowed_tools: vec!["Read".into(), "Bash(git log *)".into()],
            mcp: Some(McpServer {
                name: "codegraph".into(),
                command: PathBuf::from("C:/bin/codegraph.exe"),
                args: vec!["mcp".into(), "C:/work".into()],
            }),
            max_turns: 30,
            time_cap: Duration::from_secs(10),
            model: Some("claude-sonnet-5".into()),
            budget_usd: Some(1.5),
            repo: &repo,
            repo_dir: Path::new("."),
            task: None,
            codegraph_bin: Path::new("codegraph"),
        };
        let args = ClaudeCli::command_args(&req);
        assert_eq!(&args[..2], &["-p".to_string(), "do the thing".to_string()]);
        assert!(args.contains(&"--bare".to_string()));
        assert!(args.contains(&"dontAsk".to_string()));
        let model_at = args.iter().position(|a| a == "--model").expect("model");
        assert_eq!(args[model_at + 1], "claude-sonnet-5");
        let mcp_at = args.iter().position(|a| a == "--mcp-config").expect("mcp");
        let config: Value = serde_json::from_str(&args[mcp_at + 1]).expect("json");
        assert_eq!(config["mcpServers"]["codegraph"]["args"][0], "mcp");
        assert!(args.contains(&"--strict-mcp-config".to_string()));
        let tools_at = args
            .iter()
            .position(|a| a == "--allowedTools")
            .expect("tools");
        assert_eq!(
            &args[tools_at + 1..],
            &["Read".to_string(), "Bash(git log *)".to_string()]
        );
    }

    #[test]
    fn scripted_policy_parses_all_and_per_arm() {
        let all = Scripted::parse("all=correct").expect("parse");
        assert_eq!(all.fix_for(Arm::A0), Fix::Correct);
        let mixed = Scripted::parse("a0=wrong, a1=wrong-blind ,a2=correct").expect("parse");
        assert_eq!(mixed.fix_for(Arm::A0), Fix::Wrong);
        assert_eq!(mixed.fix_for(Arm::A1), Fix::WrongBlind);
        assert_eq!(mixed.fix_for(Arm::A2), Fix::Correct);
        assert!(Scripted::parse("a9=wrong").is_err());
        assert!(Scripted::parse("a0=maybe").is_err());
        assert!(Scripted::parse("").is_err());
        assert_eq!(
            mixed.describe(),
            "scripted (a0=wrong,a1=wrongblind,a2=correct)"
        );
    }

    #[test]
    fn script_produces_parseable_stream_json() {
        let mut script = Script::new();
        script.call("Read", json!({ "file_path": "x" }), "content");
        let text = script.finish(3, "Done.");
        let parsed = crate::transcript::parse(&text);
        assert_eq!(parsed.calls.len(), 1);
        assert_eq!(parsed.calls[0].result.as_deref(), Some("content"));
        assert_eq!(parsed.usage.unwrap().input_tokens, Some(1003));
        assert_eq!(parsed.final_text, "Done.");
    }
}
