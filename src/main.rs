//! CodeGraph command-line interface.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use codegraph::builder::build_graph;
use codegraph::cache::{Cache, JsonCache};
use codegraph::query::{self, QueryResult};
use codegraph::tokens::HeuristicCounter;

/// Token-efficient code graph for LLM agents.
#[derive(Parser)]
#[command(name = "codegraph", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a codebase directory and write the graph cache.
    Build {
        /// Root directory of the codebase to analyze.
        path: PathBuf,
        /// Path to the cache file to write.
        #[arg(long, default_value = ".codegraph/graph.json")]
        cache: PathBuf,
    },
    /// Print a compressed, project-wide map of symbols.
    Map {
        /// Maximum number of tokens in the output.
        #[arg(long, default_value_t = 4000)]
        budget: usize,
        /// Path to the cache file to read.
        #[arg(long, default_value = ".codegraph/graph.json")]
        cache: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Print the relevant context bundle for a symbol.
    Context {
        /// Symbol name or fully-qualified name to look up.
        symbol: String,
        /// Maximum number of tokens in the output.
        #[arg(long, default_value_t = 4000)]
        budget: usize,
        /// Path to the cache file to read.
        #[arg(long, default_value = ".codegraph/graph.json")]
        cache: PathBuf,
        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
}

/// Output format for query results.
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    /// Machine-consumable JSON.
    Json,
    /// Human-readable text.
    Text,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Build { path, cache } => {
            let graph = build_graph(&path).context("building graph")?;
            if let Some(parent) = cache.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).context("creating cache directory")?;
                }
            }
            JsonCache::new(&cache)
                .save(&graph)
                .context("saving cache")?;
            println!(
                "Built graph: {} nodes, {} edges -> {}",
                graph.nodes().len(),
                graph.edges().len(),
                cache.display()
            );
        }
        Command::Map {
            budget,
            cache,
            format,
        } => {
            let graph = JsonCache::new(&cache).load().context("loading cache")?;
            let result = query::map(&graph, budget, &HeuristicCounter);
            print_result(&result, format)?;
        }
        Command::Context {
            symbol,
            budget,
            cache,
            format,
        } => {
            let graph = JsonCache::new(&cache).load().context("loading cache")?;
            let result = query::context(&graph, &symbol, budget, &HeuristicCounter);
            print_result(&result, format)?;
        }
    }
    Ok(())
}

/// Print a [`QueryResult`] in the requested format.
fn print_result(result: &QueryResult, format: Format) -> Result<()> {
    match format {
        Format::Json => {
            let json = serde_json::to_string_pretty(result).context("serializing result")?;
            println!("{json}");
        }
        Format::Text => {
            for item in &result.items {
                println!(
                    "[{:?}] {} :: {}  ({}:{})",
                    item.kind, item.fqn, item.signature, item.file, item.line_start
                );
            }
            if result.truncated {
                println!("... (truncated to fit token budget)");
            }
            let r = &result.token_report;
            println!(
                "-- {} bundle tokens vs ~{} full-source tokens ({:.1}x saved)",
                r.bundle_tokens, r.full_source_tokens, r.savings_ratio
            );
        }
    }
    Ok(())
}
