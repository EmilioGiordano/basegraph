# Gotchas

- scheduling::merge_windows — the merge loop only compares against `merged.last_mut()`, so it is ONLY correct when windows are visited in ascending `(start, end)` order; the `ordered.sort_by_key` at the top is load-bearing, never remove it or iterate the raw input slice.
- scheduling::merge_windows — because input is sorted, `last.start <= w.start` always holds, which is why only `last.end` is widened on merge; if you ever process unsorted windows you must also take `last.start.min(w.start)`.
- scheduling::merge_windows — output is sorted by `start` and pairwise non-overlapping; downstream code (`first_gap`) relies on this, do not post-process the result in a way that reorders it.
- scheduling::first_gap — assumes a sorted, non-overlapping schedule (i.e. the output of `merge_windows`); calling it on raw request windows returns bogus gaps.
- scheduling::Window::overlaps — bounds are INCLUSIVE, so touching windows (`[1,2]` and `[2,6]`) merge; `[1,2]` and `[3,4]` do NOT merge but `first_gap` treats `end + 1 == next.start` as no gap, keep both sides of that boundary consistent if you change either.
