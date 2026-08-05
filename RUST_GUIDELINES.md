# Rust Coding Guidelines — CodeGraph

> **Audience:** the implementation agent (GPT-OSS local). Follow these rules on **every**
> change. Code that violates them will be sent back for rework. Goal: idiomatic, safe,
> maintainable, extensible Rust — not just "code that compiles".

## Non-negotiables

- **No `unsafe`.** v1 has zero need for it.
- **No `.unwrap()` / `.expect()` / `panic!` in library code.** Return `Result`. `unwrap` is
  allowed only inside `#[cfg(test)]` tests, or with an inline comment proving it is infallible.
- **Deterministic core:** no randomness, no wall-clock time, no network, no environment
  reads inside indexing/analysis. Same input → same output (RNF2).
- **Only the approved dependencies** listed in `design.md` §10. Do not add crates without asking.
- Code must pass `cargo clippy` with **no warnings** and be `cargo fmt`-formatted.

## Error handling

- Library modules define errors with **`thiserror`**; the binary (`main`) uses **`anyhow`**.
- Propagate with `?`. Add context where it helps debugging.
- Never silently swallow errors. Unparseable source files are **skipped and reported**, not fatal.

## Types & API design

- Use **newtypes** for identifiers (e.g. `struct NodeId(u32)`), not raw integers.
- Model states/kinds with **enums**, not stringly-typed values.
- Prefer `&str` over `String` and `&[T]` over `Vec<T>` in function parameters.
- Avoid needless `.clone()`; borrow first, clone only when ownership is genuinely required.
- Derive traits deliberately (`Debug`, `Clone`, `PartialEq`, `Eq`, `Hash`, `serde::{Serialize, Deserialize}`) — only what a type needs.
- Keep fields **private**; expose behavior through methods. `pub` only what the module boundary requires.

## Structure & style

- **Program to the traits** defined in the design (`LanguageParser`, `Cache`, `TokenCounter`).
  The core (`model`, `graph`, `rank`, `query`) must not depend on any concrete impl.
- Small, single-purpose functions. Prefer **early returns** over deep nesting.
- Prefer **iterator chains** over manual index loops when it reads more clearly.
- Follow standard naming: `snake_case` items, `CamelCase` types, `SCREAMING_SNAKE_CASE` consts.
- One responsibility per module; keep the module tree in `design.md` §2.

## Documentation & tests

- `//!` module-level docs on each module; `///` doc comments on every public item.
- Unit tests in `#[cfg(test)] mod tests {}` next to the code; broader tests in `tests/`.
- Test **edge cases**, not just the happy path (empty input, unparseable file, missing symbol,
  budget exceeded, cycles in the graph).
- Every feature lands with tests that actually exercise it.

## Performance (secondary to correctness)

- Correctness and clarity first; then avoid obvious `O(n²)` scans over all symbols.
- Build indexes/maps (`HashMap`) for name/id lookups instead of repeated linear searches.

## Workflow reminders (enforced by the orchestrator)

- Work happens on feature branches; commits are small, focused, Conventional-Commits style
  (`feat:`, `fix:`, `test:`, `docs:`, `refactor:`, `chore:`), English, brief but descriptive.
- Do not write files outside the task's scope. When told to fix an error, fix **that** and
  keep the change minimal.
