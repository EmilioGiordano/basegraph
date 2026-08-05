# Design — CodeGraph v1

> **Status:** Design. Resolves the open decisions from `requirements.md` §10.
> Read `requirements.md` first, then this, then `RUST_GUIDELINES.md`.
> This document defines **HOW**. Implementation must follow it.

## 1. Decisions (resolving requirements §10)

| # | Decision | Choice | Rationale |
|---|---|---|---|
| 1 | Implementation language | **Rust** (edition 2021) | Performance, scale, single binary; chosen by product owner. |
| 2 | First analyzed language + parser | **Rust source via the `syn` crate**, behind a `LanguageParser` trait | Deterministic, pure-Rust (RNF2); enables **dogfooding** (analyze our own code); trait keeps it extensible (RNF3). |
| 3 | Graph + cache | In-memory graph via **`petgraph`**; persistent cache as **JSON via `serde`**, behind a `Cache` trait | Simple, deterministic, easy to implement/verify. Trait allows a SQLite backend later for scale (see §6). |
| 4 | Ranking | **Personalized PageRank** over the reference graph (deterministic); degree-count fallback | Proven approach (aider); relevance-ranked, budget-aware. |
| 5 | Token counting | **`TokenCounter` trait**; v1 default = heuristic estimator | Deterministic, offline, dependency-free; a real BPE (`tiktoken-rs`) can plug in later. |
| 6 | CLI + output | **`clap`** (derive) subcommands; **JSON output by default** (+ text for debug) | Machine-consumable for agents (RF5). |

## 2. Module architecture

```
src/
├── main.rs            # wiring only
├── model.rs           # domain types: Node, Edge, NodeKind, EdgeKind, ids
├── parser/
│   ├── mod.rs         # trait LanguageParser
│   └── rust.rs        # RustParser (syn) — the only impl in v1
├── graph.rs           # build in-memory graph (petgraph) + cross-file linking
├── cache/
│   ├── mod.rs         # trait Cache
│   └── json.rs        # JsonCache (serde) — the only impl in v1
├── rank.rs            # personalized PageRank
├── tokens.rs          # trait TokenCounter + HeuristicCounter
├── query.rs           # map + context logic
└── cli.rs             # clap definitions
```

Everything language-specific lives behind `LanguageParser`; everything storage-specific behind `Cache`; everything tokenization-specific behind `TokenCounter`. The core (model, graph, rank, query) must not depend on a concrete language, storage, or tokenizer.

## 3. Data model

```
NodeId(u32)                         // newtype
Node { id, kind, name, fqn, signature, file, line_start, line_end, doc }
Edge { src: NodeId, dst: NodeId, kind: EdgeKind, confidence: Confidence }

NodeKind  = Module | Struct | Enum | Trait | Function | Method | Const   (extensible)
EdgeKind  = Defines | Uses | Calls | Implements | References
Confidence = Deterministic | Heuristic
```

`Uses` = an import/`use`. `Implements` covers trait impls / inheritance-like relations. Every edge is tagged `Deterministic` (statically certain, e.g. a `use` path) or `Heuristic` (inferred, e.g. a call resolved by name).

## 4. Parsing approach (Rust via `syn`)

1. Traverse the codebase for `.rs` files (`walkdir`), honoring a skip list (`target/`, hidden dirs).
2. Parse each file with `syn::parse_file` (features: `full`, `extra-traits`).
3. Extract items: `fn`, `struct`, `enum`, `trait`, `impl`, `mod`, `use`. Build a `Node` per symbol with its **signature** (declaration, no body) and source location.
4. Source locations (line spans) via `proc-macro2` with the `span-locations` feature.
5. Edges: `use` paths → `Uses` (Deterministic). Method/function call expressions inside bodies → `Calls` (Heuristic, resolved by name). Trait impls → `Implements`.
6. A file that fails to parse is **skipped and reported**, never a crash (RNF5).
7. **No LLM, no network, no randomness** anywhere in indexing (RNF2).

## 5. Cache (v1 = JSON)

- Default path: `.codegraph/graph.json`.
- `JsonCache` serializes `{ nodes, edges, meta }` with `serde_json`.
- Deterministic output: sort nodes and edges by id before writing (reproducible builds).
- `Cache` trait: `save(&Graph)`, `load() -> Result<Graph>`.
- `build` writes the cache; `map`/`context` load it and never re-parse (RF4).

## 6. Scale path (documented, NOT in v1)

If holding the full graph in memory ever strains large codebases, implement `SqliteCache`
behind the same `Cache` trait (`rusqlite`, bundled): store nodes/edges in tables, load only
the compact adjacency (`NodeId → NodeId` + kind) into `petgraph` for ranking, and fetch
signatures/text on demand. **No changes to core, query, or ranking code.**

## 7. Ranking

Personalized PageRank over the node reference graph (all edge kinds):
- **map**: uniform teleport → global structural centrality.
- **context**: teleport vector biased toward the target node(s) → relevance to the target.
- Fixed damping `0.85`, fixed iteration count (e.g. 30), deterministic node ordering → reproducible. Degree-count ranking is an acceptable fallback if PageRank is deferred.

## 8. Token budget & savings

- `TokenCounter::count(&str) -> usize`. v1 `HeuristicCounter` ≈ `chars/4` (documented approximation; consistency matters more than exactness for the ratio).
- `map`/`context` render items in rank order, accumulating until the `--budget` is reached; set `truncated = true` when items are dropped.
- **Savings report** (RF5): `bundle_tokens` vs `full_source_tokens` (tokens of the full source files the bundle's symbols come from) and `savings_ratio = full / bundle`.

## 9. CLI & output schema

```
codegraph build   <path> [--cache <file>]
codegraph map            [--budget N] [--format json|text] [--cache <file>]
codegraph context <sym>  [--budget N] [--format json|text] [--cache <file>]
```

JSON output (default):
```json
{
  "items": [ { "fqn": "...", "kind": "...", "signature": "...", "file": "...", "lines": [s,e], "relations": {"callers": [...], "callees": [...], "uses": [...]} } ],
  "truncated": false,
  "token_report": { "bundle_tokens": 0, "full_source_tokens": 0, "savings_ratio": 0.0 }
}
```
`text` format is a compact human-readable rendering for debugging only.

## 10. Approved dependencies

`syn` (features: full, extra-traits), `proc-macro2` (span-locations), `quote` (if needed),
`petgraph`, `serde` + `serde_json`, `clap` (derive), `walkdir`, `anyhow` (binary),
`thiserror` (library). **Do not add any other crate without approval.**
Deferred/optional (not v1): `rayon` (parallel build), `rusqlite` (scale), `tiktoken-rs` (tokens).

## 11. Build, verify & acceptance

- Toolchain: **GNU** (`cargo +stable-x86_64-pc-windows-gnu ...`) — MSVC linker is unavailable on this machine.
- Quality gates: `cargo build`, `cargo test`, `cargo clippy` (no warnings), `cargo fmt --check`.
- **Dogfood test:** run `codegraph build` on CodeGraph's own `src/`, then `map` and `context`, and confirm the acceptance criteria (requirements §9): token savings, recall of related symbols, and freshness after an edit.
```
