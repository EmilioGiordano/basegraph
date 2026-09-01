//! Data contracts shared by the generator, the runner and the scorer:
//! `manifest.json` (materials) and `runs.jsonl` (one record per run).
//!
//! Included by each tool with `#[path]`; it must stay dependency-light and
//! free of tool-specific logic.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

pub const MANIFEST_VERSION: u32 = 1;

/// The three arms of the protocol (go-no-go.md §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arm {
    A0,
    A1,
    A2,
}

impl Arm {
    pub const ALL: [Arm; 3] = [Arm::A0, Arm::A1, Arm::A2];

    pub fn label(self) -> &'static str {
        match self {
            Arm::A0 => "a0",
            Arm::A1 => "a1",
            Arm::A2 => "a2",
        }
    }

    pub fn parse(s: &str) -> Option<Arm> {
        match s.trim().to_ascii_lowercase().as_str() {
            "a0" => Some(Arm::A0),
            "a1" => Some(Arm::A1),
            "a2" => Some(Arm::A2),
            _ => None,
        }
    }
}

/// How C3 moved the anchored symbol (drift repos only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftKind {
    /// The function keeps its module but gets a new name.
    Rename,
    /// The whole module is moved to a new file (module path changes).
    Move,
    /// Signature-only change: the parameter is renamed.
    Signature,
    /// The function is gone: renamed AND re-parameterised at once, so no
    /// hash matches and the old anchor is orphaned.
    Delete,
    /// Legacy (pilot seed 123 manifests only): a same-named wrapper in a
    /// second module. No longer generated — free-function fqns are bare, so
    /// the duplicate collided with the original and `recall` stayed intact.
    Duplicate,
}

impl DriftKind {
    /// The kinds the generator emits.
    pub const ALL: [DriftKind; 4] = [
        DriftKind::Rename,
        DriftKind::Move,
        DriftKind::Signature,
        DriftKind::Delete,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commits {
    pub c1: String,
    pub c2: String,
    pub c3: String,
}

/// One task of a repo, with its pre-written ground truth. Paths are relative
/// to the repo directory; none of these files is committed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSpec {
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub primary_test: String,
    pub oracle_test: String,
    pub fix_correct: String,
    pub fix_wrong: String,
    /// The file the reference fixes replace (relative to the repo).
    pub fix_target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoSpec {
    pub repo_id: String,
    /// Directory of the repo, relative to the manifest's directory.
    pub path: String,
    pub crate_name: String,
    pub scenario: String,
    pub invariant_type: String,
    pub invariant_text: String,
    /// Symbol identity as codegraph sees it, at C2 (when memories are seeded)
    /// and at C3 (when tasks run).
    pub anchor_fqn_c2: String,
    pub anchor_fqn_c3: String,
    pub anchor_file_c2: String,
    pub anchor_file_c3: String,
    pub drift: bool,
    pub drift_kind: Option<DriftKind>,
    pub file_count: usize,
    pub commits: Commits,
    /// The bug report C2 fixed; drives the memory-seeding sessions (§4).
    pub capture_task: String,
    pub tasks: Vec<TaskSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub seed: u64,
    pub repos: Vec<RepoSpec>,
}

impl Manifest {
    pub fn drift_repos(&self) -> impl Iterator<Item = &RepoSpec> {
        self.repos.iter().filter(|r| r.drift)
    }
}

/// Which arm-specific material the agent actually used (go-no-go.md §5.4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instrumentation {
    pub tool_calls: usize,
    /// A2: `recall` was called at least once.
    pub memory_consulted: bool,
    /// A1: `gotchas.md` was read.
    pub md_read: bool,
    /// A0 (recorded for every arm): `git log|show|blame|diff` was run.
    pub git_archaeology: bool,
    /// A2: anchor statuses reported by `recall`, in order.
    pub memory_statuses: Vec<String>,
    /// Stale material was served: A2 recall returned a non-intact anchor, or
    /// A1 read the .md in a drift repo (stale by construction).
    pub stale_material_seen: bool,
    /// After seeing stale material the agent looked at the current code of the
    /// anchored symbol (read its file, or called show/context on it).
    pub verified_after_stale: bool,
    /// The agent had already looked at the anchored symbol's current code in
    /// the verification window (the 3 tool calls before the stale item) —
    /// rubric refinement from the seed-123 pilot.
    #[serde(default)]
    pub verified_before_stale: bool,
    /// The agent modified the working tree.
    pub edited: bool,
}

/// One line of `runs.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub run_id: String,
    pub repo_id: String,
    pub task_id: String,
    pub arm: Arm,
    pub seed: u32,
    pub drift: bool,
    /// Primary metric: the oracle test failed (invariant violated). `None`
    /// when the oracle could not be evaluated because the primary fix does
    /// not compile/pass — counted as a primary failure, not a violation.
    pub violation: Option<bool>,
    pub fix_pass: bool,
    /// Heuristic tokens (chars/4) over prompt + transcript, as in codegraph.
    pub tokens: usize,
    /// Tokens reported by the agent CLI, when available.
    pub reported_tokens: Option<u64>,
    pub time_secs: f64,
    pub cap_exhausted: bool,
    pub timed_out: bool,
    pub instrumentation: Instrumentation,
    /// Rubric (go-no-go.md §8): acted on stale material without verifying.
    /// Auto-scored by the runner; the scorer accepts manual overrides.
    pub false_confidence: bool,
    pub transcript: Option<String>,
    pub notes: Vec<String>,
}

impl RunRecord {
    pub fn violated(&self) -> bool {
        self.violation == Some(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arm_roundtrip_and_parse() {
        for arm in Arm::ALL {
            let json = serde_json::to_string(&arm).expect("serialize");
            assert_eq!(json, format!("\"{}\"", arm.label()));
            assert_eq!(
                serde_json::from_str::<Arm>(&json).expect("deserialize"),
                arm
            );
            assert_eq!(Arm::parse(arm.label()), Some(arm));
            assert_eq!(Arm::parse(&arm.label().to_uppercase()), Some(arm));
        }
        assert_eq!(Arm::parse("a3"), None);
    }

    #[test]
    fn drift_kind_serializes_snake_case() {
        let json = serde_json::to_string(&DriftKind::Signature).expect("serialize");
        assert_eq!(json, "\"signature\"");
        assert_eq!(
            serde_json::to_string(&DriftKind::Delete).expect("serialize"),
            "\"delete\""
        );
        // Old pilot manifests still deserialize, but duplicate is not emitted.
        let legacy: DriftKind = serde_json::from_str("\"duplicate\"").expect("deserialize");
        assert_eq!(legacy, DriftKind::Duplicate);
        assert!(!DriftKind::ALL.contains(&DriftKind::Duplicate));
        assert!(DriftKind::ALL.contains(&DriftKind::Delete));
    }

    #[test]
    fn run_record_roundtrip() {
        let record = RunRecord {
            run_id: "repo_01-task_1-a2-s0".into(),
            repo_id: "repo_01".into(),
            task_id: "task_1".into(),
            arm: Arm::A2,
            seed: 0,
            drift: true,
            violation: Some(false),
            fix_pass: true,
            tokens: 1234,
            reported_tokens: Some(2000),
            time_secs: 12.5,
            cap_exhausted: false,
            timed_out: false,
            instrumentation: Instrumentation {
                tool_calls: 7,
                memory_consulted: true,
                memory_statuses: vec!["evolved".into()],
                stale_material_seen: true,
                verified_after_stale: true,
                edited: true,
                ..Instrumentation::default()
            },
            false_confidence: false,
            transcript: Some("transcripts/x.jsonl".into()),
            notes: vec![],
        };
        let line = serde_json::to_string(&record).expect("serialize");
        let back: RunRecord = serde_json::from_str(&line).expect("deserialize");
        assert_eq!(back, record);
        assert!(!back.violated());
    }
}
