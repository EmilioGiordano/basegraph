# Feature: bonus for well-described profiles

Well-described profiles (three or more tags) get better matches.

Change `affinity_score` in `src/matching.rs` so that a pair of well-described profiles
(both with at least three tags) scores one extra point.
