# Feature: blocked profiles never match

Profiles can block other profiles by name (`Profile::block`).

Change `affinity_score` in `src/matching.rs` so that a blocked pair scores 0: when a
profile has blocked the other one, the affinity is 0 regardless of tags.
