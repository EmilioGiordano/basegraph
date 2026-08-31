# GOAL: Synthetic Repo Generator + Go/No-Go Experiment Runner

## Context
This is the **empirical chapter (Chapter 2)** of the thesis. The protocol is pre-registered and sealed in `go-no-go.md` (v2). Implement the generator, runner, and scorer to produce a GO/NO-GO verdict against the pre-committed thresholds.

## What to Build

### 1. Synthetic Repo Generator (`tools/synth_repo_gen/`)
A Rust binary (or Python script) that generates **10 Rust repos** with scripted git history.

#### Repo Structure (each repo: 20–60 files)
```
repo_01/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── module_a.rs
│   ├── module_b.rs
│   └── ...
└── .git/ (scripted history)
```

#### Git History (scripted, 3 commits per repo)
| Commit | Purpose | Details |
|---|---|---|
| **C1: Base** | Working codebase | 20–60 files, multiple modules, traits, structs, fns |
| **C2: Bug + Fix + Latent Invariant** | Introduce bug, fix it, leave invariant | Bug fix message mentions fix **without** stating the invariant explicitly. The invariant must be checkable by a deterministic test (see "Invariant Catalog" below). |
| **C3: Drift (7 of 10 repos)** | Refactor that moves/renames anchored symbol | Rename fn, move fn to new module, change signature, or duplicate type. **No semantic change** — only syntactic. The other 3 repos: only body changes (no drift). |

#### Invariant Catalog (each repo picks ONE)
Each invariant must have a **deterministic test oracle** (pre-written, compiles, passes on C2, fails on violation):

| Invariant Type | Natural Language (for memory) | Oracle Test |
|---|---|---|
| **Return positivity** | "function `X` always returns positive" | `assert!(result > 0)` |
| **Non-null/non-empty** | "function `X` never returns null/empty" | `assert!(!result.is_empty())` / `assert!(result.is_some())` |
| **Sorted output** | "function `X` returns sorted vec" | `assert!(result.windows(2).all(|w| w[0] <= w[1]))` |
| **No panic** | "function `X` never panics on valid input" | `assert!(std::panic::catch_unwind(|| X(input)).is_ok())` |
| **Idempotence** | "calling `X` twice equals calling once" | `assert_eq!(X(input), X(X(input)))` |
| **Precondition** | "function `X` requires input > 0" | `assert!(std::panic::catch_unwind(|| X(-1)).is_err())` |
| **No side effect** | "function `X` does not mutate global state" | `let before = GLOBAL.load(); X(); assert_eq!(GLOBAL.load(), before)` |
| **Commutativity** | "`X(a, b) == X(b, a)`" | `assert_eq!(X(a,b), X(b,a))` |

**Critical:** The invariant is **latent** — the bug fix in C2 leaves it true, but the commit message does NOT mention it. The "obvious wrong fix" for the task would violate it.

#### Task Definition (2 per repo = 20 tasks total)
Each task = a bug report or feature request written in plain text.
- **Correct fix:** satisfies the task AND preserves the invariant.
- **Obvious wrong fix:** satisfies the task BUT violates the invariant.
- The oracle test detects the violation (binary: pass/fail).

#### Output Format
```
synth_repos/
├── repo_01/ (drift)
│   ├── .git/
│   ├── Cargo.toml
│   ├── src/...
│   ├── task_1.md
│   ├── task_2.md
│   └── oracle_test_1.rs / oracle_test_2.rs  (pre-written, compiles)
├── repo_02/ (no drift)
│   ...
└── manifest.json  # { repo_id, drift: bool, invariant_type, anchor_fqn, tasks: [...] }
```

---

### 2. Experiment Runner (`tools/experiment_runner/`)
Orchestrates the 3-arm experiment on the 10 generated repos.

#### Arms (same agent, same model, pinned version, temp fixed)
| Arm | Tools | Coaching |
|---|---|---|
| **A0 — Baseline** | `read`/`grep`/`glob` + `git log` | None |
| **A1 — Markdown** | A0 + `gotchas.md` (curated) | "There is a `gotchas.md`, consult it for what you're about to touch" |
| **A2 — Anchored** | A0 + MCP `recall`/`remember` | "There is a memory tool, consult it" |

**Coaching is symmetric** for A1/A2 (measures quality + freshness, not tool discovery). A0 gets no extra.

#### Procedure per Task (fresh agent instance each run)
1. Clone fresh repo at C3 (post-drift or post-body-change).
2. Inject memories for A1/A2:
   - A1: Agent session solves C2 bug → distills `gotchas.md` (realistic quality, not oracle).
   - A2: Agent session solves C2 bug → writes via `remember` (anchor, kind, content, commit).
   - **Do not hand-edit memories.** If capture produces garbage, that's a finding.
3. Fresh agent instance per run, arm-specific tools + coaching.
4. Token/time cap (same for all arms). Cap exhaustion = primary metric failure if no valid fix.
5. Run: primary fix test suite + **oracle test** (invariant violation).
6. Record: violation (Y/N), primary fix pass/fail, tokens, time, tool calls, memory consulted (A2), md read (A1), git archaeology (A0).
6. **2 seeds per task** (sensitivity, not independent n).

#### Drift Condition Power
- 7 repos with drift → 14 tasks in drift condition (main comparison A2 vs A1)
- 3 repos without drift → 6 tasks (A2 vs A0 absolute value)

---

### 3. Scorer & Verdict (`tools/scorer/`)
Reads all run logs, computes metrics, emits verdict.

#### Primary Metrics (binary)
- **Violation rate** = % runs where oracle test fails (invariant violated).

#### Decision Thresholds (pre-committed, from `go-no-go.md` §7)
- **GO:** (a) A2 violation rate < A0 violation rate (strictly) on all 20 tasks, **AND** (b) in drift condition (14 tasks), A2 violation rate < A1 violation rate (strictly) with false-confidence rate ≤ A1.
- **NO-GO:** A2 !< A0, OR in drift A2 !< A1, OR A2 false-confidence materially worse than A1.
- **Grey zone:** ties or 1-run difference → one expansion to 3 tasks/repo (n=30, 21 drift). If still grey → NO-GO.
- **Sanity bound:** if A2 hits token cap systematically where A0/A1 don't, report and weigh.

#### False Confidence Metric (drift condition)
Rate of runs where agent cites a memory whose anchor is `Evolved`/`Orphaned` as if it were `Intact` and acts on it. **Net value = hits − deceptions.**

#### Output
```
results/
├── runs.jsonl          # one line per run: repo, task, arm, seed, violation, fix_pass, tokens, time, tools_called, memory_consulted, false_confidence
├── summary.json        # aggregated rates, CIs, verdict
└── verdict.txt         # GO / NO-GO / GREY
```

---

## Integration with CodeGraph
- The experiment uses **CodeGraph binary** (`codegraph mcp ...`) for A2 arm.
- A1 arm: writes/reads a `gotchas.md` in the repo root.
- A0 arm: only standard tools + `git log`.
- All arms share the same **agent invocation logic** (Claude Code CLI, pinned model, temp=0).
- Token counting: use the same heuristic counter as CodeGraph (`chars/4`).

---

## Deliverables (Definition of Done)
1. `tools/synth_repo_gen/` — binary that generates `synth_repos/` + `manifest.json` from a seed.
2. `tools/experiment_runner/` — orchestrates 120 runs (or 60 if 1 seed), produces `results/runs.jsonl`.
3. `tools/scorer/` — reads `runs.jsonl`, emits `summary.json` + `verdict.txt` with GO/NO-GO.
4. `tests/experiment_integration.rs` — smoke test: generates 1 mini repo, runs 1 task × 3 arms, verifies pipeline.
5. `cargo build && cargo test && cargo clippy` clean.
6. **All artifacts versioned** (repos, memories, logs, rubric) as supplementary material.

---

## What NOT to Do
- ❌ Don't implement the agent — use existing CLI (`claude` command).
- ❌ Don't hand-write memories; capture them via the real pipeline (session → `remember` / distill → `gotchas.md`).
- ❌ Don't change `go-no-go.md` thresholds; they are sealed.
- ❌ Don't add ML/NLP; oracle tests are deterministic Rust code.

---

## Reference Files (in repo)
- `go-no-go.md` — full pre-registered protocol (read first)
- `doc-estado-y-tesis.md` — theoretical framing
- `AGENT_INSTRUCTIONS.md` — style guide for CodeGraph code

---

## Suggested Implementation Order
1. **Generator** (standalone, testable in isolation) → produces `synth_repos/`.
2. **Oracle test templates** — parametrized by invariant type.
3. **Runner skeleton** — clones repo, invokes agent CLI, captures output.
4. **Memory capture pipeline** — A1: session → `gotchas.md`; A2: session → `remember`.
5. **Scorer** — pure data analysis, no external deps.
6. **Integration test** — end-to-end on 1 repo.

---

## Questions / Clarifications
If anything is ambiguous, **stop and ask**. The `go-no-go.md` is the source of truth for any decision not explicit here.