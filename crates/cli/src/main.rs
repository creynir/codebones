use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "codebones", version, about = "Strip codebases down to their structural skeleton", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Builds or updates the SQLite cache for the given directory
    Index {
        /// The directory to index (defaults to current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Prints the file tree or the skeleton of a specific file
    Outline {
        /// The path to a file or directory
        path: PathBuf,
    },
    /// Retrieves the full source code for a specific symbol or file
    Get {
        /// The symbol name (e.g., `src/main.rs::Database.connect`) or file path
        symbol_or_path: String,
    },
    /// Searches for symbols or text across the repository using FTS5
    Search {
        /// The search query
        query: String,
    },
    /// Packs the repository's skeleton into a single string for LLM context
    Pack {
        /// The directory to pack (defaults to current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Output format (e.g., xml, markdown)
        #[arg(short, long, default_value = "markdown")]
        format: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Index { dir } => {
            codebones_core::api::index(&dir)?;
            println!("Indexing complete");
        }
        Commands::Outline { path } => {
            let result =
                codebones_core::api::outline(std::path::Path::new("."), &path.to_string_lossy())?;
            println!("{}", result);
        }
        Commands::Get { symbol_or_path } => {
            let result = codebones_core::api::get(std::path::Path::new("."), &symbol_or_path)?;
            println!("{}", result);
        }
        Commands::Search { query } => {
            let results = codebones_core::api::search(std::path::Path::new("."), &query)?;
            for res in results {
                println!("{}", res);
            }
        }
        Commands::Pack { dir, format } => {
            let result = codebones_core::api::pack(&dir, &format)?;
            println!("{}", result);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_cli_index_and_get_e2e() {}

    #[test]
    fn test_cli_pack_format() {}

    #[test]
    fn test_cli_search_fts5() {}
}
