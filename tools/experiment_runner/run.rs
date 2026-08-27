//! One task run (go-no-go.md §5): fresh clone at C3, arm-specific material
//! and coaching, the agent, then the primary suite and the hidden-invariant
//! oracle, recorded as one `RunRecord`.

use std::time::Duration;

use anyhow::{Context, Result};
use codegraph::tokens::{HeuristicCounter, TokenCounter};

use crate::agent::{AgentRequest, Phase};
use crate::capture::CaptureArtifact;
use crate::cargo::{cargo_test, Outcome};
use crate::files;
use crate::git;
use crate::plan::PlanItem;
use crate::schema::{Arm, RepoSpec, RunRecord, TaskSpec};
use crate::transcript;
use crate::Ctx;

/// Per-arm prompt blocks (pilot protocol wording). A0 gets a plain tool
/// restriction; A1 and A2 get the symmetric "consult your material" line.
pub const COACH_A0: &str = "Tools allowed: grep, read, glob, git log.\nDo NOT use codegraph, recall, remember, or any markdown file.\n";
pub const COACH_A1: &str = "Read gotchas.md first. Consult it for what you're about to touch.\n";
pub const COACH_A2: &str =
    "Use 'recall' to check what's known about the symbol you're about to touch.\n";

pub fn task_prompt(description: &str, arm: Arm, primary_stem: &str) -> String {
    format!(
        "Task: {}\n\n{}Solve the task. Run: cargo test --test {primary_stem}\n",
        description.trim_end(),
        coaching(arm)
    )
}

/// Keeps experiment material out of `git status` in the clones.
pub const EXCLUDE: &str = "gotchas.md\ncodegraph.json\ncodegraph-memory.jsonl\ntests/\n";

const BASE_TOOLS: &[&str] = &["Read", "Grep", "Glob", "LS", "Edit", "Write", "MultiEdit"];
const BASH_PREFIXES: &[&str] = &[
    "cargo build",
    "cargo test",
    "cargo check",
    "git log",
    "git show",
    "git diff",
    "git blame",
    "git status",
];
const MCP_TOOLS: &[&str] = &["mcp__codegraph__recall", "mcp__codegraph__remember"];

/// Tool allowlist per arm: read/grep/glob + editing + cargo + `git log`
/// archaeology for everyone; the memory tools only for A2.
pub fn allowed_tools(arm: Arm) -> Vec<String> {
    let mut tools: Vec<String> = BASE_TOOLS.iter().map(|t| t.to_string()).collect();
    for prefix in BASH_PREFIXES {
        // Plain and `rtk`-prefixed forms: the operator's global CLAUDE.md
        // (loaded for every arm alike) tells agents to prefix commands with
        // `rtk`, and a denial there only wastes turns.
        for form in [prefix.to_string(), format!("rtk {prefix}")] {
            tools.push(format!("Bash({form})"));
            tools.push(format!("Bash({form} *)"));
        }
    }
    if arm == Arm::A2 {
        tools.extend(MCP_TOOLS.iter().map(|t| t.to_string()));
    }
    tools
}

pub fn coaching(arm: Arm) -> &'static str {
    match arm {
        Arm::A0 => COACH_A0,
        Arm::A1 => COACH_A1,
        Arm::A2 => COACH_A2,
    }
}

pub fn run_task(
    cx: &Ctx,
    item: &PlanItem,
    repo: &RepoSpec,
    task: &TaskSpec,
    capture: Option<&CaptureArtifact>,
) -> Result<RunRecord> {
    let repo_dir = cx.repo_dir(repo);
    let work = cx.work.join(&item.run_id);
    if work.exists() {
        std::fs::remove_dir_all(&work)?;
    }
    git::clone(&repo_dir, &work)?;
    git::checkout(&work, &repo.commits.c3)?;
    std::fs::write(work.join(".git").join("info").join("exclude"), EXCLUDE)?;

    let mut notes = Vec::new();
    // Arm material: the seeded memories, exactly as captured.
    let mcp = match item.arm {
        Arm::A0 => None,
        Arm::A1 => {
            match capture {
                Some(c) if c.file.is_file() => {
                    files::copy_fresh(&c.file, &work.join("gotchas.md"))?
                }
                _ => notes.push("no gotchas.md was captured for this repo".into()),
            }
            None
        }
        Arm::A2 => {
            match capture {
                Some(c) if c.file.is_file() => {
                    files::copy_fresh(&c.file, &work.join("codegraph-memory.jsonl"))?
                }
                _ => notes.push("no memory log was captured for this repo".into()),
            }
            cx.build_index(&work)?;
            Some(cx.mcp_server(&work))
        }
    };

    let description = std::fs::read_to_string(repo_dir.join(&task.description))
        .with_context(|| format!("reading {}", task.description))?;
    let primary_stem = task.primary_test.trim_end_matches(".rs");
    let prompt = task_prompt(&description, item.arm, primary_stem);
    let tests_dir = work.join("tests");
    if cx.expose_primary_test {
        // The task's acceptance test is part of the request; the oracle is not.
        files::copy_fresh(
            &repo_dir.join(&task.primary_test),
            &tests_dir.join(&task.primary_test),
        )?;
    }

    let req = AgentRequest {
        phase: Phase::Task,
        capture_at: cx.capture_at,
        arm: item.arm,
        seed: item.seed,
        cwd: &work,
        prompt: prompt.clone(),
        allowed_tools: allowed_tools(item.arm),
        mcp,
        max_turns: cx.max_turns,
        time_cap: Duration::from_secs(cx.time_cap_secs),
        model: cx.model.clone(),
        budget_usd: cx.budget_usd,
        repo,
        repo_dir: &repo_dir,
        task: Some(task),
        codegraph_bin: &cx.codegraph_bin,
    };
    let outcome = cx.agent.run(&req)?;

    let transcripts = cx.out.join("transcripts");
    std::fs::create_dir_all(&transcripts)?;
    let transcript_path = transcripts.join(format!("{}.jsonl", item.run_id));
    std::fs::write(&transcript_path, &outcome.transcript)?;

    let parsed = transcript::parse(&outcome.transcript);
    let dirty = !git::run(&work, &["status", "--porcelain"], &[])?
        .trim()
        .is_empty();
    let instrumentation = transcript::instrument(&parsed, item.arm, repo, dirty);
    let false_confidence = transcript::false_confidence(&instrumentation);

    // Ground truth, (re)injected only after the agent is done: the agent may
    // have edited an exposed primary test, and never saw the oracle.
    files::copy_fresh(
        &repo_dir.join(&task.primary_test),
        &tests_dir.join(&task.primary_test),
    )?;
    files::copy_fresh(
        &repo_dir.join(&task.oracle_test),
        &tests_dir.join(&task.oracle_test),
    )?;
    let target_dir = cx.out.join("target").join(&repo.repo_id);
    let (primary, primary_tail) = cargo_test(
        &work,
        Some(task.primary_test.trim_end_matches(".rs")),
        &target_dir,
    )?;
    let (oracle, oracle_tail) = cargo_test(
        &work,
        Some(task.oracle_test.trim_end_matches(".rs")),
        &target_dir,
    )?;
    let fix_pass = primary == Outcome::Pass;
    let violation = match oracle {
        Outcome::Pass => Some(false),
        Outcome::Fail => Some(true),
        Outcome::CompileError => {
            notes.push("oracle did not compile (task API missing); violation not evaluable".into());
            None
        }
    };
    if !fix_pass {
        notes.push(format!(
            "primary suite {primary:?}: {}",
            last_line(&primary_tail)
        ));
    }
    if violation == Some(true) {
        notes.push(format!("oracle: {}", last_line(&oracle_tail)));
    }

    let tokens = HeuristicCounter.count(&prompt) + HeuristicCounter.count(&outcome.transcript);
    let usage = parsed.usage.clone().unwrap_or_default();
    let reported_tokens = match (usage.input_tokens, usage.output_tokens) {
        (None, None) => None,
        (i, o) => Some(i.unwrap_or(0) + o.unwrap_or(0)),
    };
    let turns_exhausted = usage.turns.is_some_and(|t| t >= u64::from(cx.max_turns));
    let cap_exhausted = outcome.timed_out
        || tokens > cx.token_cap
        || turns_exhausted
        || parsed.error.as_deref().is_some_and(|e| {
            let e = e.to_ascii_lowercase();
            e.contains("turn") || e.contains("budget")
        });
    if outcome.timed_out {
        notes.push("time cap reached; agent killed".into());
    }
    if tokens > cx.token_cap {
        notes.push(format!("token cap exceeded ({tokens} > {})", cx.token_cap));
    }
    if parsed.is_error {
        notes.push(format!(
            "agent reported an error: {}",
            parsed.error.as_deref().unwrap_or("(none)")
        ));
    } else if !outcome.exit_ok && !outcome.timed_out {
        notes.push("agent process did not exit cleanly".into());
    }

    if !cx.keep_work {
        let _ = std::fs::remove_dir_all(&work);
    }
    Ok(RunRecord {
        run_id: item.run_id.clone(),
        repo_id: repo.repo_id.clone(),
        task_id: task.task_id.clone(),
        arm: item.arm,
        seed: item.seed,
        drift: repo.drift,
        violation,
        fix_pass,
        tokens,
        reported_tokens,
        time_secs: outcome.elapsed.as_secs_f64(),
        cap_exhausted,
        timed_out: outcome.timed_out,
        instrumentation,
        false_confidence,
        transcript: Some(
            transcript_path
                .strip_prefix(&cx.out)
                .unwrap_or(&transcript_path)
                .to_string_lossy()
                .replace('\\', "/"),
        ),
        notes,
    })
}

fn last_line(text: &str) -> String {
    text.lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a2_gets_the_memory_tools_and_everyone_gets_git_log() {
        let a0 = allowed_tools(Arm::A0);
        let a1 = allowed_tools(Arm::A1);
        let a2 = allowed_tools(Arm::A2);
        assert_eq!(a0, a1);
        assert!(a0.contains(&"Bash(git log *)".to_string()));
        assert!(a0.contains(&"Bash(cargo test)".to_string()));
        assert!(!a0.iter().any(|t| t.starts_with("mcp__")));
        assert!(a2.contains(&"mcp__codegraph__recall".to_string()));
        assert!(!a0.iter().any(|t| t.contains("WebSearch")));
    }

    #[test]
    fn prompts_name_the_material_per_arm_and_the_primary_test_for_all() {
        let a0 = task_prompt("# Feature\n\ndo it\n", Arm::A0, "primary_test_1");
        let a1 = task_prompt("# Feature\n\ndo it\n", Arm::A1, "primary_test_1");
        let a2 = task_prompt("# Feature\n\ndo it\n", Arm::A2, "primary_test_1");
        for p in [&a0, &a1, &a2] {
            assert!(p.starts_with("Task: # Feature\n\ndo it\n\n"), "{p}");
            assert!(
                p.ends_with("Solve the task. Run: cargo test --test primary_test_1\n"),
                "{p}"
            );
        }
        assert!(a0.contains("Do NOT use codegraph, recall, remember"));
        assert!(!a0.contains("gotchas.md first") && !a0.contains("'recall'"));
        assert!(a1.contains("Read gotchas.md first") && !a1.contains("recall"));
        assert!(a2.contains("Use 'recall'") && !a2.contains("gotchas"));
    }

    #[test]
    fn last_line_skips_blanks() {
        assert_eq!(last_line("a\n\nb\n\n"), "b");
        assert_eq!(last_line(""), "");
    }
}
