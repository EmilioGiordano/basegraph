# Experiment harness (go/no-go, `go-no-go.md`)

Three binaries in this package implement the empirical chapter: they generate
the materials, run the three-arm experiment and score it against the sealed
thresholds. None of them touches the product code; they share only
`tools/common/` (manifest and run-record schema, PRNG, git and cargo helpers).

```
tools/
├── common/              schema.rs (manifest.json / runs.jsonl), rng.rs, git.rs, cargo.rs, files.rs, mcp_client.rs
├── synth_repo_gen/      scenarios.rs (invariant catalog + ground truth), render.rs, assemble.rs, verify.rs
├── experiment_runner/   agent.rs (claude cli | scripted), capture.rs (memory seeding), run.rs, transcript.rs, plan.rs
└── scorer/              stats.rs (Wilson CIs), verdict.rs (§7 decision rule)
```

Build everything with `cargo build`; `cargo test` includes the unit tests of
the tools and `tests/experiment_integration.rs`, a smoke run of the whole
pipeline with a scripted agent.

## 1. Materials: `synth_repo_gen`

```
synth_repo_gen --out synth_repos --seed 42 --count 10 --drift 7 --noise-commits 4 --verify \
               --supplementary supplementary/synth_repos_seed42
```

- One repo per scenario of the invariant catalog (`scenarios.rs`: sorted
  output, return positivity, non-empty, no panic, idempotence, precondition,
  no side effect, commutativity), 20–60 files each, scripted history
  `C1 base → C2 bug fix leaving a latent invariant → C3 drift (rename | move |
  signature | duplicate) or body-only change`. Commit dates and author are
  fixed, so the same seed reproduces the same SHAs.
- `--noise-commits N` is the findability knob of §3: filler commits buried
  around the fix. `--count 3` gives the pilot batch of §0.
- Per task, next to the repo but never committed: `task_N.md` (the request,
  with the API it needs), `primary_test_N.rs` (validates the task),
  `oracle_test_N.rs` (detects the invariant violation), `fix_correct_N.rs`
  and `fix_wrong_N.rs` (reference fixes replacing the anchored module), plus
  `capture_task.md` (the bug C2 fixed, for the seeding sessions).
- `--verify` self-checks every task with cargo: pristine → primary fails and
  the oracle holds; correct fix → both pass and the repo's own suite stays
  green; wrong fix → primary passes and the oracle fails. A generator bug
  cannot silently become an experimental result.
- `--supplementary` writes what can be versioned: a git bundle per repo, the
  ground truth, the manifest.

## 2. Runs: `experiment_runner`

```
experiment_runner --manifest synth_repos/manifest.json --out results \
                  --model <pinned model id> --max-turns 40 --time-cap-secs 900 --token-cap 200000 --seeds 2
```

Per §4 the memory material is seeded through the real pipeline, once per
repo and arm, before any task run: a fresh session at C1 solves the C2 bug
and then writes `gotchas.md` (A1) or calls `remember` (A2). Raw artifacts,
transcripts and a usability count land in `results/captures/`; nothing is
edited by hand.

Per §5 every task × arm × seed is a fresh agent instance in a fresh clone at
C3, in a seeded random order (`--order-seed`), with the same caps for every
arm:

| Arm | Material injected | Tools | Coaching |
|---|---|---|---|
| A0 | — | Read/Grep/Glob/LS, Edit/Write, `cargo build/test/check`, `git log/show/diff/blame/status` | none |
| A1 | `gotchas.md` | as A0 | "There is a `gotchas.md` …: consult it for the code you are about to touch" |
| A2 | `codegraph-memory.jsonl` + index; MCP server `codegraph mcp` | A0 + `mcp__codegraph__recall`/`remember` | "There is a memory tool (`recall`): consult it …" (same sentence) |

The Claude Code CLI is driven headless (`claude -p … --output-format
stream-json --bare --permission-mode dontAsk --allowedTools …`); `--bare`
keeps CLAUDE.md, hooks and MCP auto-discovery out of the experiment. After
the agent finishes, the primary suite and the oracle are injected and run;
one line per run goes to `results/runs.jsonl` (schema in
`tools/common/schema.rs`), the transcript to `results/transcripts/`. Runs
already recorded are skipped, so an interrupted campaign resumes.

`--agent scripted:<arm=fix,…>` (fix: `correct` | `wrong` | `wrong-blind` |
`noop`) replaces the LLM with a deterministic stand-in that applies the
reference fixes and still goes through `codegraph mcp`; it exists to test
the pipeline, not to produce results.

## 3. Verdict: `scorer`

```
scorer --runs results/runs.jsonl [--overrides fc_overrides.json] [--expanded]
```

Writes `summary.json` (rates with 95% Wilson intervals per arm, overall / drift
/ no-drift, per seed and pooled) and `verdict.txt`. The decision follows §7
verbatim on seed 0 (one run per task, n = 20 / 14 in drift): GO needs A2 < A0
on all tasks and, in drift, A2 < A1 with false confidence ≤ A1; ties or
one-run differences are GREY (one expansion allowed, then `--expanded` turns
a remaining grey into NO-GO); a false-confidence rate more than one run worse
than A1 is NO-GO. Other seeds are scored the same way as sensitivity; a
verdict that flips across seeds is downgraded to GREY. The §7 sanity bound
(A2 hitting the cap where A0/A1 do not) is reported, not gated.

False confidence is auto-scored from the transcript by the rubric in
`supplementary/rubric.md` and can be overridden after manual review.

## Order of work (§9)

1. Pilot: `synth_repo_gen --count 3 --drift 2 --verify`, run by hand. Gate.
2. Full materials with `--verify` and `--supplementary`.
3. `experiment_runner` (seeding happens automatically first).
4. Manual false-confidence review of the drift runs → overrides.
5. `scorer`.

## Deviations and limits to declare

- The Claude Code CLI has no temperature flag; "temperature fixed" (§2) is
  whatever the pinned model's default is. The model is pinned with `--model`
  and recorded in the run log.
- Tokens are the codegraph heuristic (chars/4) over prompt + transcript, plus
  the CLI's reported usage when present; the cap is enforced post hoc on the
  heuristic, and by `--max-turns` / `--time-cap-secs` during the run.
- The oracle of a task depends on the task's API; when the primary fix does
  not compile, the violation is recorded as not evaluable rather than as a
  violation.
- Synthetic repos and a keyword-driven scripted agent are not a substitute
  for the real runs; the smoke test only proves the pipeline.
