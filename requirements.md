# Requirements — CodeGraph (working name)

> **Status:** Requirements specification (v1). This document defines **WHAT** the
> system must do — NOT how. Implementation decisions (programming language, parser,
> storage format, algorithms, exact CLI) are deferred to a later **Design** phase and
> listed in section 10. Do **not** start coding from this document alone; wait for the
> design doc and the build task.
>
> **Audience:** an LLM coding agent that will help design and implement the system.
> Read every section before proposing anything.

---

## 1. Purpose & problem

LLM coding agents waste large amounts of tokens re-reading source files, and their
understanding of a codebase is trapped in a single chat session — when the session
resets, context is lost and must be rebuilt (manually maintained `AGENTS.md`/docs, or
by re-reading files).

**CodeGraph** solves this by doing the expensive analysis **once**, deterministically,
and compressing a codebase into a **persistent, queryable graph** of its structure.
Any agent, in any session, can then query that graph to obtain **only the relevant
structural context** for a task, using a small fraction of the tokens that reading the
raw files would cost.

**Primary value:** token efficiency + always-available structural context.
**Secondary value (consequence, not a feature):** by giving agents a clear structural
map, they can more easily infer the codebase's implicit business rules.

## 2. Target user

The consumer is an **LLM coding agent** (e.g. opencode, Claude Code), **not a human**.
Therefore all query output MUST be **structured and machine-consumable**. There is no
human GUI. A human may run the build command, but the product's outputs are for agents.

## 3. Glossary / definitions

- **Codebase**: a single local directory tree of source files (one repository).
- **Symbol**: a named code entity — e.g. a module/file, class, function, or method.
- **Signature**: a symbol's declaration without its body (e.g. `def foo(a, b) -> int`).
- **Graph**: nodes (symbols) connected by typed edges (relationships) — see section 7.
- **Map**: a compressed, project-wide overview (symbols + signatures, no bodies).
- **Context bundle**: the minimal relevant subgraph for a given target symbol/query,
  rendered compactly for an agent to consume.
- **Token budget**: a configurable maximum number of tokens an output may occupy.
- **Cache**: the persisted graph produced by the build step, read by queries.

## 4. Scope

### 4.1 In scope (v1)
- Analyze **one** programming language (which one is a design decision), with an
  **architecture explicitly designed to extend to any language later** (see RNF3).
- Build a **structural graph** of the codebase (section 7).
- **Persist** the graph to a cache that queries read without re-analyzing.
- Two query capabilities: **map** (overview) and **context** (targeted subgraph).
- A **re-runnable build/update command** (idempotent) — re-running it refreshes the
  cache to match the current code (this is how "freshness" is achieved in v1).
- **Structured output** plus a **token-savings report** on every query.

### 4.2 Out of scope (v1) — non-goals
- ❌ Simultaneous multi-language support (architecture must anticipate it; v1 ships one).
- ❌ Automatic/real-time incremental updating or a file-watcher → **roadmap**
  (v1 freshness = re-run the build command, e.g. via an `AGENTS.md` instruction like
  RTK, or a manual/cron invocation).
- ❌ Explicit business-rule / domain-semantics extraction → **V2 / separate branch**.
- ❌ Editing source code.
- ❌ Executing source code.
- ❌ Multiple repositories at once.
- ❌ Human GUI.
- ❌ MCP server integration → **roadmap** (v1 exposes the cached, queryable graph;
  wrapping it as an MCP server for live agent use comes later).

## 5. Functional requirements (v1)

- **RF1 — Build / update**
  A command that takes a codebase directory, analyzes all supported-language source
  files, builds the structural graph, and writes it to the cache.
  - MUST be **idempotent**: same source → same graph.
  - MUST be **re-runnable** to refresh the cache after code changes.
  - MUST report build statistics (files parsed, symbols found, edges found, files
    skipped/unparseable, elapsed time).
  - MUST NOT call an LLM (see RNF2).

- **RF2 — Query: map**
  Produce a compressed, project-wide overview: for each module/file, its top-level
  symbols with **signatures only (no bodies)**, organized readably.
  - MUST respect a configurable **token budget**; when the full map exceeds it, MUST
    include the **most relevant/central symbols first** (ranking is a design decision)
    and clearly indicate truncation.
  - Output MUST be structured/machine-consumable and include its token count.

- **RF3 — Query: context `<target>`**
  Given a target (a symbol name and/or a query string), return the **minimal relevant
  subgraph**: the target's definition(s), its **direct callers**, its **direct callees**,
  and directly related definitions (imports it depends on, base classes, etc.).
  - The goal: enough context for an agent to understand/modify the target **without
    reading whole files**.
  - MUST respect the token budget and rank by relevance when over budget.
  - Output MUST be structured/machine-consumable and include its token count.

- **RF4 — Persistent cache**
  The graph MUST persist between runs and be queryable (RF2/RF3) **without
  re-analyzing** the codebase. Rebuild/refresh happens only via RF1.

- **RF5 — Structured output + token-savings report**
  Every query (RF2/RF3) MUST emit machine-consumable structured output and a
  **token-savings report**: the tokens of the returned bundle vs. an estimate of the
  tokens needed to read the corresponding full source files.

## 6. Non-functional requirements (v1)

- **RNF1 — Token efficiency (vital metric).** Minimizing tokens/context is the primary
  optimization target; outputs must be as compact as possible while preserving the
  information an agent needs.
- **RNF2 — Deterministic indexing.** The graph is built by static analysis only — **no
  LLM, no network, no randomness**. Same input always yields the same graph.
- **RNF3 — Extensible architecture.** Language-specific parsing MUST sit behind a
  clear abstraction so that adding a new language does not require rewriting the graph,
  cache, query, or ranking logic. v1 implements exactly one language behind this
  abstraction.
- **RNF4 — Scales to large codebases.** MUST handle projects with thousands of files
  in reasonable time; queries against the cache MUST be fast (interactive).
- **RNF5 — Lightweight & offline.** Runs fully offline with minimal/no heavy external
  dependencies. Must fail gracefully on files it cannot parse (skip + report, never crash).

## 7. Conceptual data model (WHAT must be captured — storage format is design)

**Node types (at minimum):** Module/File, Class, Function/Method. (Top-level
constants/variables optional.)

**Node attributes (at minimum):** name, kind, fully-qualified name, signature,
source location (file path + line range). Optional: leading docstring/comment.

**Edge types (at minimum):**
- `DEFINES` — a module defines a symbol; a class defines a method.
- `IMPORTS` — a module imports another module/symbol.
- `CALLS` — a function/method calls another function/method.
- `INHERITS` — a class inherits from another class.
- `REFERENCES` — a symbol references another (fallback relation).

**Edge confidence:** each edge SHOULD be tagged as **deterministic** (statically
certain, e.g. an explicit import) or **heuristic** (inferred, e.g. an ambiguous call),
so consumers can trust the graph accordingly.

## 8. Conceptual interface (behavior, not final syntax)

Consumption model (decided): a **cached, queryable graph**. Conceptually there are:
1. a **build/update** operation (RF1),
2. a **map** query (RF2),
3. a **context** query (RF3).

Exact command names, flags, and I/O formats are a **design decision** (section 10),
constrained by "machine-consumable" (RF5) and "configurable token budget" (RF2/RF3).

## 9. Acceptance criteria

The v1 is acceptable when all three are demonstrably met on a real test codebase:

- **AC1 — Token savings.** For a set of representative tasks, the context bundle
  (RF3) / map (RF2) uses **substantially fewer tokens** than reading the corresponding
  full source files. Target ratio to be fixed during design/benchmark (e.g. ≥ 5×);
  the token-savings report (RF5) must quantify it.
- **AC2 — Precision / recall.** For a defined set of test targets, the returned context
  bundle **contains all the symbols** an agent needs to understand/modify that target
  (its definition, callers, callees, and direct dependencies). Measured against a
  hand-labeled expected set.
- **AC3 — Freshness.** After modifying a source file and re-running the build (RF1),
  a subsequent query reflects the change.

## 10. Open design decisions (deferred to the Design phase)

These are intentionally UNDECIDED here and must be resolved in design, derived from
the requirements above:
1. Implementation language of the tool.
2. First target language to analyze, and the parsing approach (must satisfy RNF2/RNF3).
3. Graph representation and **cache format** (must satisfy RF4/RNF4/RNF5).
4. Relevance/centrality **ranking algorithm** for budget-limited output (RF2/RF3).
5. **Token-counting method** for budgets and the savings report (RF5/RNF1).
6. Exact CLI: command names, arguments, and output schema (RF2/RF3/RF5).

## 11. Roadmap (post-v1, not part of this scope)

- Incremental/real-time updates + file watcher.
- Multiple languages simultaneously.
- MCP server for live agent integration.
- Business-rule / domain-semantics extraction (V2).

---

*End of requirements. Next phase: Design (resolve section 10). Do not begin
implementation until the design is agreed and a build task is issued.*
