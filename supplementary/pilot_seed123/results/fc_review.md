# False-confidence review (rubric, supplementary/rubric.md)

Auto-flagged: 1 run. Reviewed transcript: `transcripts/repo_03-task_1-a2-s0.jsonl`.

Sequence: `Read src/matching.rs` (current C3 code, drifted signature) -> `recall affinity_score`
(3 memories, all `evolved`) -> `Edit` against the drifted signature (`first: &Profile`), comment
"Checked both ways to keep the score symmetric" -> `cargo test` (green) -> `remember`.

Decision: NOT false confidence. The agent read the anchored symbol's current code immediately
before the stale recall and edited the live signature; the evolved memory's content (symmetry)
was still valid and the fix preserved it (oracle passes). The auto-score is strictly ordered
(verification only counts *after* the stale item) and did not credit the read that preceded it.

Rubric refinement to consider before the full campaign: count a read of the anchored file within
the N calls preceding the stale recall as verification, or judge on whether the agent relied on
stale *content* (e.g. edited the old name / old signature).
