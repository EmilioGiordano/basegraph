//! Smoke test of the experiment pipeline (goal deliverable 4): generate one
//! mini repo with verified ground truth, run one task through the three arms
//! with the scripted agent, and score the result.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

struct TempRoot(PathBuf);

impl TempRoot {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("cg_experiment_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        Self(dir)
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(bin: &str, args: &[&str]) -> Output {
    Command::new(bin).args(args).output().expect("spawn tool")
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn json_file(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[test]
fn pipeline_generates_runs_and_scores() {
    let root = TempRoot::new();
    let synth = root.0.join("synth");
    let supp = root.0.join("supplementary");
    let results = root.0.join("results");
    let s = |p: &Path| p.to_string_lossy().into_owned();

    // 1. Generator: one drifting repo, self-checked ground truth, versionable copy.
    let gen = run(
        env!("CARGO_BIN_EXE_synth_repo_gen"),
        &[
            "--out",
            &s(&synth),
            "--seed",
            "7",
            "--count",
            "1",
            "--drift",
            "1",
            "--verify",
            "--supplementary",
            &s(&supp),
        ],
    );
    assert!(
        gen.status.success(),
        "generator failed:\n{}",
        combined(&gen)
    );
    let manifest = json_file(&synth.join("manifest.json"));
    let repos = manifest["repos"].as_array().expect("repos");
    assert_eq!(repos.len(), 1);
    let repo = &repos[0];
    assert_eq!(repo["repo_id"], "repo_01");
    assert_eq!(repo["drift"], true);
    assert_eq!(repo["tasks"].as_array().unwrap().len(), 2);
    let commits = &repo["commits"];
    assert_ne!(commits["c1"], commits["c2"]);
    assert_ne!(commits["c2"], commits["c3"]);
    let files = repo["file_count"].as_u64().unwrap();
    assert!((20..=60).contains(&files), "{files} files");
    let repo_dir = synth.join("repo_01");
    assert!(repo_dir.join(".git").is_dir());
    assert!(repo_dir.join("task_1.md").is_file());
    assert!(repo_dir.join("oracle_test_1.rs").is_file());
    assert!(supp.join("bundles").join("repo_01.bundle").is_file());
    assert!(supp
        .join("ground_truth")
        .join("repo_01")
        .join("fix_wrong_2.rs")
        .is_file());
    assert!(supp.join("manifest.json").is_file());
    let report = json_file(&synth.join("verify_report.json"));
    assert!(report[0]["ok"].as_bool().unwrap(), "{report}");

    // Same seed, same materials (commit SHAs included).
    let again = root.0.join("synth_again");
    let gen2 = run(
        env!("CARGO_BIN_EXE_synth_repo_gen"),
        &[
            "--out",
            &s(&again),
            "--seed",
            "7",
            "--count",
            "1",
            "--drift",
            "1",
        ],
    );
    assert!(gen2.status.success(), "{}", combined(&gen2));
    assert_eq!(
        std::fs::read_to_string(synth.join("manifest.json")).unwrap(),
        std::fs::read_to_string(again.join("manifest.json")).unwrap()
    );

    // 2. Runner: one task, three arms, scripted agent (wrong / wrong-blind / correct).
    let runner = run(
        env!("CARGO_BIN_EXE_experiment_runner"),
        &[
            "--manifest",
            &s(&synth.join("manifest.json")),
            "--out",
            &s(&results),
            "--agent",
            "scripted:a0=wrong,a1=wrong-blind,a2=correct",
            "--seeds",
            "1",
            "--tasks",
            "task_1",
            "--codegraph-bin",
            env!("CARGO_BIN_EXE_codegraph"),
        ],
    );
    assert!(
        runner.status.success(),
        "runner failed:\n{}",
        combined(&runner)
    );
    let runs_text = std::fs::read_to_string(results.join("runs.jsonl")).expect("runs.jsonl");
    let runs: Vec<Value> = runs_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("run record"))
        .collect();
    assert_eq!(runs.len(), 3, "{runs_text}");
    let by_arm = |arm: &str| {
        runs.iter()
            .find(|r| r["arm"] == arm)
            .unwrap_or_else(|| panic!("no {arm} run"))
    };
    let a0 = by_arm("a0");
    assert_eq!(a0["violation"], true);
    assert_eq!(a0["fix_pass"], true);
    assert_eq!(a0["instrumentation"]["git_archaeology"], true);
    assert_eq!(a0["false_confidence"], false);
    let a1 = by_arm("a1");
    assert_eq!(a1["violation"], true);
    assert_eq!(a1["instrumentation"]["md_read"], true);
    assert_eq!(
        a1["instrumentation"]["stale_material_seen"], true,
        "gotchas.md is stale under drift"
    );
    assert_eq!(
        a1["false_confidence"], true,
        "blind wrong fix after stale material"
    );
    let a2 = by_arm("a2");
    assert_eq!(a2["violation"], false);
    assert_eq!(a2["fix_pass"], true);
    assert_eq!(a2["instrumentation"]["memory_consulted"], true);
    assert!(!a2["instrumentation"]["memory_statuses"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(a2["false_confidence"], false);
    for r in &runs {
        assert_eq!(r["drift"], true);
        assert_eq!(r["cap_exhausted"], false);
        let transcript = results.join(r["transcript"].as_str().unwrap());
        assert!(transcript.is_file(), "{}", transcript.display());
    }
    let captures = json_file(&results.join("captures.json"));
    let captures = captures.as_array().unwrap();
    assert_eq!(captures.len(), 2);
    assert!(captures.iter().all(|c| c["usable"] == true), "{captures:?}");
    assert!(results
        .join("captures")
        .join("repo_01")
        .join("a2")
        .join("codegraph-memory.jsonl")
        .is_file());

    // Resumable: a second invocation has nothing left to do.
    let resumed = run(
        env!("CARGO_BIN_EXE_experiment_runner"),
        &[
            "--manifest",
            &s(&synth.join("manifest.json")),
            "--out",
            &s(&results),
            "--agent",
            "scripted:all=correct",
            "--seeds",
            "1",
            "--tasks",
            "task_1",
            "--codegraph-bin",
            env!("CARGO_BIN_EXE_codegraph"),
        ],
    );
    assert!(resumed.status.success(), "{}", combined(&resumed));
    assert!(
        combined(&resumed).contains("3 done, 0 pending"),
        "{}",
        combined(&resumed)
    );
    assert_eq!(
        std::fs::read_to_string(results.join("runs.jsonl")).unwrap(),
        runs_text
    );

    // 3. Scorer: summary and verdict against the sealed thresholds.
    let scorer = run(
        env!("CARGO_BIN_EXE_scorer"),
        &["--runs", &s(&results.join("runs.jsonl"))],
    );
    assert!(
        scorer.status.success(),
        "scorer failed:\n{}",
        combined(&scorer)
    );
    let verdict = std::fs::read_to_string(results.join("verdict.txt")).expect("verdict.txt");
    let first = verdict.lines().next().unwrap_or_default();
    assert!(["GO", "NO-GO", "GREY"].contains(&first), "{verdict}");
    let summary = json_file(&results.join("summary.json"));
    assert_eq!(summary["runs"], 3);
    assert_eq!(summary["tasks"], 1);
    assert_eq!(summary["drift_tasks"], 1);
    assert_eq!(summary["primary"]["all"]["a2"]["violation"]["hits"], 0);
    assert_eq!(summary["primary"]["all"]["a0"]["violation"]["hits"], 1);
    assert_eq!(
        summary["primary"]["drift"]["a1"]["false_confidence"]["hits"],
        1
    );
    // A one-run difference is the grey zone by construction.
    assert_eq!(first, "GREY", "{verdict}");
}
