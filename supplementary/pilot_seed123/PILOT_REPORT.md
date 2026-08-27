# Manual pilot — go/no-go §0 (seed 123, 3 repos × 2 tasks × 3 arms)

Date: 2026-08-27. Model: `claude-fable-5` (Claude Code CLI 2.1.247, headless,
`--permission-mode dontAsk`, per-arm tool allowlists, `--strict-mcp-config`),
same caps for every arm (40 turns, 900 s, 400k heuristic tokens, $8). One seed.
Memories seeded at C2 through the real pipeline (protocol prompt), tasks run at
C3 in fresh clones with the primary test exposed and the oracle hidden.
Everything under `results/` is the raw material: `runs.jsonl`, `transcripts/`,
`captures/` (the six seeding sessions and what they produced), `summary.json`,
`verdict.txt`, `runs_table.md`, `fc_review.md`.

## Materials

| Repo | Scenario | Crate | Files | C3 |
|---|---|---|---|---|
| repo_01 | no_side_effect (`render_invoice` must never advance `NEXT_INVOICE`) | orbitdesk | 47 | body-only change (no drift) |
| repo_02 | sorted_output (`merge_windows` returns a sorted schedule; `first_gap` relies on it) | quotaflow | 22 | drift: duplicate (`legacy_scheduling::merge_windows` wrapper) |
| repo_03 | commutativity (`affinity_score(a,b) == affinity_score(b,a)`) | beaconhub | 40 | drift: signature (`left` → `first`) |

`synth_repo_gen --verify` passed all 45 checks before the pilot: on every task
the pristine repo fails the primary test and keeps the invariant, the reference
fix passes both, the obvious wrong fix passes the primary test and fails the
oracle.

## Seeding sessions (§4, real pipeline, nothing hand-edited)

| Repo | Arm | Usable | Entries | Turns | Cost | Observation |
|---|---|---|---|---|---|---|
| repo_01 | A1 | yes | 5 lines | 6 | $0.5 | states the purity rule for `render_invoice` |
| repo_01 | A2 | yes | 2 memories | 7 | $0.5 | invariants on `render_invoice`, `issue_number` |
| repo_02 | A1 | yes | 6 lines | 6 | $0.5 | "output is sorted … `first_gap` relies on this, do not post-process the result in a way that reorders it" |
| repo_02 | A2 | yes | 2 memories | 7 | $0.5 | invariants on `merge_windows`, `first_gap` |
| repo_03 | A1 | yes | 4 lines | – | – | symmetry of `affinity_score` |
| repo_03 | A2 | yes | 3 memories | – | – | symmetry + tag-counting caveat on `affinity_score` |

6/6 usable. With the bare-fqn hint in the prompt every `remember` anchored on
the first try; an earlier seeding variant (C1, "solve the bug then distill",
kept in `results_capture_at_c1/`) showed the write-path friction the goal
anticipated: the agent tried module-qualified names
(`orbitdesk::billing::render_invoice`) and half its `remember` calls were
refused before it recovered.

## The 18 runs

| Repo | Task | Arm | Drift | Oracle passes | Fix passes | Freshness seen (A2) | Read gotchas (A1) | git archaeology | False confidence (auto → reviewed) | Tokens | Time (s) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| repo_01 | task_1 | a0 | no | yes | yes | - | - | no | no | 7803 | 26 |
| repo_01 | task_1 | a1 | no | yes | yes | - | yes | no | no | 9580 | 33 |
| repo_01 | task_1 | a2 | no | yes | yes | intact | - | no | no | 15486 | 57 |
| repo_01 | task_2 | a0 | no | yes | yes | - | - | no | no | 10647 | 36 |
| repo_01 | task_2 | a1 | no | yes | yes | - | yes | no | no | 9805 | 32 |
| repo_01 | task_2 | a2 | no | yes | yes | intact | - | no | no | 15402 | 62 |
| repo_02 | task_1 | a0 | yes | yes | yes | - | - | no | no | 11422 | 47 |
| repo_02 | task_1 | a1 | yes | yes | yes | - | yes | no | no | 14321 | 58 |
| repo_02 | task_1 | a2 | yes | yes | yes | intact | - | no | no | 17599 | 70 |
| repo_02 | task_2 | a0 | yes | yes | yes | - | - | no | no | 7301 | 25 |
| repo_02 | task_2 | a1 | yes | yes | yes | - | yes | no | no | 13007 | 42 |
| repo_02 | task_2 | a2 | yes | yes | yes | intact | - | no | no | 10533 | 35 |
| repo_03 | task_1 | a0 | yes | yes | yes | - | - | no | no | 7475 | 27 |
| repo_03 | task_1 | a1 | yes | yes | yes | - | yes | no | no | 10054 | 38 |
| repo_03 | task_1 | a2 | yes | yes | yes | evolved ×3 | - | no | yes → **no** (see `fc_review.md`) | 11104 | 42 |
| repo_03 | task_2 | a0 | yes | yes | yes | - | - | no | no | 8603 | 30 |
| repo_03 | task_2 | a1 | yes | yes | yes | - | yes | no | no | 7447 | 24 |
| repo_03 | task_2 | a2 | yes | yes | yes | evolved ×3 | - | no | no | 10956 | 37 |

Tokens are the codegraph heuristic (chars/4) over prompt + transcript.

### Aggregates (seed 0, n = 6 tasks, 4 in drift)

| Arm | Violations | Fix passes | Material consulted | git archaeology | False confidence (reviewed) | Mean tokens | Mean time |
|---|---|---|---|---|---|---|---|
| A0 | 0/6 (drift 0/4) | 6/6 | – | 0/6 | – | 8.9k | 32 s |
| A1 | 0/6 (drift 0/4) | 6/6 | 6/6 read `gotchas.md` | 0/6 | 0 | 10.7k | 38 s |
| A2 | 0/6 (drift 0/4) | 6/6 | 6/6 called `recall` | 0/6 | 0 (1 auto-flag overruled) | 13.5k | 51 s |

Scorer (§7 rule): **GREY** — every comparison is a tie (A2 = A0 = A1 = 0
violations). Total cost of the pilot: **$12.78** over 24 sessions (6 seeding
+ 18 runs).

## §0 gate

**Detectable signal in any direction? No.** Zero violations in 18/18 runs;
all three arms are indistinguishable on the primary metric. The only
measurable differences are secondary: A2 costs ~1.5× the baseline in tokens
and time (the retrieval tax), A1 ~1.2×.

**Materials healthy?** Mechanically yes, experimentally no:

- Yes: every task is runnable, every fix passed, oracles and reference fixes
  behave as designed (`--verify`), both memory tools work end to end, 6/6
  captures were usable, `recall` classified correctly (`intact` on the
  no-drift and duplicate repos, `evolved` on the signature-drift repo).
- No: **the traps are too weak for this model.** The baseline never opened
  git (0/6 archaeology) and still avoided every "obvious wrong fix": the
  invariant is visible from a plain read of the anchored module (`first_gap`
  and the load-bearing sort sit next to `merge_windows`; the symmetric term
  in `affinity_score` is one line). The findability knob of §3 is set far too
  low — the invariant is trivially rediscoverable, so A0 wins for free and
  nothing is measured.
- Also: for free functions the **duplicate** drift kind is invisible to the
  anchor (codegraph's fqn is the bare name; the original still matches and
  `recall` says `intact`). It measures ambiguity, not staleness, and must not
  count as drift.

**Decision: NO ESCALAR (do not scale to the 10-repo campaign yet).**
Per §0, the pilot exposed a design problem in the materials; the design gets
fixed and the pilot is run again — the protocol and thresholds stay as sealed.

## What to change before re-piloting

1. Make the invariant non-local: move the consumer that depends on it
   (`first_gap`, the boards, the capacity hints) to another module, and make
   the C2 fix a one-line change whose reason is only in history.
2. Make the wrong fix the path of least resistance: tasks whose *natural*
   implementation breaks the invariant (e.g. "append the pinned windows"
   phrased so the order is not suggested; helpers that need a fresh number).
3. Turn the noise knob up (`--noise-commits`) and bury the C2 fix under
   unrelated commits to the same file.
4. Drop `duplicate` as a drift kind for free-function anchors (or qualify
   fqns by module in the indexer — a product change, out of scope here).
5. Rubric: count a read of the anchored file immediately *before* a stale
   recall as verification (one auto-flag in this pilot was a false positive).

## Incidents

- The account's session limit tripped after the seeding sessions; 17 task runs
  returned instantly with "You've hit your session limit". They were removed
  from `runs.jsonl` (kept in `results/invalid_credit_exhausted/`) and re-run
  after the reset with the cached captures. Only genuine sessions are in the
  table.
- `--bare` cannot be used with a claude.ai login; isolation was achieved with
  an empty `CLAUDE_CONFIG_DIR` (credentials only), a work dir outside any
  `CLAUDE.md` tree and a strict MCP config. The operator's global
  `~/.claude/CLAUDE.md` (an `rtk` command-prefix convention) was still loaded
  by every arm alike; the allowlists accept the `rtk` forms so it cost no
  turns.
- The Claude Code CLI has no temperature flag; "temperature fixed" means the
  model default.
