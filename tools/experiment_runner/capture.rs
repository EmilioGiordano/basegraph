//! Memory seeding through the real pipeline (go-no-go.md §4): a separate
//! agent session, either at C1 (solves the bug C2 fixed, then distills) or at
//! C2 (the fix is applied; infers the latent invariant it relies on), writes
//! what it learned into `gotchas.md` (A1) or through `remember` (A2). Nothing
//! is hand-edited; raw artifacts are kept and their usability is reported.

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::agent::{AgentRequest, CaptureAt, Phase};
use crate::files;
use crate::git;
use crate::schema::{Arm, RepoSpec};
use crate::Ctx;

// Capture at C1: the session solves the bug first.
pub const CAPTURE_C1_SUFFIX: &str = "\n\nFix this bug in the repository you are in and verify with `cargo test`. Work autonomously and do not ask questions.";
pub const CAPTURE_C1_A1: &str = "\n\nWhen the fix is done, write what a future maintainer must know about this code into `gotchas.md` at the repository root: short, concrete bullets naming the functions involved.";
pub const CAPTURE_C1_A2: &str = "\n\nWhen the fix is done, record what a future maintainer must know with the `remember` tool, anchored to the function involved (kind: invariant, gotcha or decision), one memory per fact.";

// Capture at C2: the fix is applied; the session infers the invariant.
pub const CAPTURE_C2_A1: &str = "You are in the codebase at commit C2 (fix just applied).\n\
The bug is fixed. There is a LATENT INVARIANT that the fix relies on.\n\
Read the code around the changed symbol. Understand what invariant the fix preserves.\n\
Write a concise gotchas.md in the repo root documenting that invariant.\n\
Format: one line per gotcha, reference the symbol (fqn).\n\
Example:\n  auth::validate_token — NEVER log raw token, use hash_token(t) for logs.\n";
pub const CAPTURE_C2_A2: &str = "You are in the codebase at commit C2 (fix just applied).\n\
The bug is fixed. There is a LATENT INVARIANT that the fix relies on.\n\
Read the code around the changed symbol. Understand what invariant the fix preserves.\n\
Use the 'remember' tool to record it anchored to that symbol: anchor = the symbol's name exactly as codegraph indexes it (a free function by its bare name, e.g. 'compute'; a method as 'Type::method'), kind = invariant, content = the invariant in natural language.\n\
One memory per gotcha.\n";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureArtifact {
    pub repo_id: String,
    pub arm: Arm,
    /// Commit the seeding session ran at.
    #[serde(default = "default_capture_at")]
    pub captured_at: String,
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

fn default_capture_at() -> String {
    "c1".into()
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

pub fn capture_prompt(arm: Arm, at: CaptureAt, bug_report: &str) -> String {
    match at {
        CaptureAt::C1 => {
            let mut prompt = bug_report.to_string();
            prompt.push_str(CAPTURE_C1_SUFFIX);
            match arm {
                Arm::A1 => prompt.push_str(CAPTURE_C1_A1),
                Arm::A2 => prompt.push_str(CAPTURE_C1_A2),
                Arm::A0 => {}
            }
            prompt
        }
        CaptureAt::C2 => match arm {
            Arm::A1 => CAPTURE_C2_A1.to_string(),
            Arm::A2 => CAPTURE_C2_A2.to_string(),
            Arm::A0 => String::new(),
        },
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
        .work
        .join(format!("capture-{}-{}", repo.repo_id, arm.label()));
    if work.exists() {
        std::fs::remove_dir_all(&work)?;
    }
    let repo_dir = cx.repo_dir(repo);
    git::clone(&repo_dir, &work)?;
    let rev = match cx.capture_at {
        CaptureAt::C1 => &repo.commits.c1,
        CaptureAt::C2 => &repo.commits.c2,
    };
    git::checkout(&work, rev)?;
    std::fs::write(
        work.join(".git").join("info").join("exclude"),
        crate::run::EXCLUDE,
    )?;

    let bug_report = std::fs::read_to_string(repo_dir.join(&repo.capture_task))
        .with_context(|| format!("reading {}", repo.capture_task))?;
    let prompt = capture_prompt(arm, cx.capture_at, &bug_report);
    let mcp = match arm {
        Arm::A2 => {
            cx.build_index(&work)?;
            Some(cx.mcp_server(&work))
        }
        Arm::A0 | Arm::A1 => None,
    };
    let req = AgentRequest {
        phase: Phase::Capture,
        capture_at: cx.capture_at,
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
        captured_at: cx.capture_at.label().into(),
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

    #[test]
    fn capture_prompts_follow_the_commit_and_the_arm() {
        let c1 = capture_prompt(Arm::A2, CaptureAt::C1, "# Bug\nrepro");
        assert!(c1.starts_with("# Bug\nrepro"));
        assert!(c1.contains("remember"));
        let c2 = capture_prompt(Arm::A2, CaptureAt::C2, "# Bug\nrepro");
        assert!(
            !c2.contains("repro"),
            "at C2 the bug report is not handed over"
        );
        assert!(c2.contains("LATENT INVARIANT") && c2.contains("remember"));
        let md = capture_prompt(Arm::A1, CaptureAt::C2, "");
        assert!(md.contains("gotchas.md") && !md.contains("remember"));
        assert_eq!(CaptureAt::parse("C2"), Some(CaptureAt::C2));
        assert_eq!(CaptureAt::parse("c9"), None);
    }
}
