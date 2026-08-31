# A0 calibration probe — pre-registration (pilot 2)

Written 2026-08-31, **before any run of pilot 2**. Materials:
`supplementary/pilot_seed456/` (3 repos × 2 tasks, generated post-redesign,
`--verify` 45/45). This document fixes the probe, its stop rule and its
declared deviations in advance; nothing here changes `go-no-go.md`, which
stays sealed.

## 1. Why a probe instead of the full 18 runs

§7 GO requires **both** (a) A2 violating strictly fewer invariants than A0
over all tasks, and (b) A2 beating A1 in the drift condition. If A0 violates
**0/6**, condition (a) cannot hold: GO becomes arithmetically impossible and
the remaining 12 runs cannot produce signal in any direction. Pilot 1 paid
full price ($12.78, 24 sessions) to learn exactly this.

The probe buys that same information for the A0 third of the cost, and only
that information. It decides nothing about the hypothesis.

## 2. What the probe is

- Arm **A0 only**, all 6 tasks (3 repos × 2), 1 seed.
- **No seeding sessions.** A0 has no memory material, so the six A1/A2
  capture sessions are not run and not paid for at this stage.
- Caps, model and exposure **identical** to pilot 1 and to the eventual
  A1/A2 runs, so nothing but the materials changes between pilots:
  `claude-fable-5`, 40 turns, 900 s, 400 000 heuristic tokens, $8/run,
  capture at C2, primary test exposed, oracle hidden.

**Declared confound.** Pilot 1 ran on Claude Code CLI **2.1.247**; the CLI
on this machine is **2.1.251**. The whole attribution of pilot 2 is "same
model, better materials", so the harness version is recorded here rather
than discovered later. A patch bump is unlikely to move the primary metric,
but it is not nothing and it is not hidden. Record the exact
`claude --version` in the pilot 2 report.

## 3. Declared deviation from the sealed protocol

§5.5 randomises repo/task/arm order across runs. Running A0 first breaks
**arm**-order randomisation. This is declared, not hidden, and is admissible
because:

- it is §0 material calibration ("confirma que los repos e invariantes se
  comportan como se diseñaron"), not the campaign;
- no threshold in §7 is touched;
- repo/task order inside the probe stays randomised (`--order-seed 1`);
- **arm order has no carrier between runs.** §5 mandates a fresh agent
  instance per run, so there is no within-agent carryover for arm-order
  randomisation to guard against. The deviation is nominal, not substantive;
- if the probe passes, the six A0 runs are **reused verbatim** as the A0
  third of pilot 2. The cost is re-ordered, not added.

The reuse claim was verified, not assumed. Against a scripted 6-row A0
`runs.jsonl`, re-invoking the runner with `--arms a0,a1,a2` on the same
`--out` reports `18 runs (6 done, 12 pending)`, executes only the 12 A1/A2
runs, and leaves the six A0 rows byte-identical. Resume is keyed on
`run_id`.

The deviation is reported in the pilot 2 report.

## 4. Stop rule (pre-committed)

| A0 result | Decision |
|---|---|
| **0/6 violations** | **STOP.** Do not run the capture sessions or A1/A2. The materials do not discriminate; per §0 the design gets fixed and the pilot is run again. |
| **≥1/6 violations** | **PROCEED** to seeding + A1/A2 on these same materials, keeping the six A0 runs. |

**Reading a 0/6 result requires `fix_pass`.** A 0/6 violation result is only
diagnosed as "traps too weak" if `fix_pass` is true on **≥5/6** runs. Below
that, the agent never produced a working fix and "did not violate" is
vacuous: the diagnosis is task difficulty or caps, not trap locality, and
the §6 candidate fixes do not apply. (Pilot 1's A0 scored 6/6 `fix_pass`, so
this is a guard, not an expectation. The runner records the column in
`runs_table.md`.)

Reported alongside, **not a gate**: whether any A0 violation falls in the
drift condition (repo_02, repo_03 — 4 of the 6 tasks). If A0 fails only on
repo_01, the decisive RQ2 axis still has no headroom; proceed, but say so
in the report.

## 5. Pre-registered prediction

Recorded before running so that a confirmed prediction counts as evidence
rather than as post-hoc explanation. From an advisor review of the generated
materials on 2026-08-31:

- **repo_01** (`no_panic`, no drift) — *most* likely to violate. The wrong
  fix, `value.split_at(value.len() - 1)`, is the natural implementation of
  suffix parsing and panics on `""` (usize underflow) and on any non-ASCII
  input (`"é"`, `"５m"`). Nothing in `src/config.rs` states the
  total-function requirement.
- **repo_02** (`return_positivity`, signature drift) — intermediate. The
  wrong fix early-returns *above* the `days.max(1)` clamp, and the file
  plants `Style: prefer early returns in new helpers here`. But the clamp is
  visible inside the function under edit.
- **repo_03** (`no_side_effect`, rename drift) — *least* likely to violate.
  `issue_number()`'s `fetch_add` sits 40 lines above the edit site, and the
  C2 regression test `previewing_a_draft_is_stable` asserts the purity rule
  **inside the file the agent must edit**. This is the same scenario that
  produced 0/6 as repo_01 in pilot 1.

If repo_03 violates, this prediction was wrong and the material is stronger
than the review judged. That gets recorded too.

## 6. Known material weaknesses, recorded before the probe

These are the candidate fixes if the stop rule fires, listed now so the
post-probe redesign is not invented after seeing the numbers:

1. **C2 leaves its regression test in the provider file.** In repo_02
   (`short_express_routes_promise_next_day`) and repo_03
   (`previewing_a_draft_is_stable`) the test that encodes the invariant
   lives in the `mod tests` of the file being edited. The generator
   redesign moved the *consumer* to another module but left the *invariant*
   locally visible. Fix: emit C2's regression test under `tests/`.
2. **C2 commit messages are self-diagnostic.** `fix: invoice previews show a
   different number every time` and `fix: express deliveries on short routes
   are promised for today` name the bug outright; `git log --oneline` is 13
   lines and the 8 noise commits are transparently noise
   (`chore: tidy X helpers`). §3 asks for realistic-but-not-explicit.
3. **repo_03 reuses a scenario that already measured nothing.**

## 7. Harness rehearsal (done, $0)

Before spending anything, the A0-only path was exercised end to end with the
scripted agent against these exact materials:

| Rehearsal | Result |
|---|---|
| `--agent scripted:a0=correct` | violation **no** 6/6, `fix_pass=true` 6/6 |
| `--agent scripted:a0=wrong` | violation **yes** 6/6, `fix_pass=true` 6/6 |

So the oracle wiring discriminates through the *runner*, not just through
the generator's `--verify`, on all six tasks. The runner classifies 4 of the
6 tasks as the drift condition, as designed.

Drift visibility was likewise confirmed end to end (`remember` at C2 →
`recall` at C3): repo_01 `intact`, repo_02 `evolved`, repo_03 `orphaned`
with re-anchor candidate `format_invoice`. The pilot-1 material defect
(`duplicate` drift classifying as `intact`) is closed.

## 8. Operational preconditions

- **Rebuild every release binary first.** `experiment_runner` resolves
  `codegraph` as its own sibling and only checks that the file exists.
  `cargo build --release --bin experiment_runner` does *not* rebuild
  `codegraph.exe`; a stale one silently answers `unknown tool: remember` /
  `unknown tool: recall`, which would void arm A2 without any error. Run
  `cargo build --release` (all bins). Not relevant to this A0 probe, but it
  is a precondition for the A1/A2 stage that follows.
- **Work dir outside any `CLAUDE.md` tree.** The default is `<out>/work`,
  which lands inside the codegraph repo and would load its `CLAUDE.md` into
  every run. Pass `--work-dir` explicitly.
- **`CLAUDE_CONFIG_DIR` isolation is the operator's job, not the runner's**
  (`tools/README.md`). The runner sets only the telemetry env vars. Without
  it the operator's hooks, plugins, user settings and user MCP servers load
  into every run, which is not what pilot 1 measured. Point it at a scratch
  directory holding **only** a copy of `~/.claude/.credentials.json`.
  (`--bare` would do the same but refuses a claude.ai login.)
- The operator's global `~/.claude/CLAUDE.md` is loaded from the real home
  directory regardless, reaching every arm alike, as in pilot 1. Declare it
  with the results.

## 9. Commands

```bash
cd C:/Users/giord/Desktop/dashboard/codegraph
cargo build --release

# Isolation (§8): a config dir holding ONLY the credentials — no hooks, no
# plugins, no user settings, no user MCP servers.
mkdir -p C:/Users/giord/Desktop/dashboard/pilot2_cfg
cp ~/.claude/.credentials.json C:/Users/giord/Desktop/dashboard/pilot2_cfg/

CLAUDE_CONFIG_DIR=C:/Users/giord/Desktop/dashboard/pilot2_cfg \
./target/release/experiment_runner.exe \
  --manifest supplementary/pilot_seed456/manifest.json \
  --arms a0 --seeds 1 \
  --agent claude --model claude-fable-5 \
  --max-turns 40 --time-cap-secs 900 --token-cap 400000 --budget-usd 8 \
  --out supplementary/pilot_seed456/results \
  --work-dir C:/Users/giord/Desktop/dashboard/pilot2_work \
  --keep-work

./target/release/scorer.exe \
  --runs supplementary/pilot_seed456/results/runs.jsonl \
  --out supplementary/pilot_seed456/results --table
```

**Reading the result.** The scorer's §7 verdict line is *not* the probe's
output — with a single arm present it necessarily prints
`GREY / not decidable: some arm or condition has no runs`. The probe's
output is the A0 violation rate, from `runs_table.md` or the
`a0: n/6 = … drift n/4` line of `summary.json`.

## 10. Note on the seed

The directory is named `pilot_seed456`, but the manifest records **seed 42**
(the generator's default; no `--seed` was passed). Pilot 1 deliberately used
seed 123, disjoint from the planned seed-42 campaign. That disjointness was
lost here: repo_01–03 of this pilot draw the same scenarios and crates as
the first three repos of a seed-42 campaign. This does not affect the probe
or pilot 2 — it blocks the *campaign*, which must therefore be generated on
a seed other than 42 so that pilot materials do not reappear inside it.

---

## 11. Amendment — 2026-08-31, before any run of pilot 2

Written after a second review of the shipped materials and of
`tools/experiment_runner/capture.rs`, and **before the probe was launched**
(`supplementary/pilot_seed456/results/` was empty; verified). Everything
below amends this pre-registration only. `go-no-go.md` stays sealed; no
threshold in §7 of that document is touched.

### 11.1 The stop rule is split by condition

§4 gated on the total (6 tasks) and demoted the drift subset to "reported
alongside, not a gate". That is inconsistent with §1: the probe exists
because runs that cannot produce signal should not be paid for, and a
violation confined to repo_01 leaves §7 condition (b) — A2 strictly beats
A1 **in the drift condition** — with no headroom, for the same arithmetic
reason a 0/6 kills condition (a). §5's own ordering makes "repo_01 only"
the modal `≥1/6` outcome, so this is the likely branch, not a corner case.

The stop rule is therefore keyed on the drift subset (repo_02, repo_03 —
4 of the 6 tasks):

| A0 result | Decision |
|---|---|
| **drift ≥1** | **PROCEED** to seeding + A1/A2 on these materials. Both axes live. |
| **drift 0, total ≥1** | Condition (b) is untestable on these materials. **Do not launch the full 12.** Spike **2 A1 runs on repo_03** (~$2) to test the discard mechanism of §11.2 first; proceed only if A1 violates at least once there. Otherwise stop and strengthen the drift materials. |
| **total 0**, `fix_pass` ≥5/6 | **STOP.** Materials do not discriminate; fix the design per §0 and re-pilot. (Unchanged.) |
| **total 0**, `fix_pass` <5/6 | **STOP**, but the diagnosis is difficulty or caps, not trap locality; the §6 candidate fixes do not apply. (Unchanged.) |

### 11.2 New material weakness (fourth §6 entry): the payload is anchor-independent

`capture.rs` gives both memory arms the same semantic payload and differs
only in addressing. `CAPTURE_C2_A1` asks for "one line per gotcha,
reference the symbol (fqn)", example `auth::validate_token — NEVER log raw
token, use hash_token(t) for logs`; `CAPTURE_C2_A2` asks for "the invariant
in natural language" anchored to the symbol.

In repo_03 A1's line will therefore read close to `render_invoice — never
advances NEXT_INVOICE; previews must not consume numbers`. At C3
`render_invoice` is gone, but the line still names `NEXT_INVOICE` and
states the requirement outright. What A2 adds over A1 is *which symbol the
rule now applies to* — recoverable from the content itself.

So §7 condition (b) has thin headroom **independently of what A0 does**.
Exactly one mechanism keeps it alive: A1 **discarding** the note as dead
documentation because its anchor does not resolve. If A1 applies the rule
anyway, (b) is dead on these materials. Hanging a GO on that single
behavioural coin-flip at n=4 is not sound, which is why §11.1 routes the
ambiguous branch through a 2-run A1 spike on repo_03.

Recorded as a §0 material-design finding, admissible under go-no-go §0
("si el piloto expone problemas de diseño, se corrige el diseño — no el
protocolo"). The constructive reading: freshness classification, the
system's actual contribution, currently surfaces in the false-confidence
rubric, which §7 makes a side-condition rather than a gate. Strengthening
the drift materials means making the memory *misdirecting* when stale (a
successor whose rule differs, or two plausible successors), not merely
mis-addressed.

### 11.3 Corrections to the §5 prediction

Both corrections are recorded before any number exists.

- **repo_01 is downgraded from "most likely to violate" to "depends on
  rewrite-vs-extend".** §5 was formed from `fix_wrong_1.rs` /
  `fix_correct_1.rs` without the *starting* state. The shipped
  `src/config.rs` already has `parse_timeout` total by construction —
  `strip_suffix` into a nested match, then `parse().unwrap_or(
  DEFAULT_TIMEOUT_MS)`. Adding `m` and `h` by extending that match is the
  minimal-diff path and preserves totality. `value.split_at(value.len() -
  1)` requires electing to rewrite the function rather than extend it, so
  it is one of two rewrite paths, not the natural one.
- **The `Style: prefer early returns in new helpers here` comment is not a
  repo_02 lure.** It is verbatim boilerplate in all three provider files
  (`repo_01/src/config.rs`, `repo_02/src/logistics.rs`,
  `repo_03/src/billing.rs`), alongside three other generic comments. It is
  a uniform decoy, not evidence of targeted difficulty in repo_02.

The §5 ordering (repo_01 > repo_02 > repo_03) is left standing as the
recorded prediction, with repo_01's confidence reduced.

### 11.4 Reuse of the A0 runs depends on the `--seeds` flag

§3's reuse claim holds because `plan.rs` iterates `for seed in 0..seeds`,
so `--seeds 1` yields one seed index and `run_id`s of the form
`repo_01-task_1-a0-s0`. **The A1/A2 stage must pass `--seeds 1` as well.**
A different value changes the `run_id`s, and the six A0 runs would be
re-executed and re-paid instead of reused.
