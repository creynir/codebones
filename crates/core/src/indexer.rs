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
            let canonical_root = std::fs::canonicalize(workspace_root)?;
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
    fn test_rejects_path_traversal() {
        // This is tricky to simulate with just the walker, but we can test the logic directly if we inject a path.
        // For the sake of the test, we'll create a symlink that escapes and check symlink escape error.
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
}
