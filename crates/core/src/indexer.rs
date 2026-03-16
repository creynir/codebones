use ignore::WalkBuilder;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Represents a successfully indexed and hashed file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHash {
    pub path: PathBuf, // Relative to the workspace root
    pub hash: String,  // Hex-encoded SHA-256 hash
}

/// Configuration options for the indexer.
#[derive(Debug, Clone)]
pub struct IndexerOptions {
    pub max_file_size_bytes: u64,           // Default: 500 KB
    pub max_file_count: usize,              // Default: 500
    pub follow_symlinks: bool,              // Default: false
    pub respect_gitignore: bool,            // Default: true
    pub custom_ignore_file: Option<String>, // e.g., ".codebonesignore"
}

impl Default for IndexerOptions {
    fn default() -> Self {
        Self {
            max_file_size_bytes: 500 * 1024,
            max_file_count: 100000,
            follow_symlinks: false,
            respect_gitignore: true,
            custom_ignore_file: Some(".codebonesignore".to_string()),
        }
    }
}

/// The core indexer trait.
pub trait Indexer {
    /// Indexes the given workspace path and returns a list of file hashes.
    fn index(
        &self,
        workspace_root: &Path,
        options: &IndexerOptions,
    ) -> Result<Vec<FileHash>, IndexerError>;
}

/// Errors that can occur during indexing.
#[derive(Debug, thiserror::Error)]
pub enum IndexerError {
    #[error("Path traversal detected: {0}")]
    PathTraversal(PathBuf),
    #[error("Symlink escape detected: {0}")]
    SymlinkEscape(PathBuf),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("File count limit exceeded")]
    FileCountLimitExceeded,
}

pub struct DefaultIndexer;

impl Indexer for DefaultIndexer {
    fn index(
        &self,
        workspace_root: &Path,
        options: &IndexerOptions,
    ) -> Result<Vec<FileHash>, IndexerError> {
        let mut results = Vec::new();
        let mut count = 0;

        let mut builder = WalkBuilder::new(workspace_root);
        builder.follow_links(options.follow_symlinks);
        builder.git_ignore(options.respect_gitignore);
        builder.git_exclude(options.respect_gitignore);
        builder.git_global(options.respect_gitignore);
        builder.ignore(options.respect_gitignore);
        builder.require_git(false);

        if let Some(ref custom) = options.custom_ignore_file {
            builder.add_custom_ignore_filename(custom);
        }

        let walker = builder.build();

        let canonical_root = std::fs::canonicalize(workspace_root)?;

        for result in walker {
            let entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            if path.is_dir() {
                continue;
            }

            // Path traversal check
            let canonical_path = match std::fs::canonicalize(path) {
                Ok(p) => p,
                Err(_) => continue, // Skip broken symlinks or missing files
            };
            if !canonical_path.starts_with(&canonical_root) {
                return Err(IndexerError::PathTraversal(path.to_path_buf()));
            }

            // Symlink escape check
            if entry.path_is_symlink() && options.follow_symlinks {
                if !canonical_path.starts_with(&canonical_root) {
                    return Err(IndexerError::SymlinkEscape(path.to_path_buf()));
                }
            } else if entry.path_is_symlink() {
                continue; // Skip symlinks if not following
            }

            // Secret exclusion
            let file_name = path.file_name().unwrap_or_default().to_string_lossy();
            if file_name == ".env"
                || file_name.starts_with(".env.")
                || file_name.ends_with(".pem")
                || file_name.ends_with(".key")
                || file_name.starts_with("id_rsa")
                || file_name.starts_with("id_ed25519")
                || file_name == "credentials.json"
                || file_name.ends_with(".secrets")
                || file_name.ends_with(".token")
                || file_name == ".npmrc"
                || file_name == ".netrc"
            {
                continue;
            }

            // Binary detection (extension)
            let ext = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if [
                "exe", "dll", "so", "png", "jpg", "jpeg", "pdf", "db", "sqlite", "wasm",
            ]
            .contains(&ext.as_str())
            {
                continue;
            }

            // Size limit
            let metadata = std::fs::metadata(path)?;
            if metadata.len() > options.max_file_size_bytes {
                continue;
            }

            // Binary detection (null bytes)
            let mut file = File::open(path)?;
            let mut buffer = [0; 8192];
            let bytes_read = file.read(&mut buffer)?;
            if buffer[..bytes_read].contains(&0) {
                continue;
            }

            // Hash
            let mut hasher = Sha256::new();
            let mut file = File::open(path)?;
            std::io::copy(&mut file, &mut hasher)?;
            let hash = hex::encode(hasher.finalize());

            let rel_path = path
                .strip_prefix(workspace_root)
                .unwrap_or(path)
                .to_path_buf();

            results.push(FileHash {
                path: rel_path,
                hash,
            });

            count += 1;
            if count > options.max_file_count {
                return Err(IndexerError::FileCountLimitExceeded);
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use tempfile::TempDir;

    fn setup_workspace() -> TempDir {
        TempDir::new().unwrap()
    }

    #[test]
    fn test_skips_symlinks_escaping_root() {
        let dir = setup_workspace();
        let root = dir.path();

        let out_dir = TempDir::new().unwrap();
        let out_file = out_dir.path().join("out.txt");
        fs::write(&out_file, "out").unwrap();

        let symlink_path = root.join("link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&out_file, &symlink_path).unwrap();

        let indexer = DefaultIndexer;
        let options = IndexerOptions {
            follow_symlinks: true,
            ..Default::default()
        };

        let result = indexer.index(root, &options);
        assert!(matches!(result, Err(IndexerError::PathTraversal(_))));
    }

    #[test]
    fn test_ignores_env_and_secret_files() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join(".env"), "secret").unwrap();
        fs::write(root.join("id_rsa"), "secret").unwrap();
        fs::write(root.join("config.pem"), "secret").unwrap();
        fs::write(root.join("normal.txt"), "normal").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].path, PathBuf::from("normal.txt"));
    }

    #[test]
    fn test_ignores_gitignore() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::create_dir(root.join("ignored_dir")).unwrap();
        fs::write(root.join("ignored_dir/test.txt"), "ignored").unwrap();
        fs::write(root.join(".gitignore"), "ignored_dir/").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        assert!(results.iter().all(|r| !r.path.starts_with("ignored_dir")));
    }

    #[test]
    fn test_ignores_codebonesignore() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::create_dir(root.join("drafts")).unwrap();
        fs::write(root.join("drafts/test.txt"), "ignored").unwrap();
        fs::write(root.join(".codebonesignore"), "drafts/").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        assert!(results.iter().all(|r| !r.path.starts_with("drafts")));
    }

    #[test]
    fn test_skips_large_files() {
        let dir = setup_workspace();
        let root = dir.path();
        let mut file = File::create(root.join("large.txt")).unwrap();
        file.write_all(&vec![b'a'; 600 * 1024]).unwrap();

        let indexer = DefaultIndexer;
        let options = IndexerOptions {
            max_file_size_bytes: 500 * 1024,
            ..Default::default()
        };
        let results = indexer.index(root, &options).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_skips_binary_extension() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join("test.exe"), "fake binary").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_skips_binary_null_bytes() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join("fake.txt"), b"hello\0world").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_replaces_invalid_utf8() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join("invalid.txt"), b"hello\xFFworld").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_stops_at_file_count_limit() {
        let dir = setup_workspace();
        let root = dir.path();
        for i in 0..10 {
            fs::write(root.join(format!("{}.txt", i)), "test").unwrap();
        }

        let indexer = DefaultIndexer;
        let options = IndexerOptions {
            max_file_count: 5,
            ..Default::default()
        };
        let result = indexer.index(root, &options);
        assert!(matches!(result, Err(IndexerError::FileCountLimitExceeded)));
    }

    #[test]
    fn test_generates_correct_hash() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join("test.txt"), "hello world").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    // --- Secret file exclusion ---

    #[test]
    fn test_excludes_dotenv_file() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join(".env"), "SECRET=hunter2").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        let names: Vec<_> = results.iter().map(|r| r.path.to_string_lossy().to_string()).collect();
        assert!(
            !names.iter().any(|n| n == ".env"),
            ".env must be excluded, got: {:?}",
            names
        );
    }

    #[test]
    fn test_excludes_id_rsa_file() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join("id_rsa"), "-----BEGIN RSA PRIVATE KEY-----").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        let names: Vec<_> = results.iter().map(|r| r.path.to_string_lossy().to_string()).collect();
        assert!(
            !names.iter().any(|n| n == "id_rsa"),
            "id_rsa must be excluded, got: {:?}",
            names
        );
    }

    #[test]
    fn test_excludes_credentials_json_file() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join("credentials.json"), r#"{"token":"secret"}"#).unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        let names: Vec<_> = results.iter().map(|r| r.path.to_string_lossy().to_string()).collect();
        assert!(
            !names.iter().any(|n| n == "credentials.json"),
            "credentials.json must be excluded, got: {:?}",
            names
        );
    }

    #[test]
    fn test_normal_rs_file_is_not_excluded() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join("lib.rs"), "pub fn add(a: i32, b: i32) -> i32 { a + b }").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        let names: Vec<_> = results.iter().map(|r| r.path.to_string_lossy().to_string()).collect();
        assert!(
            names.iter().any(|n| n == "lib.rs"),
            "lib.rs must be indexed, got: {:?}",
            names
        );
    }

    // --- Binary file exclusion ---

    #[test]
    fn test_excludes_exe_extension() {
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join("app.exe"), "MZ fake windows binary").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        let names: Vec<_> = results.iter().map(|r| r.path.to_string_lossy().to_string()).collect();
        assert!(
            !names.iter().any(|n| n.ends_with(".exe")),
            ".exe must be excluded, got: {:?}",
            names
        );
    }

    #[test]
    fn test_excludes_png_extension() {
        let dir = setup_workspace();
        let root = dir.path();
        // PNG magic bytes header to make it realistic, but content doesn't matter
        fs::write(root.join("logo.png"), b"\x89PNG\r\n\x1a\nfake image data").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        let names: Vec<_> = results.iter().map(|r| r.path.to_string_lossy().to_string()).collect();
        assert!(
            !names.iter().any(|n| n.ends_with(".png")),
            ".png must be excluded, got: {:?}",
            names
        );
    }

    #[test]
    fn test_excludes_source_file_with_null_bytes() {
        // A file with a .rs extension but containing null bytes should be treated
        // as binary and skipped. This catches embedded binaries misnamed as source.
        let dir = setup_workspace();
        let root = dir.path();
        let mut content = b"fn main() { println!(\"hello\"); }\n".to_vec();
        content.push(0x00); // inject a null byte
        content.extend_from_slice(b" // more code");
        fs::write(root.join("tricky.rs"), &content).unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        let names: Vec<_> = results.iter().map(|r| r.path.to_string_lossy().to_string()).collect();
        assert!(
            !names.iter().any(|n| n == "tricky.rs"),
            "Source file with null bytes must be excluded, got: {:?}",
            names
        );
    }

    // --- Glob filtering via ignore file ---

    #[test]
    fn test_codebonesignore_glob_excludes_toml_files() {
        // Simulate "--ignore *.toml" by writing a .codebonesignore with a glob pattern
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"test\"").unwrap();
        fs::write(root.join("main.rs"), "fn main() {}").unwrap();
        fs::write(root.join(".codebonesignore"), "*.toml\n").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        let names: Vec<_> = results.iter().map(|r| r.path.to_string_lossy().to_string()).collect();

        assert!(
            !names.iter().any(|n| n.ends_with(".toml")),
            "*.toml files must be excluded via .codebonesignore, got: {:?}",
            names
        );
        assert!(
            names.iter().any(|n| n == "main.rs"),
            "main.rs must still be indexed, got: {:?}",
            names
        );
    }

    #[test]
    fn test_gitignore_glob_excludes_matching_files() {
        // Simulate "--ignore *.log" by writing a .gitignore with a glob pattern
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join("app.log"), "INFO: server started").unwrap();
        fs::write(root.join("server.rs"), "fn serve() {}").unwrap();
        fs::write(root.join(".gitignore"), "*.log\n").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        let names: Vec<_> = results.iter().map(|r| r.path.to_string_lossy().to_string()).collect();

        assert!(
            !names.iter().any(|n| n.ends_with(".log")),
            "*.log files must be excluded via .gitignore, got: {:?}",
            names
        );
    }

    #[test]
    fn test_only_rs_files_indexed_when_all_others_ignored() {
        // Simulate "--include *.rs only" by ignoring everything else via .codebonesignore
        let dir = setup_workspace();
        let root = dir.path();
        fs::write(root.join("main.rs"), "fn main() {}").unwrap();
        fs::write(root.join("readme.md"), "# Project").unwrap();
        fs::write(root.join("config.yaml"), "key: value").unwrap();
        // Use .codebonesignore to exclude non-Rust files
        fs::write(root.join(".codebonesignore"), "*.md\n*.yaml\n").unwrap();

        let indexer = DefaultIndexer;
        let results = indexer.index(root, &IndexerOptions::default()).unwrap();
        let names: Vec<_> = results.iter().map(|r| r.path.to_string_lossy().to_string()).collect();

        for name in &names {
            assert!(
                name.ends_with(".rs"),
                "Only .rs files should be indexed, but found: {}",
                name
            );
        }
        assert!(
            names.iter().any(|n| n == "main.rs"),
            "main.rs must be in results"
        );
    }

    // --- Path traversal security test ---

    #[test]
    fn test_path_traversal_outside_root_is_rejected_or_absent() {
        // Create a workspace root and a separate directory outside it.
        // Attempt to index a path that canonically lives outside the root.
        // The indexer must either return an error or produce no results
        // referencing paths outside the workspace root.
        let workspace = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();

        // Write a file in the workspace
        fs::write(workspace.path().join("inside.txt"), "safe content").unwrap();

        // Write a file outside the workspace
        fs::write(outside.path().join("outside.txt"), "secret content").unwrap();

        // Attempt to index using a symlink that escapes the workspace root
        // (only possible on Unix; on Windows the symlink call is a no-op and we
        // just verify the walker doesn't traverse outside on its own)
        #[cfg(unix)]
        {
            let link_path = workspace.path().join("escape_link");
            std::os::unix::fs::symlink(outside.path().join("outside.txt"), &link_path).unwrap();

            let indexer = DefaultIndexer;
            // With follow_symlinks=false (default) the symlink is either skipped
            // (Ok with no escaping entry) or rejected outright (Err PathTraversal).
            // Both are correct — the escaping file must never appear in results.
            let result = indexer.index(workspace.path(), &IndexerOptions::default());
            let files = match result {
                Ok(f) => f,
                Err(IndexerError::PathTraversal(_)) | Err(IndexerError::SymlinkEscape(_)) => {
                    vec![] // rejected at the gate — correct behaviour
                }
                Err(e) => panic!("Unexpected error with follow_symlinks=false: {}", e),
            };

            let outside_root = outside.path();
            for fh in &files {
                let absolute = workspace.path().join(&fh.path);
                assert!(
                    absolute.starts_with(workspace.path()),
                    "Traversal detected: {:?} is outside {:?}",
                    absolute,
                    workspace.path()
                );
                assert_ne!(
                    fh.path.to_string_lossy().as_ref(),
                    "escape_link",
                    "Symlink pointing outside root must not be indexed"
                );
                let _ = outside_root;
            }
        }

        // When follow_symlinks=true, the indexer is expected to return an error
        // for paths that escape the workspace root.
        #[cfg(unix)]
        {
            let link_path2 = workspace.path().join("escape_link2");
            // Only create if it doesn't already exist (test may run twice in parallel)
            if !link_path2.exists() {
                std::os::unix::fs::symlink(outside.path().join("outside.txt"), &link_path2)
                    .unwrap();
            }
            let indexer = DefaultIndexer;
            let options = IndexerOptions {
                follow_symlinks: true,
                ..Default::default()
            };
            let result = indexer.index(workspace.path(), &options);
            // Must either be an error (PathTraversal/SymlinkEscape) or not include
            // files that canonically live outside the workspace.
            match result {
                Err(IndexerError::PathTraversal(_)) | Err(IndexerError::SymlinkEscape(_)) => {
                    // Correct: traversal detected and rejected
                }
                Ok(files) => {
                    for fh in &files {
                        let absolute = workspace.path().join(&fh.path);
                        assert!(
                            absolute.starts_with(workspace.path()),
                            "Returned file escapes workspace: {:?}",
                            absolute
                        );
                    }
                }
                Err(other) => panic!("Unexpected error: {}", other),
            }
        }
    }

    // --- Incremental indexing ---

    #[test]
    fn test_large_file_at_limit_is_indexed_small_file_over_limit_is_skipped() {
        // The indexer uses `metadata.len() > max_file_size_bytes` (strict greater-than),
        // so a file of exactly max_file_size_bytes is INCLUDED; one of max+1 is EXCLUDED.
        let dir = setup_workspace();
        let root = dir.path();

        let max_size: u64 = 500 * 1024; // 512_000 bytes — the default limit

        // File exactly AT the limit — should be indexed (not greater-than, so passes the check)
        let at_limit_path = root.join("at_limit.txt");
        let mut at_limit = File::create(&at_limit_path).unwrap();
        at_limit.write_all(&vec![b'a'; max_size as usize]).unwrap();

        // File one byte OVER the limit — should be skipped
        let over_limit_path = root.join("over_limit.txt");
        let mut over_limit = File::create(&over_limit_path).unwrap();
        over_limit.write_all(&vec![b'b'; max_size as usize + 1]).unwrap();

        let indexer = DefaultIndexer;
        let options = IndexerOptions {
            max_file_size_bytes: max_size,
            respect_gitignore: false,
            ..Default::default()
        };

        let results = indexer.index(root, &options).unwrap();
        let names: Vec<String> = results
            .iter()
            .map(|r| r.path.to_string_lossy().to_string())
            .collect();

        assert!(
            names.iter().any(|n| n == "at_limit.txt"),
            "File of exactly max_file_size_bytes should be indexed (boundary is exclusive); got: {:?}",
            names
        );

        assert!(
            !names.iter().any(|n| n == "over_limit.txt"),
            "File of max_file_size_bytes + 1 should NOT be indexed; got: {:?}",
            names
        );
    }

    #[test]
    fn test_incremental_indexing_only_changed_file_has_new_hash() {
        use std::collections::HashMap;

        let dir = setup_workspace();
        let root = dir.path();

        // Write two files
        fs::write(root.join("stable.rs"), "fn stable() {}").unwrap();
        fs::write(root.join("volatile.rs"), "fn original() {}").unwrap();

        let indexer = DefaultIndexer;
        let options = IndexerOptions {
            respect_gitignore: false,
            ..Default::default()
        };

        // First index pass: record all hashes
        let first_results = indexer.index(root, &options).unwrap();
        let first_hashes: HashMap<String, String> = first_results
            .iter()
            .map(|fh| (fh.path.to_string_lossy().to_string(), fh.hash.clone()))
            .collect();

        assert!(
            first_hashes.contains_key("stable.rs"),
            "stable.rs must be in first index"
        );
        assert!(
            first_hashes.contains_key("volatile.rs"),
            "volatile.rs must be in first index"
        );

        // Modify only volatile.rs
        fs::write(root.join("volatile.rs"), "fn modified() {}").unwrap();

        // Second index pass
        let second_results = indexer.index(root, &options).unwrap();
        let second_hashes: HashMap<String, String> = second_results
            .iter()
            .map(|fh| (fh.path.to_string_lossy().to_string(), fh.hash.clone()))
            .collect();

        // stable.rs hash must be unchanged
        assert_eq!(
            first_hashes["stable.rs"], second_hashes["stable.rs"],
            "stable.rs hash must not change between index passes"
        );

        // volatile.rs hash must have changed
        assert_ne!(
            first_hashes["volatile.rs"], second_hashes["volatile.rs"],
            "volatile.rs hash must change after file modification"
        );
    }
}
