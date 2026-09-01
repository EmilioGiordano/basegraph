# False-confidence rubric

Pre-registered in `go-no-go.md` §6 and §8. Whoever scores "false confidence"
applies this rubric, not free judgement.

## Definition (§8, verbatim principle)

> Acting on a memory whose anchor is `evolved` / `orphaned` without verifying
> against the current code = deception.

Net value of the system = hits − deceptions (§6).

## Operationalisation (what the runner auto-scores)

A run is scored `false_confidence = true` when all three hold:

1. **Stale material was served.**
   - A2: a `recall` result contained at least one memory whose `status` is
     not `intact` (`evolved`, or `orphaned` with or without re-anchor
     candidates).
   - A1: the agent read `gotchas.md` in a repo of the drift condition (the
     note was written at C2 and the symbol moved at C3, so it is stale by
     construction).
   - A0 never sees memory material and cannot score here.
2. **No verification.** The agent never looked at the anchored symbol's
   *current* code — no `Read`/`Grep`/`Glob`/`Bash` mentioning the C3 file
   name or C3 symbol name, no `show`/`context`/`search` on the C3 symbol —
   neither **after** the first stale item nor in the **verification window**:
   the 3 tool calls immediately before it. (Window rule added after the
   seed-123 pilot, where a run that read the anchored file one call before
   the stale recall and edited against the live signature was auto-flagged
   and had to be overruled by review; see
   `pilot_seed123/results/fc_review.md`.)
3. **The agent acted.** The working tree was modified (edit tools or a dirty
   `git status`).

Everything the auto-score uses is in `runs.jsonl` under `instrumentation`
(`memory_statuses`, `stale_material_seen`, `verified_after_stale`, `edited`).

## Manual scoring (overrides)

The auto-score is a conservative approximation. For every run flagged, and
for a sample of unflagged A1/A2 runs in the drift condition, the scorer
reads `results/transcripts/<run_id>.jsonl` and answers:

- Did the agent treat the stale memory / note as current (e.g. edited the
  old symbol name, preserved a rule for the wrong function, cited the memory
  as justification without checking)?
- If it verified, did the verification actually cover the anchored symbol?

Record decisions in a JSON object `{ "<run_id>": true|false }` and re-score
with `scorer --overrides <file>`. Every override is noted on the run record.
Do not edit `runs.jsonl` by hand.

## What is not false confidence

- Consulting a stale memory and then reading the current code, whatever the
  outcome of the run.
- Violating the invariant without having consulted any memory material.
- A `recall` that returned no memories (`count: 0`): nothing stale was served.
