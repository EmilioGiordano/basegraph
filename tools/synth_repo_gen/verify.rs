//! Self-check of the generated ground truth (go-no-go.md §0 and §3): the
//! invariant is latent, the reference fix satisfies the task and keeps it,
//! the obvious wrong fix satisfies the task and breaks it, and the oracle
//! detects exactly that.

use std::path::Path;

use anyhow::Result;
use serde::Serialize;

use crate::cargo::{cargo_test, Outcome};
use crate::files;
use crate::git;
use crate::schema::RepoSpec;

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub task_id: String,
    pub state: String,
    pub target: String,
    pub expect: String,
    pub outcome: Outcome,
    pub ok: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoReport {
    pub repo_id: String,
    pub ok: bool,
    pub checks: Vec<Check>,
}

fn check(
    checks: &mut Vec<Check>,
    task_id: &str,
    state: &str,
    target: &str,
    expect: &str,
    accept: impl Fn(Outcome) -> bool,
    result: (Outcome, String),
) {
    let ok = accept(result.0);
    checks.push(Check {
        task_id: task_id.to_string(),
        state: state.to_string(),
        target: target.to_string(),
        expect: expect.to_string(),
        outcome: result.0,
        ok,
        detail: if ok { String::new() } else { result.1 },
    });
}

/// Clone the repo into `work` and exercise every task through its three
/// states: pristine, reference fix, obvious wrong fix.
pub fn verify(spec: &RepoSpec, repo_dir: &Path, work: &Path) -> Result<RepoReport> {
    if work.exists() {
        std::fs::remove_dir_all(work)?;
    }
    std::fs::create_dir_all(work)?;
    let clone = work.join("repo");
    git::clone(repo_dir, &clone)?;
    let target_dir = work.join("target");
    let tests_dir = clone.join("tests");
    let mut checks = Vec::new();

    check(
        &mut checks,
        "-",
        "pristine",
        "own suite",
        "pass",
        |o| o == Outcome::Pass,
        cargo_test(&clone, None, &target_dir)?,
    );

    for task in &spec.tasks {
        let primary = task.primary_test.trim_end_matches(".rs");
        let oracle = task.oracle_test.trim_end_matches(".rs");
        files::copy_fresh(
            &repo_dir.join(&task.primary_test),
            &tests_dir.join(&task.primary_test),
        )?;
        files::copy_fresh(
            &repo_dir.join(&task.oracle_test),
            &tests_dir.join(&task.oracle_test),
        )?;

        // Pristine: the task is not done yet; the invariant holds (or the
        // oracle needs the task's API and cannot compile yet).
        check(
            &mut checks,
            &task.task_id,
            "pristine",
            primary,
            "not pass",
            |o| o != Outcome::Pass,
            cargo_test(&clone, Some(primary), &target_dir)?,
        );
        check(
            &mut checks,
            &task.task_id,
            "pristine",
            oracle,
            "pass or compile error",
            |o| o != Outcome::Fail,
            cargo_test(&clone, Some(oracle), &target_dir)?,
        );

        // Reference fix: task done, invariant kept, own suite still green.
        files::copy_fresh(
            &repo_dir.join(&task.fix_correct),
            &clone.join(&task.fix_target),
        )?;
        check(
            &mut checks,
            &task.task_id,
            "fix_correct",
            primary,
            "pass",
            |o| o == Outcome::Pass,
            cargo_test(&clone, Some(primary), &target_dir)?,
        );
        check(
            &mut checks,
            &task.task_id,
            "fix_correct",
            oracle,
            "pass",
            |o| o == Outcome::Pass,
            cargo_test(&clone, Some(oracle), &target_dir)?,
        );
        check(
            &mut checks,
            &task.task_id,
            "fix_correct",
            "own suite",
            "pass",
            |o| o == Outcome::Pass,
            cargo_test(&clone, None, &target_dir)?,
        );

        // Obvious wrong fix: task done, invariant broken, and the oracle is
        // what catches it (a compile error would be a generator bug).
        files::copy_fresh(
            &repo_dir.join(&task.fix_wrong),
            &clone.join(&task.fix_target),
        )?;
        check(
            &mut checks,
            &task.task_id,
            "fix_wrong",
            primary,
            "pass",
            |o| o == Outcome::Pass,
            cargo_test(&clone, Some(primary), &target_dir)?,
        );
        check(
            &mut checks,
            &task.task_id,
            "fix_wrong",
            oracle,
            "fail",
            |o| o == Outcome::Fail,
            cargo_test(&clone, Some(oracle), &target_dir)?,
        );

        git::restore(&clone)?;
        std::fs::remove_dir_all(&tests_dir)?;
    }

    let ok = checks.iter().all(|c| c.ok);
    Ok(RepoReport {
        repo_id: spec.repo_id.clone(),
        ok,
        checks,
    })
}
