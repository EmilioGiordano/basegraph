# Feature: resolve `..` segments

Extend `normalize_path` in `src/paths.rs` to resolve parent segments: `a/b/../c`
becomes `a/c`. A `..` with nothing before it is dropped (`../a` becomes
`a`). Everything else is unchanged.
