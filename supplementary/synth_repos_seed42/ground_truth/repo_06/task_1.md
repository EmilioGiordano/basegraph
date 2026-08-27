# Feature: pinned maintenance windows

Operators need to pin a window so it is scheduled exactly as requested.

Required API (in `src/scheduling.rs`):

- `Window::pinned(start: u32, end: u32) -> Window` creates a pinned window.
- `Window::is_pinned(&self) -> bool`.
- `merge_windows` must return every pinned window exactly as given (never merged
  with a neighbour), while regular windows keep being merged among
  themselves as today. Pinned windows do not absorb regular ones.

Existing behaviour for regular windows must not change.
