//! The run plan: every repo × task × arm × seed, in a seeded random order
//! (go-no-go.md §5.5), minus the runs already recorded in `runs.jsonl`.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};

use crate::rng::Rng;
use crate::schema::{Arm, Manifest, RunRecord};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanItem {
    pub run_id: String,
    pub repo_id: String,
    pub task_id: String,
    pub arm: Arm,
    pub seed: u32,
}

pub fn run_id(repo_id: &str, task_id: &str, arm: Arm, seed: u32) -> String {
    format!("{repo_id}-{task_id}-{}-s{seed}", arm.label())
}

pub fn build(
    manifest: &Manifest,
    arms: &[Arm],
    seeds: u32,
    repos: Option<&[String]>,
    tasks: Option<&[String]>,
    order_seed: u64,
) -> Vec<PlanItem> {
    let mut items = Vec::new();
    for repo in &manifest.repos {
        if repos.is_some_and(|list| !list.contains(&repo.repo_id)) {
            continue;
        }
        for task in &repo.tasks {
            if tasks.is_some_and(|list| !list.contains(&task.task_id)) {
                continue;
            }
            for &arm in arms {
                for seed in 0..seeds {
                    items.push(PlanItem {
                        run_id: run_id(&repo.repo_id, &task.task_id, arm, seed),
                        repo_id: repo.repo_id.clone(),
                        task_id: task.task_id.clone(),
                        arm,
                        seed,
                    });
                }
            }
        }
    }
    Rng::new(order_seed).shuffle(&mut items);
    items
}

/// Run ids already present in `runs.jsonl` (a missing file is an empty log).
pub fn completed(runs_path: &Path) -> Result<BTreeSet<String>> {
    let text = match std::fs::read_to_string(runs_path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", runs_path.display())),
    };
    let mut done = BTreeSet::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let record: RunRecord = serde_json::from_str(line)
            .with_context(|| format!("malformed run record in {}", runs_path.display()))?;
        done.insert(record.run_id);
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Commits, RepoSpec, TaskSpec, MANIFEST_VERSION};

    fn manifest() -> Manifest {
        let task = |n: usize| TaskSpec {
            task_id: format!("task_{n}"),
            title: String::new(),
            description: format!("task_{n}.md"),
            primary_test: format!("primary_test_{n}.rs"),
            oracle_test: format!("oracle_test_{n}.rs"),
            fix_correct: format!("fix_correct_{n}.rs"),
            fix_wrong: format!("fix_wrong_{n}.rs"),
            fix_target: "src/x.rs".into(),
        };
        let repo = |id: &str, drift: bool| RepoSpec {
            repo_id: id.into(),
            path: id.into(),
            crate_name: "demo".into(),
            scenario: "s".into(),
            invariant_type: "s".into(),
            invariant_text: String::new(),
            anchor_fqn_c2: "f".into(),
            anchor_fqn_c3: "f".into(),
            anchor_file_c2: "src/x.rs".into(),
            anchor_file_c3: "src/x.rs".into(),
            drift,
            drift_kind: None,
            file_count: 1,
            commits: Commits {
                c1: "a".into(),
                c2: "b".into(),
                c3: "c".into(),
            },
            capture_task: "capture_task.md".into(),
            tasks: vec![task(1), task(2)],
        };
        Manifest {
            version: MANIFEST_VERSION,
            seed: 1,
            repos: vec![repo("repo_01", true), repo("repo_02", false)],
        }
    }

    #[test]
    fn full_plan_covers_every_combination_in_seeded_order() {
        let m = manifest();
        let plan = build(&m, &Arm::ALL, 2, None, None, 7);
        assert_eq!(plan.len(), 2 * 2 * 3 * 2);
        let ids: BTreeSet<&str> = plan.iter().map(|p| p.run_id.as_str()).collect();
        assert_eq!(ids.len(), plan.len(), "run ids are unique");
        assert!(ids.contains("repo_02-task_1-a1-s1"));
        assert_eq!(plan, build(&m, &Arm::ALL, 2, None, None, 7));
        assert_ne!(plan, build(&m, &Arm::ALL, 2, None, None, 8));
    }

    #[test]
    fn filters_restrict_repos_tasks_and_arms() {
        let m = manifest();
        let plan = build(
            &m,
            &[Arm::A2],
            1,
            Some(&["repo_01".to_string()]),
            Some(&["task_2".to_string()]),
            1,
        );
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].run_id, "repo_01-task_2-a2-s0");
    }

    #[test]
    fn completed_reads_run_ids_and_tolerates_a_missing_log() {
        let dir = std::env::temp_dir().join(format!("cg_plan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("runs.jsonl");
        assert!(completed(&path).expect("missing is empty").is_empty());
        let record = RunRecord {
            run_id: "r1".into(),
            repo_id: "repo_01".into(),
            task_id: "task_1".into(),
            arm: Arm::A0,
            seed: 0,
            drift: false,
            violation: None,
            fix_pass: false,
            tokens: 0,
            reported_tokens: None,
            time_secs: 0.0,
            cap_exhausted: false,
            timed_out: false,
            instrumentation: Default::default(),
            false_confidence: false,
            transcript: None,
            notes: vec![],
        };
        std::fs::write(
            &path,
            format!("{}\n\n", serde_json::to_string(&record).unwrap()),
        )
        .unwrap();
        let done = completed(&path).expect("read");
        assert_eq!(done.len(), 1);
        assert!(done.contains("r1"));
        std::fs::write(&path, "not json\n").unwrap();
        assert!(completed(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
