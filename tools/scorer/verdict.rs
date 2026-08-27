//! Aggregation and the pre-committed decision rule of go-no-go.md §7.
//!
//! The decision uses one run per task (seed 0): repeated seeds are a
//! sensitivity analysis and do not inflate n (§5.6). Every other seed and the
//! pooled runs are scored the same way and reported; if any seed disagrees
//! with the seed-0 verdict the result is downgraded to GREY.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::schema::{Arm, RunRecord};
use crate::stats::Rate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Verdict {
    Go,
    NoGo,
    Grey,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::Go => "GO",
            Verdict::NoGo => "NO-GO",
            Verdict::Grey => "GREY",
        }
    }
}

/// Runs of one arm in one condition, with the rates the thresholds use.
#[derive(Debug, Clone, Serialize)]
pub struct ArmStats {
    pub runs: usize,
    pub violation: Rate,
    pub fix_pass: Rate,
    pub false_confidence: Rate,
    pub cap_exhausted: Rate,
    pub material_used: Rate,
    pub git_archaeology: Rate,
    pub mean_tokens: f64,
    pub mean_time_secs: f64,
    /// Runs whose oracle could not be evaluated (task API missing).
    pub violation_unknown: usize,
}

impl ArmStats {
    pub fn of(runs: &[&RunRecord], arm: Arm) -> ArmStats {
        let n = runs.len();
        let count = |f: &dyn Fn(&RunRecord) -> bool| runs.iter().filter(|r| f(r)).count();
        let mean = |f: &dyn Fn(&RunRecord) -> f64| {
            if n == 0 {
                f64::NAN
            } else {
                runs.iter().map(|r| f(r)).sum::<f64>() / n as f64
            }
        };
        ArmStats {
            runs: n,
            violation: Rate::new(count(&|r| r.violated()), n),
            fix_pass: Rate::new(count(&|r| r.fix_pass), n),
            false_confidence: Rate::new(count(&|r| r.false_confidence), n),
            cap_exhausted: Rate::new(count(&|r| r.cap_exhausted), n),
            material_used: Rate::new(
                count(&|r| match arm {
                    Arm::A0 => r.instrumentation.git_archaeology,
                    Arm::A1 => r.instrumentation.md_read,
                    Arm::A2 => r.instrumentation.memory_consulted,
                }),
                n,
            ),
            git_archaeology: Rate::new(count(&|r| r.instrumentation.git_archaeology), n),
            mean_tokens: mean(&|r| r.tokens as f64),
            mean_time_secs: mean(&|r| r.time_secs),
            violation_unknown: count(&|r| r.violation.is_none()),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Comparison {
    pub label: String,
    pub condition: String,
    pub left_arm: Arm,
    pub right_arm: Arm,
    pub left_violations: usize,
    pub right_violations: usize,
    pub tasks: usize,
    /// left − right in runs; the decision needs it strictly below −1.
    pub difference: i64,
    pub strictly_better: bool,
    pub grey: bool,
}

fn compare(
    label: &str,
    condition: &str,
    left: (Arm, &ArmStats),
    right: (Arm, &ArmStats),
) -> Comparison {
    let l = left.1.violation.hits as i64;
    let r = right.1.violation.hits as i64;
    let difference = l - r;
    Comparison {
        label: label.into(),
        condition: condition.into(),
        left_arm: left.0,
        right_arm: right.0,
        left_violations: left.1.violation.hits,
        right_violations: right.1.violation.hits,
        tasks: left.1.runs.max(right.1.runs),
        difference,
        strictly_better: difference < 0,
        // Ties or one-run differences are the grey zone (§7).
        grey: difference.abs() <= 1,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Decision {
    pub verdict: Verdict,
    pub condition_a: Comparison,
    pub condition_b: Comparison,
    pub false_confidence_a2: usize,
    pub false_confidence_a1: usize,
    pub false_confidence_ok: bool,
    pub false_confidence_material: bool,
    pub grey_zone: bool,
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SeedSummary {
    pub seed: Option<u32>,
    pub all: BTreeMap<String, ArmStats>,
    pub drift: BTreeMap<String, ArmStats>,
    pub no_drift: BTreeMap<String, ArmStats>,
    pub decision: Option<Decision>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub runs: usize,
    pub tasks: usize,
    pub drift_tasks: usize,
    pub seeds: Vec<u32>,
    pub arms: Vec<Arm>,
    /// Decision on seed 0 (n = tasks), per §5.6.
    pub primary: SeedSummary,
    /// Every other seed, scored the same way.
    pub sensitivity: Vec<SeedSummary>,
    /// All seeds pooled (runs, not independent samples).
    pub pooled: SeedSummary,
    pub verdict_stable_across_seeds: bool,
    pub sanity_cap_warning: Option<String>,
    pub expanded: bool,
    pub verdict: Verdict,
    pub rationale: Vec<String>,
}

fn arm_map(runs: &[&RunRecord], arms: &[Arm]) -> BTreeMap<String, ArmStats> {
    arms.iter()
        .map(|&arm| {
            let of_arm: Vec<&RunRecord> = runs.iter().copied().filter(|r| r.arm == arm).collect();
            (arm.label().to_string(), ArmStats::of(&of_arm, arm))
        })
        .collect()
}

fn seed_summary(
    runs: &[&RunRecord],
    seed: Option<u32>,
    arms: &[Arm],
    expanded: bool,
) -> SeedSummary {
    let drift: Vec<&RunRecord> = runs.iter().copied().filter(|r| r.drift).collect();
    let no_drift: Vec<&RunRecord> = runs.iter().copied().filter(|r| !r.drift).collect();
    let all = arm_map(runs, arms);
    let drift_map = arm_map(&drift, arms);
    let no_drift_map = arm_map(&no_drift, arms);
    let decision = decide(&all, &drift_map, expanded);
    SeedSummary {
        seed,
        all,
        drift: drift_map,
        no_drift: no_drift_map,
        decision,
    }
}

/// §7, verbatim: GO needs (a) A2 < A0 on all tasks and (b) A2 < A1 in drift
/// with false confidence ≤ A1; ties or one-run differences are grey (once
/// expanded, grey is NO-GO); a materially worse false-confidence rate
/// (more than one run) is NO-GO.
pub fn decide(
    all: &BTreeMap<String, ArmStats>,
    drift: &BTreeMap<String, ArmStats>,
    expanded: bool,
) -> Option<Decision> {
    let a0 = all.get("a0")?;
    let a2 = all.get("a2")?;
    let d1 = drift.get("a1")?;
    let d2 = drift.get("a2")?;
    if a0.runs == 0 || a2.runs == 0 || d1.runs == 0 || d2.runs == 0 {
        return None;
    }
    let condition_a = compare("A2 vs A0", "all tasks", (Arm::A2, a2), (Arm::A0, a0));
    let condition_b = compare("A2 vs A1", "drift", (Arm::A2, d2), (Arm::A1, d1));
    let fc2 = d2.false_confidence.hits;
    let fc1 = d1.false_confidence.hits;
    let false_confidence_ok = fc2 <= fc1;
    let false_confidence_material = fc2 as i64 - fc1 as i64 > 1;
    let mut rationale = Vec::new();
    rationale.push(format!(
        "(a) all tasks: A2 {}/{} vs A0 {}/{} violations (diff {})",
        a2.violation.hits, a2.runs, a0.violation.hits, a0.runs, condition_a.difference
    ));
    rationale.push(format!(
        "(b) drift: A2 {}/{} vs A1 {}/{} violations (diff {}); false confidence A2 {} vs A1 {}",
        d2.violation.hits, d2.runs, d1.violation.hits, d1.runs, condition_b.difference, fc2, fc1
    ));
    let grey_zone = condition_a.grey || condition_b.grey;
    let verdict = if condition_a.difference > 0
        || condition_b.difference > 0
        || false_confidence_material
    {
        rationale.push("NO-GO: A2 does not beat the baseline or the markdown, or its false confidence is materially worse".into());
        Verdict::NoGo
    } else if grey_zone {
        if expanded {
            rationale.push("grey zone after the one allowed expansion: NO-GO".into());
            Verdict::NoGo
        } else {
            rationale.push(
                "grey zone (tie or one-run difference): expand to 3 tasks per repo once".into(),
            );
            Verdict::Grey
        }
    } else if condition_a.strictly_better && condition_b.strictly_better && false_confidence_ok {
        rationale.push("GO: both primary conditions hold".into());
        Verdict::Go
    } else {
        rationale.push("NO-GO: false confidence of A2 exceeds A1 in drift".into());
        Verdict::NoGo
    };
    Some(Decision {
        verdict,
        condition_a,
        condition_b,
        false_confidence_a2: fc2,
        false_confidence_a1: fc1,
        false_confidence_ok,
        false_confidence_material,
        grey_zone,
        rationale,
    })
}

pub fn summarize(records: &[RunRecord], expanded: bool) -> Summary {
    let arms: Vec<Arm> = Arm::ALL
        .into_iter()
        .filter(|a| records.iter().any(|r| r.arm == *a))
        .collect();
    let seeds: Vec<u32> = records
        .iter()
        .map(|r| r.seed)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let tasks: BTreeSet<(String, String)> = records
        .iter()
        .map(|r| (r.repo_id.clone(), r.task_id.clone()))
        .collect();
    let drift_tasks = records
        .iter()
        .filter(|r| r.drift)
        .map(|r| (r.repo_id.clone(), r.task_id.clone()))
        .collect::<BTreeSet<_>>()
        .len();

    let primary_seed = seeds.first().copied().unwrap_or(0);
    let of_seed =
        |seed: u32| -> Vec<&RunRecord> { records.iter().filter(|r| r.seed == seed).collect() };
    let primary = seed_summary(&of_seed(primary_seed), Some(primary_seed), &arms, expanded);
    let sensitivity: Vec<SeedSummary> = seeds
        .iter()
        .skip(1)
        .map(|&s| seed_summary(&of_seed(s), Some(s), &arms, expanded))
        .collect();
    let all_refs: Vec<&RunRecord> = records.iter().collect();
    let pooled = seed_summary(&all_refs, None, &arms, expanded);

    let primary_verdict = primary.decision.as_ref().map(|d| d.verdict);
    let verdict_stable_across_seeds = sensitivity
        .iter()
        .all(|s| s.decision.as_ref().map(|d| d.verdict) == primary_verdict);

    let mut rationale: Vec<String> = primary
        .decision
        .as_ref()
        .map(|d| d.rationale.clone())
        .unwrap_or_else(|| vec!["not decidable: some arm or condition has no runs".into()]);
    let mut verdict = primary_verdict.unwrap_or(Verdict::Grey);
    if !verdict_stable_across_seeds && verdict != Verdict::NoGo {
        rationale.push("verdict differs across seeds: downgraded to GREY".into());
        verdict = Verdict::Grey;
    }

    let sanity_cap_warning = {
        let cap = |arm: &str| {
            pooled
                .all
                .get(arm)
                .map(|s| s.cap_exhausted.hits)
                .unwrap_or(0) as i64
        };
        if cap("a2") - cap("a0").max(cap("a1")) >= 2 {
            Some(format!(
                "A2 exhausted the cap in {} runs vs A0 {} / A1 {}: weigh in the interpretation (§7 sanity bound)",
                cap("a2"), cap("a0"), cap("a1")
            ))
        } else {
            None
        }
    };
    if let Some(w) = &sanity_cap_warning {
        rationale.push(w.clone());
    }

    Summary {
        runs: records.len(),
        tasks: tasks.len(),
        drift_tasks,
        seeds,
        arms,
        primary,
        sensitivity,
        pooled,
        verdict_stable_across_seeds,
        sanity_cap_warning,
        expanded,
        verdict,
        rationale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Instrumentation;

    fn record(
        repo: usize,
        task: usize,
        arm: Arm,
        seed: u32,
        drift: bool,
        violation: bool,
        fc: bool,
    ) -> RunRecord {
        RunRecord {
            run_id: format!("repo_{repo:02}-task_{task}-{}-s{seed}", arm.label()),
            repo_id: format!("repo_{repo:02}"),
            task_id: format!("task_{task}"),
            arm,
            seed,
            drift,
            violation: Some(violation),
            fix_pass: true,
            tokens: 100,
            reported_tokens: None,
            time_secs: 1.0,
            cap_exhausted: false,
            timed_out: false,
            instrumentation: Instrumentation {
                memory_consulted: arm == Arm::A2,
                md_read: arm == Arm::A1,
                git_archaeology: arm == Arm::A0,
                ..Instrumentation::default()
            },
            false_confidence: fc,
            transcript: None,
            notes: vec![],
        }
    }

    /// 10 repos (7 drift) × 2 tasks; violations per arm given as a rate.
    fn experiment(
        seed: u32,
        a0: &dyn Fn(usize) -> bool,
        a1: &dyn Fn(usize) -> bool,
        a2: &dyn Fn(usize) -> bool,
        fc2: &dyn Fn(usize) -> bool,
    ) -> Vec<RunRecord> {
        let mut out = Vec::new();
        let mut i = 0;
        for repo in 1..=10 {
            let drift = repo <= 7;
            for task in 1..=2 {
                out.push(record(repo, task, Arm::A0, seed, drift, a0(i), false));
                out.push(record(repo, task, Arm::A1, seed, drift, a1(i), false));
                out.push(record(repo, task, Arm::A2, seed, drift, a2(i), fc2(i)));
                i += 1;
            }
        }
        out
    }

    #[test]
    fn clear_win_is_go() {
        let runs = experiment(0, &|i| i % 2 == 0, &|i| i % 3 == 0, &|_| false, &|_| false);
        let s = summarize(&runs, false);
        assert_eq!(s.tasks, 20);
        assert_eq!(s.drift_tasks, 14);
        assert_eq!(s.verdict, Verdict::Go);
        let d = s.primary.decision.as_ref().unwrap();
        assert!(d.condition_a.strictly_better && d.condition_b.strictly_better);
        assert_eq!(s.primary.all["a2"].violation.hits, 0);
        assert_eq!(s.primary.all["a0"].violation.hits, 10);
        assert_eq!(s.primary.all["a0"].material_used.hits, 20);
    }

    #[test]
    fn losing_to_the_markdown_in_drift_is_no_go() {
        // A2 beats A0 overall but A1 is better in drift.
        let runs = experiment(0, &|_| true, &|_| false, &|i| i % 2 == 0, &|_| false);
        let s = summarize(&runs, false);
        assert_eq!(s.verdict, Verdict::NoGo);
    }

    #[test]
    fn material_false_confidence_is_no_go_even_with_fewer_violations() {
        let runs = experiment(0, &|_| true, &|_| true, &|_| false, &|i| i < 3);
        let s = summarize(&runs, false);
        assert_eq!(s.verdict, Verdict::NoGo);
        assert!(
            s.primary
                .decision
                .as_ref()
                .unwrap()
                .false_confidence_material
        );
    }

    #[test]
    fn one_run_difference_is_grey_then_no_go_after_expansion() {
        // A2 violates once less than A0 and once less than A1.
        let runs = experiment(
            0,
            &|i| i == 0 || i == 1,
            &|i| i == 0 || i == 1,
            &|i| i == 0,
            &|_| false,
        );
        assert_eq!(summarize(&runs, false).verdict, Verdict::Grey);
        assert_eq!(summarize(&runs, true).verdict, Verdict::NoGo);
    }

    #[test]
    fn disagreeing_seeds_downgrade_to_grey() {
        let mut runs = experiment(0, &|_| true, &|_| true, &|_| false, &|_| false);
        runs.extend(experiment(1, &|_| true, &|_| true, &|_| true, &|_| false));
        let s = summarize(&runs, false);
        assert_eq!(s.seeds, vec![0, 1]);
        assert_eq!(s.primary.decision.as_ref().unwrap().verdict, Verdict::Go);
        assert!(!s.verdict_stable_across_seeds);
        assert_eq!(s.verdict, Verdict::Grey);
        assert_eq!(s.runs, 120);
        assert_eq!(s.pooled.all["a2"].runs, 40);
        assert_eq!(s.pooled.drift["a2"].runs, 28);
    }

    #[test]
    fn missing_arms_are_not_decidable() {
        let runs: Vec<RunRecord> = experiment(0, &|_| true, &|_| true, &|_| false, &|_| false)
            .into_iter()
            .filter(|r| r.arm != Arm::A1)
            .collect();
        let s = summarize(&runs, false);
        assert!(s.primary.decision.is_none());
        assert_eq!(s.verdict, Verdict::Grey);
        assert!(s.rationale[0].contains("not decidable"));
    }

    #[test]
    fn cap_exhaustion_of_a2_raises_the_sanity_warning() {
        let mut runs = experiment(0, &|_| true, &|_| true, &|_| false, &|_| false);
        for r in runs.iter_mut().filter(|r| r.arm == Arm::A2).take(3) {
            r.cap_exhausted = true;
        }
        let s = summarize(&runs, false);
        assert!(s.sanity_cap_warning.is_some());
        assert_eq!(s.verdict, Verdict::Go, "the sanity bound is not a gate");
    }
}
