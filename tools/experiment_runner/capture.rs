//! Memory seeding through the real pipeline (go-no-go.md §4): a separate
//! agent session at C1 solves the bug C2 fixed, then distills what it learned
//! into `gotchas.md` (A1) or `remember` (A2). Nothing is hand-edited; raw
//! artifacts are kept and their usability is reported.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::agent::{AgentRequest, Phase};
use crate::files;
use crate::git;
use crate::schema::{Arm, RepoSpec};
use crate::Ctx;

pub const CAPTURE_SUFFIX: &str = "\n\nFix this bug in the repository you are in and verify with `cargo test`. Work autonomously and do not ask questions.";
pub const CAPTURE_A1: &str = "\n\nWhen the fix is done, write what a future maintainer must know about this code into `gotchas.md` at the repository root: short, concrete bullets naming the functions involved.";
pub const CAPTURE_A2: &str = "\n\nWhen the fix is done, record what a future maintainer must know with the `remember` tool, anchored to the function involved (kind: invariant, gotcha or decision), one memory per fact.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureArtifact {
    pub repo_id: String,
    pub arm: Arm,
    /// `gotchas.md` (A1) or `codegraph-memory.jsonl` (A2), copied out of the
    /// seeding session's clone.
    pub file: PathBuf,
    pub usable: bool,
    /// Memories recorded (A2) or non-empty lines (A1).
    pub entries: usize,
    pub transcript: Option<PathBuf>,
    pub time_secs: f64,
    pub notes: Vec<String>,
}

pub fn material_name(arm: Arm) -> Option<&'static str> {
    match arm {
        Arm::A0 => None,
        Arm::A1 => Some("gotchas.md"),
        Arm::A2 => Some("codegraph-memory.jsonl"),
    }
}

fn count_entries(arm: Arm, text: &str) -> usize {
    match arm {
        Arm::A0 => 0,
        Arm::A1 => text.lines().filter(|l| !l.trim().is_empty()).count(),
        Arm::A2 => text.lines().filter(|l| l.contains("\"Created\"")).count(),
    }
}

/// Seed the memory material for `(repo, arm)`, reusing an existing capture
/// under `<out>/captures/` unless `force` is set.
pub fn seed_memories(cx: &Ctx, repo: &RepoSpec, arm: Arm, force: bool) -> Result<CaptureArtifact> {
    let material = material_name(arm).context("A0 has no memory material")?;
    let dir = cx
        .out
        .join("captures")
        .join(&repo.repo_id)
        .join(arm.label());
    let file = dir.join(material);
    let meta = dir.join("capture.json");
    if !force && meta.is_file() {
        let text = std::fs::read_to_string(&meta)?;
        if let Ok(existing) = serde_json::from_str::<CaptureArtifact>(&text) {
            return Ok(existing);
        }
    }
    std::fs::create_dir_all(&dir)?;

    let work = cx
        .out
        .join("work")
        .join(format!("capture-{}-{}", repo.repo_id, arm.label()));
    if work.exists() {
        std::fs::remove_dir_all(&work)?;
    }
    let repo_dir = cx.repo_dir(repo);
    git::clone(&repo_dir, &work)?;
    git::checkout(&work, &repo.commits.c1)?;
    std::fs::write(
        work.join(".git").join("info").join("exclude"),
        crate::run::EXCLUDE,
    )?;

    let mut prompt = std::fs::read_to_string(repo_dir.join(&repo.capture_task))
        .with_context(|| format!("reading {}", repo.capture_task))?;
    prompt.push_str(CAPTURE_SUFFIX);
    let mcp = match arm {
        Arm::A1 => {
            prompt.push_str(CAPTURE_A1);
            None
        }
        Arm::A2 => {
            prompt.push_str(CAPTURE_A2);
            cx.build_index(&work)?;
            Some(cx.mcp_server(&work))
        }
        Arm::A0 => None,
    };
    let req = AgentRequest {
        phase: Phase::Capture,
        arm,
        seed: 0,
        cwd: &work,
        prompt,
        allowed_tools: crate::run::allowed_tools(arm),
        mcp,
        max_turns: cx.max_turns,
        time_cap: Duration::from_secs(cx.time_cap_secs),
        model: cx.model.clone(),
        budget_usd: cx.budget_usd,
        repo,
        repo_dir: &repo_dir,
        task: None,
        codegraph_bin: &cx.codegraph_bin,
    };
    let outcome = cx.agent.run(&req)?;
    let transcript_path = dir.join("transcript.jsonl");
    std::fs::write(&transcript_path, &outcome.transcript)?;

    let mut notes = Vec::new();
    if outcome.timed_out {
        notes.push("seeding session hit the time cap".to_string());
    }
    if !outcome.exit_ok {
        notes.push("agent process did not exit cleanly".to_string());
    }
    let produced = work.join(material);
    let (usable, entries) = if produced.is_file() {
        files::copy_fresh(&produced, &file)?;
        let text = std::fs::read_to_string(&file)?;
        let entries = count_entries(arm, &text);
        (entries > 0, entries)
    } else {
        notes.push(format!("agent produced no {material}"));
        (false, 0)
    };
    let artifact = CaptureArtifact {
        repo_id: repo.repo_id.clone(),
        arm,
        file,
        usable,
        entries,
        transcript: Some(transcript_path),
        time_secs: outcome.elapsed.as_secs_f64(),
        notes,
    };
    std::fs::write(&meta, serde_json::to_string_pretty(&artifact)?)?;
    if !cx.keep_work {
        let _ = std::fs::remove_dir_all(&work);
    }
    Ok(artifact)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_counted_per_material() {
        assert_eq!(count_entries(Arm::A1, "# Gotchas\n\n- one\n- two\n"), 3);
        assert_eq!(
            count_entries(
                Arm::A2,
                "{\"event\":{\"Created\":{}}}\n{\"event\":{\"Reanchored\":{}}}\n"
            ),
            1
        );
        assert_eq!(count_entries(Arm::A0, "anything"), 0);
        assert_eq!(material_name(Arm::A2), Some("codegraph-memory.jsonl"));
        assert_eq!(material_name(Arm::A0), None);
    }
}
