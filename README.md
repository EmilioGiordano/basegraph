# CodeGraph

A token-efficient structural index of a codebase, built for LLM agents.

CodeGraph parses a project into a graph of symbols (functions, methods, structs,
enums, traits, ...) and their relationships (who calls whom, who implements what,
which types are used where). An agent can then ask for a **ranked project map**, a
**labeled neighborhood** of any symbol, or **search** for a symbol by name — getting
just the relevant structure in a few hundred tokens instead of reading whole files.

It speaks the **Model Context Protocol**, so Claude Code, Cursor, or any MCP client
can query a codebase natively.

> Status: works on **Rust** codebases. Multi-language is on the roadmap.

## Install

```bash
cargo install --path .
```

This puts the `codegraph` binary on your `PATH`.

## Quickstart

```bash
# 1. Index a codebase (writes <dir>/codegraph.json)
codegraph build ./my-project

# 2. Orient: the most central symbols, ranked, within a token budget
codegraph map ./my-project --format text

# 3. Find a symbol by name
codegraph search ./my-project AcpEvent --format text

# 4. Understand a symbol: its callers, callees, impls, and used types
codegraph context ./my-project start_agent --format text
```

Every query command accepts `--format json` (default) or `--format text`, a
`--budget` (map/context) or `--limit` (search), and an optional `--cache <path>`.

## The output model

`context` labels every symbol by how it relates to the one you asked about:

| Label            | Meaning                                             |
| ---------------- | --------------------------------------------------- |
| `[target]`       | the symbol you queried                              |
| `[caller]`       | calls the target (reverse lookup)                   |
| `[callee]`       | is called by the target                             |
| `[implements]`   | trait the target type implements                    |
| `[implementor]`  | type that implements the target trait               |
| `[uses]`         | type referenced in the target's signature/body      |
| `[used-by]`      | references the target type                          |
| `[co-located]`   | defined in the same file                            |

Call/impl relations rank above type-use relations, which rank above co-located
ones, so a symbol's callers and callees are never crowded out by the types it
touches. Within a tier, results are ordered by centrality.

The graph carries three edge kinds: **Calls** (`fn` → `fn`), **Implements**
(`type` → `trait`), and **Uses** (symbol → the named types in its signature,
fields, or constructed values).

## Ranking

`map` and `search` rank symbols by **PageRank** over the graph. Because types accrue
`Uses` edges from every signature and construction that names them, the ranking
surfaces architecturally central types and functions — not just whatever the file
walker happened to reach first.

## MCP server

Run CodeGraph as an MCP server over stdio for one codebase:

```bash
codegraph mcp ./my-project
```

It exposes three tools — `map`, `context`, `search` — loads the graph once, and
reloads automatically when the cache changes on disk.

Register it with Claude Code:

```bash
claude mcp add codegraph -- codegraph mcp /absolute/path/to/my-project
```

Or add it to any MCP client's config as the command `codegraph` with args
`["mcp", "/absolute/path/to/my-project"]`.

## Caching

`build` writes a versioned `codegraph.json` into the codebase directory (so the
same directory an agent reads also holds the index). An index written by an older,
incompatible format is rejected with a message telling you to rebuild.

## Limitations

CodeGraph is a fast, name-based static analyzer, not a type checker. Honestly:

- **Rust only**, for now.
- **Ambiguous method calls are dropped.** A call to a common method name
  (`new`, `map`, `initialize`) that has many definitions is skipped rather than
  linked to the wrong one — precision over recall. Resolving these needs receiver-type
  inference.
- **Dynamic dispatch is invisible.** Calls that cross a channel, a `spawn`, or a
  trait object aren't static call edges and won't appear as `Calls`.

Missing edges are always omissions, never wrong data.

## Roadmap

- Multi-language parsing (TypeScript first) via tree-sitter.
- Receiver-type inference to resolve ambiguous method calls.
- Transitive `context` (pull a whole subsystem to depth N).
