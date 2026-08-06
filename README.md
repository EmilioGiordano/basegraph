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

# 5. Read a symbol's source — whole, a range, matching lines, or an outline
codegraph show ./my-project start_agent
```

Every query command accepts `--format json` (default) or `--format text`, a
`--budget` (map/context) or `--limit` (search), and an optional `--cache <path>`.

## Commands

The intended flow: **build** once to index, **map** to orient, **search** to find a
symbol's name, **context** to understand its relationships and change impact, **show**
to read the code and act — and **mcp** to expose all of it to an agent.

| Command             | What it does                                                                                          | Key flags                              |
| ------------------- | ---------------------------------------------------------------------------------------------------- | -------------------------------------- |
| `build <dir>`       | Index the codebase → writes `<dir>/codegraph.json` (with PageRank precomputed)                        | `--cache`                              |
| `map <dir>`         | Project map ranked by centrality, capped at a token budget                                            | `--budget`, `--format`                 |
| `search <dir> <q>`  | Find symbols by name / fqn, ranked by relevance and centrality                                        | `--limit`, `--format`                  |
| `context <dir> <s>` | A symbol's neighborhood, each line labeled: caller/callee/implements/implementor/uses/used-by/co-located | `--budget`, `--format`              |
| `show <dir> <s>`    | Read a symbol's source live from the file                                                             | `--full`, `--range`, `--grep`, `--outline`, `--pretty` |
| `mcp <dir>`         | MCP server over stdio, exposing map/context/search to any MCP client                                  | —                                      |

All commands accept `--cache <path>` (default `<dir>/codegraph.json`); query commands
accept `--format json|text`. See [Reading source with `show`](#reading-source-with-show)
for `show`'s modes.

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

## Reading source with `show`

`show <symbol>` prints a symbol's source, read live from the file — nothing is
duplicated in the cache. It takes a name or fully-qualified name and prints each
match. `context`/`search` already show every symbol's line range (e.g.
`bridge.rs:328-369`), so you know a symbol's size before reading it. Modes:

| Flag            | What it prints                                                                        |
| --------------- | ------------------------------------------------------------------------------------- |
| *(none)*        | a preview capped at 200 lines, with a header stating the total                        |
| `--full`        | the entire body                                                                       |
| `--range X:Y`   | absolute file lines `X` to `Y` (`X:` means `X` to the end)                             |
| `--grep <text>` | only lines matching `<text>` (case-insensitive) with context, grouped into segments   |
| `--outline`     | a skeleton: the signature plus control-flow headers and match arms                    |

Output is **compact by default** — dedented, with line numbers only where they
are not derivable: `--range` omits them, `--grep` heads each segment with
`@ start-end`, and `--outline` keeps them (they are the navigation). Pass
`--pretty` for human-readable output: faithful indentation and a line number on
every line.

`--outline` is AST-based, so control flow inside macros (e.g. `tokio::select!`) is
invisible to it — use `--grep` for those, since it works on text.

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
