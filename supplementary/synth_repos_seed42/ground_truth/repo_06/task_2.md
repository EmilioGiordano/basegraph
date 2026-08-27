# Bug: inverted windows from the legacy importer

The legacy importer sometimes emits windows with `end < start`. Today they
are passed through as-is and end up as nonsense entries in the schedule.

Required behaviour (in `src/scheduling.rs`):

- Add `Window::normalised(&self) -> Window` returning the window with its
  bounds swapped when `end < start` (unchanged otherwise).
- `merge_windows` must treat an inverted window as its normalised form, i.e.
  `merge_windows(&[Window::new(6, 2)])` yields `[Window::new(2, 6)]`, and inverted
  windows merge with the windows they overlap after normalisation.
