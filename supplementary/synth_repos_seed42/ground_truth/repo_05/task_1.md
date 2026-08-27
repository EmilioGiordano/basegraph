# Feature: drop `./` segments

Asset paths coming from templates contain current-directory segments.

Extend `normalize_path` in `src/paths.rs` so that `./` segments are removed:
`a/./b` becomes `a/b` and `./a` becomes `a`. Everything else is unchanged.
