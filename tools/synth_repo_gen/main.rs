//! Synthetic repo generator for the go/no-go experiment (go-no-go.md §3):
//! Rust repos with scripted history, a latent invariant and pre-written
//! ground truth, reproducible from a seed.

mod assemble;
#[path = "../common/cargo.rs"]
mod cargo;
#[path = "../common/files.rs"]
mod files;
#[path = "../common/git.rs"]
mod git;
mod render;
#[path = "../common/rng.rs"]
mod rng;
mod scenarios;
#[path = "../common/schema.rs"]
mod schema;
mod verify;

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Parser;

use schema::{Manifest, RepoSpec, MANIFEST_VERSION};

#[derive(Parser)]
#[command(
    name = "synth_repo_gen",
    about = "Generate synthetic Rust repos with scripted history and latent invariants"
)]
struct Cli {
    /// Output directory (receives one repo per subdirectory plus manifest.json).
    #[arg(long, default_value = "synth_repos")]
    out: PathBuf,
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Number of repos (the pilot batch is 2-3, the full run 10).
    #[arg(long, default_value_t = 10)]
    count: usize,
    /// How many of the repos drift at C3.
    #[arg(long, default_value_t = 7)]
    drift: usize,
    /// Findability knob: filler commits buried around the fix commit.
    #[arg(long, default_value_t = 0)]
    noise_commits: usize,
    /// Restrict the catalog to these scenario ids (comma separated).
    #[arg(long)]
    scenarios: Option<String>,
    /// Remove an existing output directory first.
    #[arg(long)]
    clean: bool,
    /// Self-check every task (pristine / correct / wrong) with cargo.
    #[arg(long)]
    verify: bool,
    /// Also write versionable material: bundles, ground truth and manifest.
    #[arg(long)]
    supplementary: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.count == 0 {
        bail!("--count must be at least 1");
    }
    if cli.drift > cli.count {
        bail!(
            "--drift ({}) cannot exceed --count ({})",
            cli.drift,
            cli.count
        );
    }
    let catalog = match &cli.scenarios {
        Some(list) => {
            let mut chosen = Vec::new();
            for id in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                chosen.push(scenarios::by_id(id).with_context(|| {
                    format!(
                        "unknown scenario `{id}` (known: {})",
                        scenarios::all()
                            .iter()
                            .map(|s| s.id)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })?);
            }
            chosen
        }
        None => scenarios::all(),
    };
    if catalog.is_empty() {
        bail!("no scenarios selected");
    }

    if cli.out.exists() {
        if !cli.clean {
            bail!(
                "{} exists; pass --clean to regenerate into it",
                cli.out.display()
            );
        }
        std::fs::remove_dir_all(&cli.out)
            .with_context(|| format!("removing {}", cli.out.display()))?;
    }
    std::fs::create_dir_all(&cli.out)?;

    let plans = assemble::plan(cli.seed, cli.count, cli.drift, cli.noise_commits, &catalog);
    let mut repos: Vec<RepoSpec> = Vec::new();
    for plan in &plans {
        let spec = assemble::build(plan, &cli.out)
            .with_context(|| format!("building {}", plan.repo_id))?;
        println!(
            "{}: {} ({}), {} files, drift={}",
            spec.repo_id,
            spec.scenario,
            spec.crate_name,
            spec.file_count,
            spec.drift_kind
                .map(|k| format!("{k:?}").to_lowercase())
                .unwrap_or_else(|| "none".into())
        );
        repos.push(spec);
    }
    let manifest = Manifest {
        version: MANIFEST_VERSION,
        seed: cli.seed,
        repos,
    };
    let manifest_path = cli.out.join("manifest.json");
    std::fs::write(&manifest_path, serde_json::to_string_pretty(&manifest)?)?;
    println!("wrote {}", manifest_path.display());

    if let Some(supp) = &cli.supplementary {
        write_supplementary(&manifest, &cli.out, supp)?;
        println!("wrote supplementary material to {}", supp.display());
    }

    if cli.verify {
        let work_root = cli.out.join(".verify");
        let mut reports = Vec::new();
        let mut all_ok = true;
        for spec in &manifest.repos {
            let report = verify::verify(
                spec,
                &cli.out.join(&spec.path),
                &work_root.join(&spec.repo_id),
            )
            .with_context(|| format!("verifying {}", spec.repo_id))?;
            let failed: Vec<String> = report
                .checks
                .iter()
                .filter(|c| !c.ok)
                .map(|c| {
                    format!(
                        "{} {} {} expected {} got {:?}",
                        c.task_id, c.state, c.target, c.expect, c.outcome
                    )
                })
                .collect();
            println!(
                "verify {}: {} ({} checks{})",
                report.repo_id,
                if report.ok { "ok" } else { "FAILED" },
                report.checks.len(),
                if failed.is_empty() {
                    String::new()
                } else {
                    format!("; {}", failed.join("; "))
                }
            );
            for c in report.checks.iter().filter(|c| !c.ok) {
                eprintln!("--- {} {} {}\n{}", c.task_id, c.state, c.target, c.detail);
            }
            all_ok &= report.ok;
            reports.push(report);
        }
        std::fs::write(
            cli.out.join("verify_report.json"),
            serde_json::to_string_pretty(&reports)?,
        )?;
        let _ = std::fs::remove_dir_all(&work_root);
        if !all_ok {
            bail!("verification failed; see verify_report.json");
        }
        println!("verification passed for {} repos", reports.len());
    }
    Ok(())
}

/// Versionable copy of the materials: a git bundle per repo (the working
/// trees contain nested .git directories and cannot be committed as-is), the
/// uncommitted ground truth, and the manifest.
fn write_supplementary(manifest: &Manifest, out: &Path, supp: &Path) -> Result<()> {
    // git resolves the bundle path against the repo it runs in.
    let supp = std::path::absolute(supp)?;
    let supp = supp.as_path();
    std::fs::create_dir_all(supp.join("bundles"))?;
    for spec in &manifest.repos {
        let repo_dir = out.join(&spec.path);
        git::bundle(
            &repo_dir,
            &supp
                .join("bundles")
                .join(format!("{}.bundle", spec.repo_id)),
        )?;
        let truth = supp.join("ground_truth").join(&spec.repo_id);
        std::fs::create_dir_all(&truth)?;
        let mut files = vec![spec.capture_task.clone()];
        for t in &spec.tasks {
            files.extend([
                t.description.clone(),
                t.primary_test.clone(),
                t.oracle_test.clone(),
                t.fix_correct.clone(),
                t.fix_wrong.clone(),
            ]);
        }
        for f in files {
            std::fs::copy(repo_dir.join(&f), truth.join(&f))
                .with_context(|| format!("copying {f} for {}", spec.repo_id))?;
        }
    }
    std::fs::write(
        supp.join("manifest.json"),
        serde_json::to_string_pretty(manifest)?,
    )?;
    Ok(())
}
