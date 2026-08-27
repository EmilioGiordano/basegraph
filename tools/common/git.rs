//! Git plumbing for scripted history. Author, committer and dates are fixed
//! so the same seed yields the same commit SHAs on every machine.

#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

const AUTHOR: &str = "Synth Bot";
const EMAIL: &str = "synth@example.invalid";
/// 2026-01-05T10:00:00Z; each commit is one day later than the previous.
const BASE_EPOCH: u64 = 1_767_607_200;
const DAY: u64 = 86_400;

pub fn run(dir: &Path, args: &[&str], envs: &[(&str, String)]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(args).current_dir(dir);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let output = cmd
        .output()
        .with_context(|| format!("running git {}", args.join(" ")))?;
    if !output.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn init(dir: &Path) -> Result<()> {
    run(dir, &["init", "-q", "-b", "main"], &[])?;
    run(dir, &["config", "core.autocrlf", "false"], &[])?;
    Ok(())
}

/// Stage everything and commit with fixed identity and a date derived from
/// `index`. Returns the new HEAD.
pub fn commit_all(dir: &Path, message: &str, index: u64) -> Result<String> {
    run(dir, &["add", "-A"], &[])?;
    let date = format!("@{} +0000", BASE_EPOCH + index * DAY);
    let envs = [
        ("GIT_AUTHOR_NAME", AUTHOR.to_string()),
        ("GIT_AUTHOR_EMAIL", EMAIL.to_string()),
        ("GIT_AUTHOR_DATE", date.clone()),
        ("GIT_COMMITTER_NAME", AUTHOR.to_string()),
        ("GIT_COMMITTER_EMAIL", EMAIL.to_string()),
        ("GIT_COMMITTER_DATE", date),
    ];
    run(
        dir,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "--no-verify",
            "--allow-empty",
            "-m",
            message,
        ],
        &envs,
    )?;
    head(dir)
}

pub fn head(dir: &Path) -> Result<String> {
    Ok(run(dir, &["rev-parse", "HEAD"], &[])?.trim().to_string())
}

pub fn bundle(dir: &Path, out: &Path) -> Result<()> {
    let out = out.to_string_lossy().into_owned();
    run(dir, &["bundle", "create", &out, "--all"], &[])?;
    Ok(())
}

pub fn clone(src: &Path, dst: &Path) -> Result<()> {
    let output = Command::new("git")
        .args([
            "clone",
            "-q",
            "--no-hardlinks",
            "--config",
            "core.autocrlf=false",
        ])
        .arg(src)
        .arg(dst)
        .output()
        .context("running git clone")?;
    if !output.status.success() {
        bail!(
            "git clone {} failed: {}",
            src.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

pub fn checkout(dir: &Path, rev: &str) -> Result<()> {
    run(dir, &["checkout", "-q", rev], &[])?;
    Ok(())
}

/// Discard every change to tracked files.
pub fn restore(dir: &Path) -> Result<()> {
    run(dir, &["checkout", "-q", "--", "."], &[])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("cg_gen_git_{tag}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("temp dir");
            Self(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn commits_are_reproducible() {
        let shas: Vec<String> = (0..2)
            .map(|i| {
                let dir = TempDir::new(&format!("repro{i}"));
                init(&dir.0).expect("init");
                std::fs::write(dir.0.join("a.txt"), "hello\n").expect("write");
                let first = commit_all(&dir.0, "first", 0).expect("commit");
                std::fs::write(dir.0.join("a.txt"), "hello again\n").expect("write");
                let second = commit_all(&dir.0, "second", 1).expect("commit");
                assert_ne!(first, second);
                assert_eq!(head(&dir.0).expect("head"), second);
                second
            })
            .collect();
        assert_eq!(shas[0], shas[1], "same content, same dates: same SHA");
    }

    #[test]
    fn clone_checkout_and_restore() {
        let src = TempDir::new("src");
        init(&src.0).expect("init");
        std::fs::write(src.0.join("a.txt"), "v1\n").expect("write");
        let c1 = commit_all(&src.0, "v1", 0).expect("commit");
        std::fs::write(src.0.join("a.txt"), "v2\n").expect("write");
        commit_all(&src.0, "v2", 1).expect("commit");

        let dst = TempDir::new("dst");
        let clone_dir = dst.0.join("clone");
        clone(&src.0, &clone_dir).expect("clone");
        assert_eq!(
            std::fs::read_to_string(clone_dir.join("a.txt")).unwrap(),
            "v2\n"
        );
        checkout(&clone_dir, &c1).expect("checkout");
        assert_eq!(
            std::fs::read_to_string(clone_dir.join("a.txt")).unwrap(),
            "v1\n"
        );
        std::fs::write(clone_dir.join("a.txt"), "dirty\n").expect("write");
        restore(&clone_dir).expect("restore");
        assert_eq!(
            std::fs::read_to_string(clone_dir.join("a.txt")).unwrap(),
            "v1\n"
        );
    }
}
