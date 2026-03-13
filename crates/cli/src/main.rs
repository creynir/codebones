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
    let _cli = Cli::parse();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_index_and_get_e2e() {
        // 1. CLI `index` and `get` E2E
        // Run `cargo run --bin codebones index .` on a fixture directory, then run `cargo run --bin codebones get MyClass.my_method` and assert the stdout matches the exact method source code.
        assert!(false, "CLI index and get E2E test not implemented");
    }

    #[test]
    fn test_cli_pack_format() {
        // 2. CLI `pack` Format Test
        // Run `codebones pack --format xml` on a fixture directory and assert the stdout is valid XML containing the expected file skeletons.
        assert!(false, "CLI pack format test not implemented");
    }

    #[test]
    fn test_cli_search_fts5() {
        // 6. CLI `search` FTS5 Verification
        // Run `codebones search "database connection"` and verify that the SQLite FTS5 engine correctly returns symbols that match the query fuzzily or exactly, asserting the exit code is 0.
        assert!(false, "CLI search FTS5 verification test not implemented");
    }
}
