//! Plan and build one synthetic repo: pick a scenario, a crate name, filler
//! modules and a drift kind from the seed, then write the scripted history
//! (C1 base → noise → C2 fix → noise → C3 drift or body change) and the
//! uncommitted ground truth next to it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result};

use crate::git;
use crate::render::{self, FillerRole, FillerSpec, Params, Stage};
use crate::rng::Rng;
use crate::scenarios::Scenario;
use crate::schema::{Commits, DriftKind, RepoSpec, TaskSpec};

const CRATES: &[&str] = &[
    "fleetops",
    "ledgerly",
    "dispatchr",
    "quotaflow",
    "harborline",
    "metricsmith",
    "auditwing",
    "vaultkeeper",
    "relaycore",
    "tallyforge",
    "orbitdesk",
    "cargoloop",
    "beaconhub",
    "shardworks",
];

const NOUNS: &[&str] = &[
    "audit",
    "backlog",
    "beacon",
    "budget",
    "cadence",
    "catalog",
    "checkpoint",
    "cohort",
    "courier",
    "digest",
    "dispatch",
    "escrow",
    "fleet",
    "forecast",
    "gateway",
    "harbor",
    "heartbeat",
    "inventory",
    "journal",
    "ledger",
    "manifest",
    "metrics",
    "outbox",
    "payload",
    "pipeline",
    "playbook",
    "quota",
    "ration",
    "registry",
    "relay",
    "replica",
    "retry",
    "roster",
    "sentinel",
    "session",
    "shard",
    "snapshot",
    "tally",
    "telemetry",
    "tenant",
    "throttle",
    "ticket",
    "tracker",
    "vault",
    "warehouse",
    "watchdog",
    "workload",
];

const SUFFIXES: &[&str] = &["", "_log", "_index", "_queue", "_stats", "_cache"];
const VERBS: &[&str] = &[
    "filter", "select", "collect", "gather", "screen", "keep", "pick",
];
const NOISE_MESSAGES: &[&str] = &[
    "chore: tidy {name} helpers",
    "feat: add {name} lookup by label",
    "refactor: simplify {name} bookkeeping",
    "feat: expose {name} search helper",
    "chore: extend {name} coverage",
];
const PROVIDER_NOISE_MESSAGES: &[&str] = &[
    "docs: annotate {name} for the hardening pass",
    "chore: leave review notes in {name}",
    "docs: record ownership and style notes in {name}",
    "chore: comment pass over {name}",
    "docs: note compat expectations in {name}",
];

pub const MIN_FILLER: usize = 18;
pub const MAX_FILLER: usize = 55;

#[derive(Debug, Clone)]
pub struct NoiseSpec {
    pub target: NoiseTarget,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoiseTarget {
    /// A cosmetic comment pass over the anchored (provider) module.
    Provider,
    /// An extra helper in one filler module.
    Filler(usize),
}

#[derive(Debug, Clone)]
pub struct RepoPlan {
    pub repo_id: String,
    pub scenario: Scenario,
    pub crate_name: String,
    pub drift: Option<DriftKind>,
    pub filler: Vec<FillerSpec>,
    pub noise_before: Vec<NoiseSpec>,
    pub noise_after: Vec<NoiseSpec>,
    /// Filler module that gets a body-only change at C3 in non-drift repos.
    pub body_change_module: usize,
}

impl RepoPlan {
    pub fn params_c1(&self) -> Params {
        Params {
            fn_name: self.scenario.fn_name.into(),
            arg: self.scenario.arg.into(),
            module: self.scenario.module.into(),
            crate_name: self.crate_name.clone(),
        }
    }

    pub fn params_c3(&self) -> Params {
        let mut p = self.params_c1();
        match self.drift {
            Some(DriftKind::Rename) => p.fn_name = self.scenario.fn_renamed.into(),
            Some(DriftKind::Move) => p.module = self.scenario.module_moved.into(),
            Some(DriftKind::Signature) => p.arg = self.scenario.arg_renamed.into(),
            // Delete: the old symbol is gone for good — its successor has a
            // new name AND a new parameter, so no hash matches the anchor.
            Some(DriftKind::Delete) => {
                p.fn_name = self.scenario.fn_renamed.into();
                p.arg = self.scenario.arg_renamed.into();
            }
            Some(DriftKind::Duplicate) | None => {}
        }
        p
    }
}

/// Lay out `count` repos: scenarios cycle through the catalog, `drift_count`
/// of them drift (kinds cycle too), everything else comes from the seed.
pub fn plan(
    seed: u64,
    count: usize,
    drift_count: usize,
    noise_commits: usize,
    scenarios: &[Scenario],
) -> Vec<RepoPlan> {
    let mut rng = Rng::new(seed);
    let mut order: Vec<usize> = (0..scenarios.len()).collect();
    rng.shuffle(&mut order);
    let mut crates: Vec<&str> = CRATES.to_vec();
    rng.shuffle(&mut crates);
    let mut drift_slots: Vec<bool> = (0..count).map(|i| i < drift_count).collect();
    rng.shuffle(&mut drift_slots);
    let mut kinds: Vec<DriftKind> = DriftKind::ALL.to_vec();
    rng.shuffle(&mut kinds);
    let mut kind_cursor = 0;

    (0..count)
        .map(|i| {
            let repo_id = format!("repo_{:02}", i + 1);
            let scenario = scenarios[order[i % order.len()]];
            let crate_name = if i < crates.len() {
                crates[i].to_string()
            } else {
                format!("{}_{}", crates[i % crates.len()], i / crates.len() + 1)
            };
            let drift = if drift_slots[i] {
                let kind = kinds[kind_cursor % kinds.len()];
                kind_cursor += 1;
                Some(kind)
            } else {
                None
            };
            let mut repo_rng = rng.fork(&repo_id);
            let filler = plan_filler(&mut repo_rng, &scenario);
            let (noise_before, noise_after) =
                plan_noise(&mut repo_rng, &scenario, &filler, noise_commits);
            let body_change_module = repo_rng.below(filler.len());
            RepoPlan {
                repo_id,
                scenario,
                crate_name,
                drift,
                filler,
                noise_before,
                noise_after,
                body_change_module,
            }
        })
        .collect()
}

fn plan_filler(rng: &mut Rng, scenario: &Scenario) -> Vec<FillerSpec> {
    let count = rng.range(MIN_FILLER, MAX_FILLER);
    let reserved: BTreeSet<String> = [
        scenario.module,
        scenario.module_moved,
        scenario.consumer_module,
        "lib",
        "main",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut ordered: Vec<String> = Vec::new();
    let mut guard = 0;
    while ordered.len() < count && guard < 10_000 {
        guard += 1;
        let name = format!("{}{}", rng.pick(NOUNS), rng.pick(SUFFIXES));
        if reserved.contains(&name) || names.contains(&name) {
            continue;
        }
        names.insert(name.clone());
        ordered.push(name);
    }
    let callers = rng.range(2, 3).min(ordered.len());
    let mut caller_slots: Vec<bool> = (0..ordered.len()).map(|i| i < callers).collect();
    rng.shuffle(&mut caller_slots);
    let mut specs: Vec<FillerSpec> = Vec::new();
    for (i, name) in ordered.iter().enumerate() {
        let prev = if i > 0 && rng.chance(40) {
            Some(ordered[rng.below(i)].clone())
        } else {
            None
        };
        specs.push(FillerSpec {
            noun: render::camel(name),
            name: name.clone(),
            verb: rng.pick(VERBS).to_string(),
            role: if caller_slots[i] {
                FillerRole::Caller
            } else {
                FillerRole::Plain
            },
            has_trait: rng.chance(60),
            has_enum: rng.chance(40),
            has_max: rng.chance(50),
            reserve: rng.range(8, 512) as u64 * 16,
            prev,
            extra_fns: 0,
        });
    }
    specs
}

/// Most noise lands between C1 and C2, and at least every other one of those
/// commits touches the provider file itself, so the fix ends up buried in the
/// anchored module's own history (the findability knob of go-no-go.md §3).
fn plan_noise(
    rng: &mut Rng,
    scenario: &Scenario,
    filler: &[FillerSpec],
    total: usize,
) -> (Vec<NoiseSpec>, Vec<NoiseSpec>) {
    let mut make = |provider: bool| {
        if provider {
            let message = rng
                .pick(PROVIDER_NOISE_MESSAGES)
                .replace("{name}", scenario.module);
            NoiseSpec {
                target: NoiseTarget::Provider,
                message,
            }
        } else {
            let module = rng.below(filler.len());
            let message = rng
                .pick(NOISE_MESSAGES)
                .replace("{name}", &filler[module].name);
            NoiseSpec {
                target: NoiseTarget::Filler(module),
                message,
            }
        }
    };
    let after_count = total / 5;
    let before: Vec<NoiseSpec> = (0..total - after_count).map(|i| make(i % 2 == 0)).collect();
    let after: Vec<NoiseSpec> = (0..after_count).map(|_| make(false)).collect();
    (before, after)
}

const EXCLUDE: &str = "# ground truth and experiment material, never committed\n\
task_*.md\ncapture_task.md\nprimary_test_*.rs\noracle_test_*.rs\nfix_correct_*.rs\nfix_wrong_*.rs\n\
gotchas.md\ncodegraph.json\ncodegraph-memory.jsonl\n";

/// Every committed file of a repo state.
fn tree(
    plan: &RepoPlan,
    filler: &[FillerSpec],
    p: &Params,
    stage: Stage,
    provider_noise: usize,
) -> BTreeMap<String, String> {
    let s = &plan.scenario;
    let mut files = BTreeMap::new();
    let mut modules: Vec<String> = vec![p.module.clone(), s.consumer_module.to_string()];
    files.insert(
        p.anchor_file(),
        render::anchor_module(s, stage, p, provider_noise),
    );
    files.insert(
        format!("src/{}.rs", s.consumer_module),
        render::consumer_module(s, p),
    );
    for f in filler {
        files.insert(f.file(), render::filler_module(f, s, p));
        modules.push(f.name.clone());
    }
    files.insert("src/lib.rs".into(), render::lib_rs(&p.crate_name, &modules));
    files.insert("Cargo.toml".into(), render::cargo_toml(&p.crate_name));
    files.insert(".gitignore".into(), render::GITIGNORE.into());
    files
}

fn write_tree(
    dir: &Path,
    previous: &BTreeMap<String, String>,
    current: &BTreeMap<String, String>,
) -> Result<()> {
    for path in previous.keys() {
        if !current.contains_key(path) {
            let full = dir.join(path);
            if full.exists() {
                std::fs::remove_file(&full)
                    .with_context(|| format!("removing {}", full.display()))?;
            }
        }
    }
    for (path, content) in current {
        if previous.get(path) == Some(content) {
            continue;
        }
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full, content).with_context(|| format!("writing {}", full.display()))?;
    }
    Ok(())
}

/// Build the repo under `out_root/<repo_id>` and describe it for the manifest.
pub fn build(plan: &RepoPlan, out_root: &Path) -> Result<RepoSpec> {
    let dir = out_root.join(&plan.repo_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("clearing {}", dir.display()))?;
    }
    std::fs::create_dir_all(&dir)?;
    git::init(&dir)?;
    std::fs::create_dir_all(dir.join(".git").join("info"))?;
    std::fs::write(dir.join(".git").join("info").join("exclude"), EXCLUDE)?;

    let s = &plan.scenario;
    let p1 = plan.params_c1();
    let p3 = plan.params_c3();
    let mut filler = plan.filler.clone();
    let mut index = 0;

    let mut provider_noise = 0usize;
    let mut previous = BTreeMap::new();
    let current = tree(plan, &filler, &p1, Stage::C1, provider_noise);
    write_tree(&dir, &previous, &current)?;
    let c1 = git::commit_all(&dir, s.commit_c1, index)?;
    previous = current;

    for noise in &plan.noise_before {
        index += 1;
        match noise.target {
            NoiseTarget::Provider => provider_noise += 1,
            NoiseTarget::Filler(module) => filler[module].extra_fns += 1,
        }
        let current = tree(plan, &filler, &p1, Stage::C1, provider_noise);
        write_tree(&dir, &previous, &current)?;
        git::commit_all(&dir, &noise.message, index)?;
        previous = current;
    }

    // The fix commit, buried in the noise the loop above laid down.
    index += 1;
    let current = tree(plan, &filler, &p1, Stage::C2, provider_noise);
    write_tree(&dir, &previous, &current)?;
    let c2 = git::commit_all(&dir, s.commit_c2, index)?;
    previous = current;

    for noise in &plan.noise_after {
        index += 1;
        match noise.target {
            NoiseTarget::Provider => provider_noise += 1,
            NoiseTarget::Filler(module) => filler[module].extra_fns += 1,
        }
        let current = tree(plan, &filler, &p1, Stage::C2, provider_noise);
        write_tree(&dir, &previous, &current)?;
        git::commit_all(&dir, &noise.message, index)?;
        previous = current;
    }

    index += 1;
    let (current, message, c3_stage) = match plan.drift {
        Some(_) => (
            tree(plan, &filler, &p3, Stage::C2, provider_noise),
            s.commit_c3_drift,
            Stage::C2,
        ),
        None => {
            filler[plan.body_change_module].extra_fns += 1;
            (
                tree(plan, &filler, &p3, Stage::C3Variant, provider_noise),
                s.commit_c3_body,
                Stage::C3Variant,
            )
        }
    };
    write_tree(&dir, &previous, &current)?;
    let c3 = git::commit_all(&dir, message, index)?;

    // Ground truth, rendered against the C3 identifiers, kept out of git.
    std::fs::write(dir.join("capture_task.md"), p1.fill(s.capture_task))?;
    let base_fn = c3_stage.anchor_fn(s);
    let mut tasks = Vec::new();
    for (i, task) in s.tasks.iter().enumerate() {
        let n = i + 1;
        let spec = TaskSpec {
            task_id: format!("task_{n}"),
            title: task.title.into(),
            description: format!("task_{n}.md"),
            primary_test: format!("primary_test_{n}.rs"),
            oracle_test: format!("oracle_test_{n}.rs"),
            fix_correct: format!("fix_correct_{n}.rs"),
            fix_wrong: format!("fix_wrong_{n}.rs"),
            fix_target: p3.anchor_file(),
        };
        std::fs::write(dir.join(&spec.description), p3.fill(task.description))?;
        std::fs::write(dir.join(&spec.primary_test), p3.fill(task.primary_test))?;
        std::fs::write(dir.join(&spec.oracle_test), p3.fill(task.oracle_test))?;
        std::fs::write(
            dir.join(&spec.fix_correct),
            render::fixed_module(s, base_fn, &task.correct, &p3),
        )?;
        std::fs::write(
            dir.join(&spec.fix_wrong),
            render::fixed_module(s, base_fn, &task.wrong, &p3),
        )?;
        tasks.push(spec);
    }

    Ok(RepoSpec {
        repo_id: plan.repo_id.clone(),
        path: plan.repo_id.clone(),
        crate_name: plan.crate_name.clone(),
        scenario: s.id.into(),
        invariant_type: s.invariant_type.into(),
        // Worded with the C2 identifiers: that is what a memory seeded at
        // C2 would say, before the drift.
        invariant_text: p1.fill(s.invariant_text),
        anchor_fqn_c2: p1.fn_name.clone(),
        anchor_fqn_c3: p3.fn_name.clone(),
        anchor_file_c2: p1.anchor_file(),
        anchor_file_c3: p3.anchor_file(),
        drift: plan.drift.is_some(),
        drift_kind: plan.drift,
        file_count: current.keys().filter(|k| k.ends_with(".rs")).count(),
        commits: Commits { c1, c2, c3 },
        capture_task: "capture_task.md".into(),
        tasks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenarios;

    #[test]
    fn plan_is_deterministic_and_respects_counts() {
        let scenarios = scenarios::all();
        let a = plan(42, 10, 7, 4, &scenarios);
        let b = plan(42, 10, 7, 4, &scenarios);
        assert_eq!(a.len(), 10);
        assert_eq!(a.iter().filter(|r| r.drift.is_some()).count(), 7);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.repo_id, y.repo_id);
            assert_eq!(x.scenario.id, y.scenario.id);
            assert_eq!(x.crate_name, y.crate_name);
            assert_eq!(x.drift, y.drift);
            assert_eq!(x.filler, y.filler);
            assert_eq!(x.noise_before.len() + x.noise_after.len(), 4);
            // Most noise sits between C1 and C2, and at least half of that
            // touches the provider file itself.
            assert!(x.noise_before.len() >= x.noise_after.len());
            let provider_noise = x
                .noise_before
                .iter()
                .filter(|n| n.target == NoiseTarget::Provider)
                .count();
            assert!(
                provider_noise * 2 >= x.noise_before.len(),
                "{provider_noise}"
            );
        }
        let ids: BTreeSet<&str> = a.iter().map(|r| r.scenario.id).collect();
        assert_eq!(
            ids.len(),
            8,
            "all eight scenarios are used across ten repos"
        );
        let crates: BTreeSet<&str> = a.iter().map(|r| r.crate_name.as_str()).collect();
        assert_eq!(crates.len(), 10);
        let other = plan(43, 10, 7, 0, &scenarios);
        assert!(a
            .iter()
            .zip(&other)
            .any(|(x, y)| x.scenario.id != y.scenario.id || x.drift != y.drift));
    }

    #[test]
    fn filler_stays_within_the_file_budget_and_has_callers() {
        for repo in plan(7, 10, 7, 0, &scenarios::all()) {
            assert!((MIN_FILLER..=MAX_FILLER).contains(&repo.filler.len()));
            let names: BTreeSet<&str> = repo.filler.iter().map(|f| f.name.as_str()).collect();
            assert_eq!(names.len(), repo.filler.len(), "module names are unique");
            assert!(!names.contains(repo.scenario.module));
            assert!(!names.contains(repo.scenario.module_moved));
            let callers = repo
                .filler
                .iter()
                .filter(|f| f.role == FillerRole::Caller)
                .count();
            assert!((2..=3).contains(&callers));
            for f in &repo.filler {
                if let Some(prev) = &f.prev {
                    assert!(names.contains(prev.as_str()));
                    assert_ne!(prev, &f.name);
                }
            }
        }
    }

    #[test]
    fn c3_params_follow_the_drift_kind() {
        let scenarios = scenarios::all();
        let plans = plan(1, 10, 10, 0, &scenarios);
        for repo in plans {
            let p1 = repo.params_c1();
            let p3 = repo.params_c3();
            match repo.drift {
                Some(DriftKind::Rename) => {
                    assert_ne!(p1.fn_name, p3.fn_name);
                    assert_eq!(p1.module, p3.module);
                }
                Some(DriftKind::Move) => {
                    assert_ne!(p1.module, p3.module);
                    assert_eq!(p1.fn_name, p3.fn_name);
                }
                Some(DriftKind::Signature) => {
                    assert_ne!(p1.arg, p3.arg);
                    assert_eq!(p1.fn_name, p3.fn_name);
                }
                Some(DriftKind::Delete) => {
                    assert_ne!(p1.fn_name, p3.fn_name);
                    assert_ne!(p1.arg, p3.arg);
                }
                Some(DriftKind::Duplicate) => panic!("duplicate is no longer generated"),
                None => assert_eq!(p1, p3),
            }
        }
        let kinds: BTreeSet<String> = plans_kinds();
        assert!(!kinds.contains("Duplicate"));
    }

    fn plans_kinds() -> BTreeSet<String> {
        plan(1, 10, 10, 0, &scenarios::all())
            .iter()
            .filter_map(|r| r.drift.map(|k| format!("{k:?}")))
            .collect()
    }

    #[test]
    fn tree_lists_every_module_in_lib_rs() {
        let scenarios = scenarios::all();
        let repo = &plan(3, 1, 1, 0, &scenarios)[0];
        let files = tree(repo, &repo.filler, &repo.params_c1(), Stage::C1, 3);
        let lib = &files["src/lib.rs"];
        for path in files
            .keys()
            .filter(|k| k.starts_with("src/") && *k != "src/lib.rs")
        {
            let module = path.trim_start_matches("src/").trim_end_matches(".rs");
            assert!(
                lib.contains(&format!("pub mod {module};")),
                "{module} missing from lib.rs"
            );
        }
        // The consumer module is present and imports the provider.
        let consumer = &files[&format!("src/{}.rs", repo.scenario.consumer_module)];
        assert!(consumer.contains(&format!("use crate::{}::", repo.scenario.module)));
        // Provider noise lines are cosmetic comments, threaded by level.
        let provider = &files[&repo.params_c1().anchor_file()];
        assert_eq!(provider.matches("\n// ").count(), 3, "{provider}");
        let quiet = tree(repo, &repo.filler, &repo.params_c1(), Stage::C1, 0);
        assert_ne!(provider, &quiet[&repo.params_c1().anchor_file()]);
    }
}
