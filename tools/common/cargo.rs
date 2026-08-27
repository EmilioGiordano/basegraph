//! Running `cargo test` on a generated repo and classifying the outcome.

#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
    CompileError,
}

pub fn cargo_bin() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string())
}

/// `cargo test [--test target]` in `dir` with an isolated target directory
/// (sharing the outer build's would deadlock on cargo's lock). Returns the
/// classified outcome and the tail of the combined output.
pub fn cargo_test(
    dir: &Path,
    target: Option<&str>,
    target_dir: &Path,
) -> Result<(Outcome, String)> {
    let mut cmd = Command::new(cargo_bin());
    cmd.arg("test").arg("--offline").arg("--quiet");
    if let Some(t) = target {
        cmd.arg("--test").arg(t);
    }
    let output = cmd
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", target_dir)
        .output()
        .with_context(|| format!("running cargo test in {}", dir.display()))?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let outcome = if output.status.success() {
        Outcome::Pass
    } else if stderr.contains("could not compile") || stderr.contains("error[E") {
        Outcome::CompileError
    } else {
        Outcome::Fail
    };
    let combined = format!("{stdout}{stderr}");
    let lines: Vec<&str> = combined.lines().collect();
    let tail = lines[lines.len().saturating_sub(15)..].join("\n");
    Ok((outcome, tail))
}
