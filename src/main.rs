//! CodeGraph command-line interface.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use codegraph::builder::build_graph;
use codegraph::cache::{Cache, JsonCache};
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
        Command::Mcp { dir } => {
            codegraph::mcp::serve(dir)?;
        }
    }
    Ok(())
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
