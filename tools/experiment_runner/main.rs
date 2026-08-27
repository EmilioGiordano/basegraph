//! Experiment runner for the go/no-go protocol (go-no-go.md §2, §4, §5):
//! seeds memories through the real pipeline, then runs every task × arm ×
//! seed with a fresh agent instance and records one line per run in
//! `results/runs.jsonl`.

mod agent;
mod capture;
#[path = "../common/cargo.rs"]
mod cargo;
#[path = "../common/files.rs"]
mod files;
#[path = "../common/git.rs"]
mod git;
#[path = "../common/mcp_client.rs"]
mod mcp_client;
mod plan;
#[path = "../common/rng.rs"]
mod rng;
mod run;
#[path = "../common/schema.rs"]
mod schema;
mod transcript;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::Parser;

use agent::{Agent, CaptureAt, ClaudeCli, McpServer, Scripted};
use capture::CaptureArtifact;
use schema::{Arm, Manifest, RepoSpec};

#[derive(Parser)]
#[command(
    name = "experiment_runner",
    about = "Run the three-arm experiment over the generated repos"
)]
struct Cli {
    #[arg(long, default_value = "synth_repos/manifest.json")]
    manifest: PathBuf,
    #[arg(long, default_value = "results")]
    out: PathBuf,
    /// Arms to run (comma separated: a0,a1,a2).
    #[arg(long, default_value = "a0,a1,a2")]
    arms: String,
    /// Seeds per task (2 in the protocol: a sensitivity analysis, not n).
    #[arg(long, default_value_t = 2)]
    seeds: u32,
    /// Restrict to these repo ids (comma separated).
    #[arg(long)]
    repos: Option<String>,
    /// Restrict to these task ids (comma separated).
    #[arg(long)]
    tasks: Option<String>,
    /// `claude`, or `scripted:<arm=fix,...>` (fix: correct|wrong|wrong-blind|noop).
    #[arg(long, default_value = "claude")]
    agent: String,
    #[arg(long, default_value = "claude")]
    claude_bin: String,
    /// Path to the codegraph binary (default: next to this executable).
    #[arg(long)]
    codegraph_bin: Option<PathBuf>,
    /// Pinned model id for every arm.
    #[arg(long)]
    model: Option<String>,
    #[arg(long, default_value_t = 40)]
    max_turns: u32,
    /// Optional cost cap per run, in USD.
    #[arg(long)]
    budget_usd: Option<f64>,
    /// Time cap per run (same for every arm).
    #[arg(long, default_value_t = 900)]
    time_cap_secs: u64,
    /// Token cap per run (heuristic chars/4 over prompt + transcript).
    #[arg(long, default_value_t = 200_000)]
    token_cap: usize,
    /// Seed of the run order randomisation.
    #[arg(long, default_value_t = 1)]
    order_seed: u64,
    /// Re-run the memory seeding sessions even if captures exist.
    #[arg(long)]
    reseed: bool,
    /// Keep the per-run working clones (for inspection).
    #[arg(long)]
    keep_work: bool,
    /// Where the per-run clones are made (default: <out>/work). Put it
    /// outside any directory tree that has a CLAUDE.md.
    #[arg(long)]
    work_dir: Option<PathBuf>,
    /// Commit the seeding sessions run at: `c2` (fix applied, infer the
    /// invariant; pilot protocol) or `c1` (solve the bug, then distill; §4).
    #[arg(long, default_value = "c2")]
    capture_at: String,
    /// Keep the task's primary test hidden from the agent (by default it is
    /// exposed under tests/ and named in the prompt; the oracle never is).
    #[arg(long)]
    hide_primary_test: bool,
    /// Print the plan and exit.
    #[arg(long)]
    dry_run: bool,
}

/// Everything a run needs to know about the environment.
pub struct Ctx {
    pub manifest_dir: PathBuf,
    pub out: PathBuf,
    pub work: PathBuf,
    pub agent: Box<dyn Agent>,
    pub codegraph_bin: PathBuf,
    pub model: Option<String>,
    pub max_turns: u32,
    pub budget_usd: Option<f64>,
    pub time_cap_secs: u64,
    pub token_cap: usize,
    pub keep_work: bool,
    pub capture_at: CaptureAt,
    pub expose_primary_test: bool,
}

impl Ctx {
    pub fn repo_dir(&self, repo: &RepoSpec) -> PathBuf {
        self.manifest_dir.join(&repo.path)
    }

    /// `codegraph build <dir>` so `recall` classifies against a fresh index.
    pub fn build_index(&self, dir: &Path) -> Result<()> {
        let output = Command::new(&self.codegraph_bin)
            .arg("build")
            .arg(dir)
            .output()
            .with_context(|| format!("running {} build", self.codegraph_bin.display()))?;
        if !output.status.success() {
            bail!(
                "codegraph build failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }

    pub fn mcp_server(&self, dir: &Path) -> McpServer {
        McpServer {
            name: "codegraph".into(),
            command: self.codegraph_bin.clone(),
            args: vec!["mcp".into(), dir.to_string_lossy().into_owned()],
        }
    }
}

fn parse_list(list: &Option<String>) -> Option<Vec<String>> {
    list.as_ref().map(|l| {
        l.split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    })
}

fn default_codegraph_bin() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("locating this executable")?;
    let dir = exe.parent().context("executable directory")?;
    Ok(dir.join(format!("codegraph{}", std::env::consts::EXE_SUFFIX)))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let arms: Vec<Arm> = cli
        .arms
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| Arm::parse(s).with_context(|| format!("unknown arm `{s}`")))
        .collect::<Result<_>>()?;
    if arms.is_empty() || cli.seeds == 0 {
        bail!("need at least one arm and one seed");
    }
    let agent: Box<dyn Agent> = match cli.agent.as_str() {
        "claude" => Box::new(ClaudeCli {
            bin: cli.claude_bin.clone(),
        }),
        other => match other.strip_prefix("scripted:") {
            Some(policy) => Box::new(Scripted::parse(policy)?),
            None => bail!("--agent must be `claude` or `scripted:<policy>`"),
        },
    };
    let codegraph_bin = match cli.codegraph_bin {
        Some(p) => p,
        None => default_codegraph_bin()?,
    };
    if !codegraph_bin.is_file() {
        bail!("codegraph binary not found at {}", codegraph_bin.display());
    }
    // `absolute`, not `canonicalize`: the latter yields `\\?\` paths on
    // Windows, which git refuses.
    let codegraph_bin = std::path::absolute(codegraph_bin)?;

    let manifest_text = std::fs::read_to_string(&cli.manifest)
        .with_context(|| format!("reading {}", cli.manifest.display()))?;
    let manifest: Manifest = serde_json::from_str(&manifest_text).context("parsing manifest")?;
    let manifest_dir = std::path::absolute(
        cli.manifest
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
    )?;
    std::fs::create_dir_all(&cli.out)?;
    let out = std::path::absolute(&cli.out)?;
    let work = match &cli.work_dir {
        Some(w) => {
            std::fs::create_dir_all(w)?;
            std::path::absolute(w)?
        }
        None => out.join("work"),
    };
    let cx = Ctx {
        manifest_dir,
        out,
        work,
        agent,
        codegraph_bin,
        model: cli.model,
        max_turns: cli.max_turns,
        budget_usd: cli.budget_usd,
        time_cap_secs: cli.time_cap_secs,
        token_cap: cli.token_cap,
        keep_work: cli.keep_work,
        capture_at: CaptureAt::parse(&cli.capture_at)
            .with_context(|| format!("--capture-at must be c1 or c2, got `{}`", cli.capture_at))?,
        expose_primary_test: !cli.hide_primary_test,
    };

    let repos = parse_list(&cli.repos);
    let tasks = parse_list(&cli.tasks);
    let plan = plan::build(
        &manifest,
        &arms,
        cli.seeds,
        repos.as_deref(),
        tasks.as_deref(),
        cli.order_seed,
    );
    let runs_path = cx.out.join("runs.jsonl");
    let done = plan::completed(&runs_path)?;
    let pending: Vec<&plan::PlanItem> = plan.iter().filter(|p| !done.contains(&p.run_id)).collect();
    println!(
        "agent: {} | plan: {} runs ({} done, {} pending) | model: {} | caps: {} turns, {}s, {} tokens | capture at {} | primary test {}",
        cx.agent.describe(),
        plan.len(),
        done.len(),
        pending.len(),
        cx.model.as_deref().unwrap_or("(cli default)"),
        cx.max_turns,
        cx.time_cap_secs,
        cx.token_cap,
        cx.capture_at.label(),
        if cx.expose_primary_test { "exposed" } else { "hidden" }
    );
    if cli.dry_run {
        for item in &pending {
            println!("  {}", item.run_id);
        }
        return Ok(());
    }

    // Memory seeding first (§4), once per repo and arm, cached on disk.
    let mut captures: BTreeMap<(String, Arm), CaptureArtifact> = BTreeMap::new();
    for item in &pending {
        if item.arm == Arm::A0 || captures.contains_key(&(item.repo_id.clone(), item.arm)) {
            continue;
        }
        let repo = manifest
            .repos
            .iter()
            .find(|r| r.repo_id == item.repo_id)
            .context("plan item refers to an unknown repo")?;
        let artifact = capture::seed_memories(&cx, repo, item.arm, cli.reseed)
            .with_context(|| format!("seeding {} for {}", item.arm.label(), repo.repo_id))?;
        println!(
            "capture {} {}: {} ({} entries{})",
            repo.repo_id,
            item.arm.label(),
            if artifact.usable {
                "usable"
            } else {
                "UNUSABLE"
            },
            artifact.entries,
            if artifact.notes.is_empty() {
                String::new()
            } else {
                format!("; {}", artifact.notes.join("; "))
            }
        );
        captures.insert((item.repo_id.clone(), item.arm), artifact);
    }
    let all_captures: Vec<&CaptureArtifact> = captures.values().collect();
    std::fs::write(
        cx.out.join("captures.json"),
        serde_json::to_string_pretty(&all_captures)?,
    )?;

    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&runs_path)?;
    for (i, item) in pending.iter().enumerate() {
        let repo = manifest
            .repos
            .iter()
            .find(|r| r.repo_id == item.repo_id)
            .context("unknown repo")?;
        let task = repo
            .tasks
            .iter()
            .find(|t| t.task_id == item.task_id)
            .context("unknown task")?;
        let capture = captures.get(&(item.repo_id.clone(), item.arm));
        let record = run::run_task(&cx, item, repo, task, capture)
            .with_context(|| format!("run {}", item.run_id))?;
        writeln!(log, "{}", serde_json::to_string(&record)?)?;
        log.flush()?;
        println!(
            "[{}/{}] {} violation={} fix_pass={} tokens={} time={:.1}s{}{}",
            i + 1,
            pending.len(),
            record.run_id,
            record
                .violation
                .map(|v| if v { "yes" } else { "no" })
                .unwrap_or("n/a"),
            record.fix_pass,
            record.tokens,
            record.time_secs,
            if record.false_confidence {
                " FALSE-CONFIDENCE"
            } else {
                ""
            },
            if record.cap_exhausted { " CAP" } else { "" }
        );
    }
    println!("wrote {}", runs_path.display());
    Ok(())
}
