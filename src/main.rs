//! CodeGraph command-line interface.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use codegraph::builder::build_graph;
use codegraph::cache::{Cache, JsonCache};
use codegraph::graph::Graph;
use codegraph::memory::anchor::{classify, Classification, ReanchorBasis};
use codegraph::memory::model::{Memory, Scope};
use codegraph::memory::store::MemoryStore;
use codegraph::query::{self, ItemView, QueryResult};
use codegraph::tokens::HeuristicCounter;

const CACHE_FILE: &str = "codegraph.json";
const DEFAULT_BUDGET: usize = 4000;

#[derive(Parser)]
#[command(
    name = "codegraph",
    version,
    about = "Token-efficient code graph for LLM agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a codebase directory and write the graph cache inside it.
    Build {
        dir: PathBuf,
        #[arg(long)]
        cache: Option<PathBuf>,
    },
    /// Print a compressed, project-wide map of symbols.
    Map {
        dir: PathBuf,
        #[arg(long, default_value_t = DEFAULT_BUDGET)]
        budget: usize,
        #[arg(long)]
        cache: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Print the relevant context bundle for a symbol.
    Context {
        dir: PathBuf,
        symbol: String,
        #[arg(long, default_value_t = DEFAULT_BUDGET)]
        budget: usize,
        #[arg(long)]
        cache: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Find symbols by name or fully-qualified name.
    Search {
        dir: PathBuf,
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: usize,
        #[arg(long)]
        cache: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Print a symbol's source: capped by default, or --full / --range X:Y / --grep <text> / --outline.
    Show {
        dir: PathBuf,
        symbol: String,
        #[arg(long)]
        full: bool,
        #[arg(long)]
        range: Option<String>,
        #[arg(long)]
        grep: Option<String>,
        #[arg(long)]
        outline: bool,
        /// Human-readable output: faithful indentation and a line number on every line.
        #[arg(long)]
        pretty: bool,
        #[arg(long)]
        cache: Option<PathBuf>,
    },
    /// Report stored memories with their anchor status against the current index.
    Memory {
        dir: PathBuf,
        /// Reference commit the current index corresponds to (informational;
        /// passed in, never read from git).
        #[arg(long)]
        commit: Option<String>,
        #[arg(long)]
        cache: Option<PathBuf>,
    },
    /// Run an MCP server over stdio, exposing map/context/search for the codebase.
    Mcp { dir: PathBuf },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Json,
    Text,
}

fn cache_path(dir: &Path, cache: Option<PathBuf>) -> PathBuf {
    cache.unwrap_or_else(|| dir.join(CACHE_FILE))
}

fn parse_range(s: &str) -> Result<(usize, Option<usize>)> {
    let (a, b) = s.split_once(':').unwrap_or((s, ""));
    let start: usize = a.trim().parse().context("range start")?;
    let end = if b.trim().is_empty() {
        None
    } else {
        Some(b.trim().parse().context("range end")?)
    };
    Ok((start, end))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build { dir, cache } => {
            let path = cache_path(&dir, cache);
            let graph = build_graph(&dir).context("building graph")?;
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).context("creating cache directory")?;
                }
            }
            JsonCache::new(&path).save(&graph).context("saving cache")?;
            println!(
                "Built graph: {} nodes, {} edges -> {}",
                graph.nodes().len(),
                graph.edges().len(),
                path.display()
            );
        }
        Command::Map {
            dir,
            budget,
            cache,
            format,
        } => {
            let path = cache_path(&dir, cache);
            let graph = JsonCache::new(&path).load().context("loading cache")?;
            let result = query::map(&graph, budget, &HeuristicCounter);
            print_result(&result, format)?;
        }
        Command::Context {
            dir,
            symbol,
            budget,
            cache,
            format,
        } => {
            let path = cache_path(&dir, cache);
            let graph = JsonCache::new(&path).load().context("loading cache")?;
            let result = query::context(&graph, &symbol, budget, &HeuristicCounter);
            print_result(&result, format)?;
        }
        Command::Search {
            dir,
            query,
            limit,
            cache,
            format,
        } => {
            let path = cache_path(&dir, cache);
            let graph = JsonCache::new(&path).load().context("loading cache")?;
            let items = query::search(&graph, &query, limit);
            print_items(&items, format)?;
        }
        Command::Show {
            dir,
            symbol,
            full,
            range,
            grep,
            outline,
            pretty,
            cache,
        } => {
            let mode = if outline {
                query::ShowMode::Outline
            } else if let Some(pattern) = grep {
                query::ShowMode::Grep(pattern)
            } else if let Some(r) = range {
                let (a, b) = parse_range(&r).context("invalid --range (use X:Y or X:)")?;
                query::ShowMode::Range(a, b)
            } else if full {
                query::ShowMode::Full
            } else {
                query::ShowMode::Default
            };
            let path = cache_path(&dir, cache);
            let graph = JsonCache::new(&path).load().context("loading cache")?;
            print!("{}", query::show(&graph, &symbol, &mode, !pretty));
        }
        Command::Memory { dir, commit, cache } => {
            let path = cache_path(&dir, cache);
            let graph = JsonCache::new(&path).load().context("loading cache")?;
            let store = MemoryStore::new(&dir);
            let memories = store.materialize().context("loading memory log")?;
            print_memory_report(&memories, &graph, commit.as_deref());
        }
        Command::Mcp { dir } => {
            codegraph::mcp::serve(dir)?;
        }
    }
    Ok(())
}

fn print_memory_report(memories: &[Memory], graph: &Graph, reference_commit: Option<&str>) {
    match reference_commit {
        Some(c) => println!("Memory report: {} memory(ies) @ {c}", memories.len()),
        None => println!("Memory report: {} memory(ies)", memories.len()),
    }
    if memories.is_empty() {
        println!("(no memories)");
        return;
    }
    for m in memories {
        let classification = classify(&m.anchor, graph);
        let scope = match &m.scope {
            Scope::File(p) => format!("file {p}"),
            Scope::Symbol(s) => format!("symbol {s}"),
        };
        println!(
            "- {} [{:?}] {} ({scope})",
            m.id.0,
            m.kind,
            status_label(&classification)
        );
        println!("    {}", m.content);
        println!("    anchor: {} @ {}", m.anchor.fqn, m.anchor.sig_hash);
        if let Classification::ReanchorCandidate { candidates, basis } = &classification {
            println!(
                "    re-anchor candidates (UNCERTAIN, {}): {}",
                basis_label(basis),
                candidates.join(", ")
            );
        }
        let prov = &m.provenance;
        if prov.commit.is_some() || prov.session.is_some() {
            println!(
                "    provenance: commit={} session={}",
                prov.commit.as_deref().unwrap_or("-"),
                prov.session.as_deref().unwrap_or("-")
            );
        }
    }
}

fn status_label(classification: &Classification) -> &'static str {
    match classification {
        Classification::Intact => "intact",
        Classification::Evolved => "evolved",
        Classification::ReanchorCandidate { .. } => "orphaned (uncertain re-anchor)",
        Classification::Orphaned => "orphaned",
    }
}

fn basis_label(basis: &ReanchorBasis) -> &'static str {
    match basis {
        ReanchorBasis::SigHash => "same signature hash",
        ReanchorBasis::ShapeHash => "same signature shape (renamed)",
        ReanchorBasis::TokenSimilarity => "similar name",
    }
}

fn print_items(items: &[ItemView], format: Format) -> Result<()> {
    match format {
        Format::Json => {
            let json = serde_json::to_string_pretty(items).context("serializing items")?;
            println!("{json}");
        }
        Format::Text => {
            for item in items {
                println!("{}", item.render());
            }
            if items.is_empty() {
                println!("(no matches)");
            }
        }
    }
    Ok(())
}

fn print_result(result: &QueryResult, format: Format) -> Result<()> {
    match format {
        Format::Json => {
            let json = serde_json::to_string_pretty(result).context("serializing result")?;
            println!("{json}");
        }
        Format::Text => {
            if let Some(note) = &result.note {
                println!("{note}");
            }
            for item in &result.items {
                println!("{}", item.render());
            }
            if result.truncated {
                println!("... (truncated to fit token budget)");
            }
            if !result.items.is_empty() {
                let r = &result.token_report;
                println!(
                    "-- {} bundle tokens vs ~{} full-source tokens ({:.1}x saved)",
                    r.bundle_tokens, r.full_source_tokens, r.savings_ratio
                );
            }
        }
    }
    Ok(())
}
