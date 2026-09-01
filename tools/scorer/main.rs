//! Scorer for the go/no-go experiment: reads `runs.jsonl`, applies the
//! sealed thresholds of go-no-go.md §7 and writes `summary.json` and
//! `verdict.txt`. Pure data analysis, no external dependencies.

#[path = "../common/schema.rs"]
mod schema;
mod stats;
mod verdict;

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;

use schema::{Arm, RunRecord};
use verdict::{Summary, Verdict};

#[derive(Parser)]
#[command(
    name = "scorer",
    about = "Score runs.jsonl against the pre-committed thresholds"
)]
struct Cli {
    #[arg(long, default_value = "results/runs.jsonl")]
    runs: PathBuf,
    /// Output directory for summary.json and verdict.txt (default: next to runs).
    #[arg(long)]
    out: Option<PathBuf>,
    /// Manual false-confidence scoring (rubric, §8): JSON object
    /// `{ "<run_id>": true|false, ... }` overriding the runner's auto-score.
    #[arg(long)]
    overrides: Option<PathBuf>,
    /// The one allowed expansion to 3 tasks per repo already happened:
    /// a grey zone is then NO-GO.
    #[arg(long)]
    expanded: bool,
    /// Also write `runs_table.md`, one row per run (the pilot's log sheet).
    #[arg(long)]
    table: bool,
}

/// One markdown row per run: the registration sheet of the pilot protocol.
pub fn render_table(runs: &[RunRecord]) -> String {
    let mut rows: Vec<&RunRecord> = runs.iter().collect();
    rows.sort_by(|a, b| {
        (&a.repo_id, &a.task_id, a.arm, a.seed).cmp(&(&b.repo_id, &b.task_id, b.arm, b.seed))
    });
    let mut out = String::from(
        "| Repo | Task | Arm | Seed | Drift | Oracle passes | Fix passes | Freshness seen (A2) | Read gotchas (A1) | git archaeology | Verified current code | False confidence | Tokens | Time (s) | Notes |\n\
         |---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for r in rows {
        let oracle = match r.violation {
            Some(true) => "no (VIOLATION)",
            Some(false) => "yes",
            None => "n/a",
        };
        let freshness = if r.arm == Arm::A2 {
            if r.instrumentation.memory_consulted {
                if r.instrumentation.memory_statuses.is_empty() {
                    "consulted, no memories".to_string()
                } else {
                    r.instrumentation.memory_statuses.join(",")
                }
            } else {
                "not consulted".to_string()
            }
        } else {
            "-".to_string()
        };
        let gotchas = match r.arm {
            Arm::A1 => {
                if r.instrumentation.md_read {
                    "yes"
                } else {
                    "no"
                }
            }
            _ => "-",
        };
        let yes_no = |b: bool| if b { "yes" } else { "no" };
        let verified = if !r.instrumentation.stale_material_seen {
            "-"
        } else if r.instrumentation.verified_before_stale {
            "before"
        } else if r.instrumentation.verified_after_stale {
            "after"
        } else {
            "no"
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {:.0} | {} |\n",
            r.repo_id,
            r.task_id,
            r.arm.label(),
            r.seed,
            yes_no(r.drift),
            oracle,
            yes_no(r.fix_pass),
            freshness,
            gotchas,
            yes_no(r.instrumentation.git_archaeology),
            verified,
            yes_no(r.false_confidence),
            r.tokens,
            r.time_secs,
            r.notes.join("; ").replace('|', "/")
        ));
    }
    out
}

pub fn load_runs(text: &str) -> Result<Vec<RunRecord>> {
    let mut runs = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let record: RunRecord =
            serde_json::from_str(line).with_context(|| format!("runs.jsonl line {}", i + 1))?;
        runs.push(record);
    }
    Ok(runs)
}

pub fn apply_overrides(runs: &mut [RunRecord], overrides: &BTreeMap<String, bool>) -> usize {
    let mut applied = 0;
    for run in runs.iter_mut() {
        if let Some(&value) = overrides.get(&run.run_id) {
            if run.false_confidence != value {
                run.notes.push(format!(
                    "false_confidence overridden by rubric scoring: {} -> {}",
                    run.false_confidence, value
                ));
            }
            run.false_confidence = value;
            applied += 1;
        }
    }
    applied
}

pub fn render_verdict(summary: &Summary) -> String {
    let mut out = format!("{}\n\n", summary.verdict.label());
    out.push_str(&format!(
        "runs: {} | tasks: {} ({} drift) | seeds: {:?} | decision on seed {}\n",
        summary.runs,
        summary.tasks,
        summary.drift_tasks,
        summary.seeds,
        summary.primary.seed.unwrap_or(0)
    ));
    for line in &summary.rationale {
        out.push_str(&format!("- {line}\n"));
    }
    out.push_str("\nviolation rate (seed 0):\n");
    for (arm, stats) in &summary.primary.all {
        let drift = summary.primary.drift.get(arm);
        out.push_str(&format!(
            "  {arm}: {}/{} = {:.2} [{:.2}, {:.2}]{}  false-confidence {}  cap {}  material-used {}\n",
            stats.violation.hits,
            stats.violation.n,
            stats.violation.rate,
            stats.violation.ci_low,
            stats.violation.ci_high,
            drift
                .map(|d| format!("  drift {}/{}", d.violation.hits, d.violation.n))
                .unwrap_or_default(),
            stats.false_confidence.hits,
            stats.cap_exhausted.hits,
            stats.material_used.hits
        ));
    }
    if !summary.verdict_stable_across_seeds {
        out.push_str("\nseed sensitivity: verdict differs across seeds\n");
    }
    if summary.verdict == Verdict::Grey && !summary.expanded {
        out.push_str("\nnext: one expansion to 3 tasks per repo, then re-score with --expanded\n");
    }
    out
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let text = std::fs::read_to_string(&cli.runs)
        .with_context(|| format!("reading {}", cli.runs.display()))?;
    let mut runs = load_runs(&text)?;
    if runs.is_empty() {
        bail!("{} has no runs", cli.runs.display());
    }
    if let Some(path) = &cli.overrides {
        let overrides: BTreeMap<String, bool> = serde_json::from_str(
            &std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?,
        )
        .context("overrides must be a JSON object of run_id -> bool")?;
        let applied = apply_overrides(&mut runs, &overrides);
        println!("applied {applied} false-confidence overrides");
    }
    let summary = verdict::summarize(&runs, cli.expanded);
    let out = match cli.out {
        Some(o) => o,
        None => cli
            .runs
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".")),
    };
    std::fs::create_dir_all(&out)?;
    std::fs::write(
        out.join("summary.json"),
        serde_json::to_string_pretty(&summary)?,
    )?;
    let text = render_verdict(&summary);
    std::fs::write(out.join("verdict.txt"), &text)?;
    if cli.table {
        std::fs::write(out.join("runs_table.md"), render_table(&runs))?;
    }
    print!("{text}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use schema::Instrumentation;

    fn record(id: &str, fc: bool) -> RunRecord {
        RunRecord {
            run_id: id.into(),
            repo_id: "repo_01".into(),
            task_id: "task_1".into(),
            arm: Arm::A2,
            seed: 0,
            drift: true,
            violation: Some(false),
            fix_pass: true,
            tokens: 1,
            reported_tokens: None,
            time_secs: 0.0,
            cap_exhausted: false,
            timed_out: false,
            instrumentation: Instrumentation::default(),
            false_confidence: fc,
            transcript: None,
            notes: vec![],
        }
    }

    #[test]
    fn loads_lines_and_rejects_garbage() {
        let a = serde_json::to_string(&record("a", false)).unwrap();
        let runs = load_runs(&format!("{a}\n\n{a}\n")).expect("load");
        assert_eq!(runs.len(), 2);
        assert!(load_runs("{\"nope\": 1}\n").is_err());
        assert!(load_runs("").expect("empty").is_empty());
    }

    #[test]
    fn overrides_replace_the_auto_score_and_leave_a_note() {
        let mut runs = vec![record("a", true), record("b", false)];
        let overrides = BTreeMap::from([("a".to_string(), false), ("zzz".to_string(), true)]);
        assert_eq!(apply_overrides(&mut runs, &overrides), 1);
        assert!(!runs[0].false_confidence);
        assert!(runs[0].notes[0].contains("overridden"));
        assert!(!runs[1].false_confidence && runs[1].notes.is_empty());
    }

    #[test]
    fn table_has_one_row_per_run_in_stable_order() {
        let mut b = record("b", false);
        b.arm = Arm::A0;
        b.violation = Some(true);
        let mut a = record("a", true);
        a.instrumentation.memory_consulted = true;
        a.instrumentation.memory_statuses = vec!["evolved".into()];
        let table = render_table(&[a, b]);
        let rows: Vec<&str> = table.lines().skip(2).collect();
        assert_eq!(rows.len(), 2);
        assert!(
            rows[0].starts_with("| repo_01 | task_1 | a0 |"),
            "{}",
            rows[0]
        );
        assert!(rows[0].contains("no (VIOLATION)"));
        assert!(
            rows[1].contains("| a2 |")
                && rows[1].contains("evolved")
                && rows[1].contains("| yes |")
        );
    }

    #[test]
    fn verdict_text_starts_with_the_verdict() {
        let runs = vec![record("a", false)];
        let summary = verdict::summarize(&runs, false);
        let text = render_verdict(&summary);
        assert!(text.starts_with("GREY\n"), "{text}");
        assert!(text.contains("not decidable"));
    }
}
