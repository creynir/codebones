use crate::cache::{CacheStore, SqliteCache, Symbol as CacheSymbol};
use crate::indexer::{DefaultIndexer, Indexer, IndexerOptions};
use crate::parser::{get_spec_for_extension, parse_file};
use crate::plugin::{OutputFormat, Packer};
use anyhow::Result;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;

/// A file entry in the import graph with its inbound import count.
#[derive(Debug, Clone)]
pub struct GraphFile {
    pub path: String,
    pub import_count: usize,
}

/// A directed import edge: `from` imports `to`.
#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
}

/// Full import graph result.
#[derive(Debug, Clone)]
pub struct GraphResult {
    pub files: Vec<GraphFile>,
    pub edges: Vec<GraphEdge>,
}

/// Blast radius result for a specific file.
#[derive(Debug, Clone)]
pub struct BlastRadiusResult {
    pub affected_files: Vec<String>,
}

const CODEBONES_SECTION: &str = r#"
## Codebones

This project is indexed by [codebones](https://github.com/anthropics/codebones). Use `codebones search`, `codebones outline`, and `codebones get` to explore the codebase structure before reading files directly.
"#;

/// Performs first-run setup for a project directory:
/// - Creates `.codebones/` directory if it doesn't exist
/// - Deletes legacy `codebones.db` at root if it exists
/// - If `.git/` exists: ensures `.codebones/` is in `.gitignore`
/// - If `CLAUDE.md` exists: appends codebones section (if not already present)
/// - If `AGENTS.md` exists: appends codebones section (if not already present)
fn first_run_setup(dir: &Path) -> Result<()> {
    // Create .codebones/ directory if it doesn't exist
    let dot_codebones = dir.join(".codebones");
    if !dot_codebones.exists() {
        fs::create_dir_all(&dot_codebones)?;
    }

    // Delete legacy codebones.db at root if it exists
    let legacy_db = dir.join("codebones.db");
    if legacy_db.exists() {
        fs::remove_file(&legacy_db)?;
    }

    // If .git/ exists, ensure .codebones/ is in .gitignore
    if dir.join(".git").exists() {
        let gitignore_path = dir.join(".gitignore");
        let existing = if gitignore_path.exists() {
            fs::read_to_string(&gitignore_path)?
        } else {
            String::new()
        };
        if !existing.lines().any(|line| line.trim() == ".codebones/") {
            let new_content = if existing.is_empty() {
                ".codebones/\n".to_string()
            } else if existing.ends_with('\n') {
                format!("{}.codebones/\n", existing)
            } else {
                format!("{}\n.codebones/\n", existing)
            };
            fs::write(&gitignore_path, new_content)?;
        }
    }

    // If CLAUDE.md exists, append codebones section if not already present
    let claude_md = dir.join("CLAUDE.md");
    if claude_md.exists() {
        let contents = fs::read_to_string(&claude_md)?;
        if !contents.contains("codebones") {
            let mut file = fs::OpenOptions::new().append(true).open(&claude_md)?;
            use std::io::Write;
            file.write_all(CODEBONES_SECTION.as_bytes())?;
        }
    }

    // If AGENTS.md exists, append codebones section if not already present
    let agents_md = dir.join("AGENTS.md");
    if agents_md.exists() {
        let contents = fs::read_to_string(&agents_md)?;
        if !contents.contains("codebones") {
            let mut file = fs::OpenOptions::new().append(true).open(&agents_md)?;
            use std::io::Write;
            file.write_all(CODEBONES_SECTION.as_bytes())?;
        }
    }

    Ok(())
}

/// Walks `dir`, hashes every eligible file, and upserts changed files and their symbols into the local SQLite cache.
///
/// Must be called before `get`, `outline`, or `search`; those functions read from the cache `index` populates.
pub fn index(dir: &Path) -> Result<()> {
    // Perform first-run setup (create .codebones/, clean legacy db, update gitignore/docs)
    first_run_setup(dir)?;

    let db_path = dir.join(".codebones").join("codebones.db");
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;

    let indexer = DefaultIndexer;
    let hashes = indexer.index(dir, &IndexerOptions::default())?;
    let current_paths: HashSet<String> = hashes
        .iter()
        .map(|fh| fh.path.to_string_lossy().to_string())
        .collect();

    for cached_path in cache.list_file_paths()? {
        if current_paths.contains(&cached_path) {
            continue;
        }

        let full_path = dir.join(&cached_path);
        match fs::symlink_metadata(&full_path) {
            Ok(_) => {
                // The file still exists on disk but was skipped by the indexer
                // (for example due to a transient read/permission failure). Keep
                // the last known cached content instead of treating it as deleted.
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                cache.delete_file(&cached_path)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                // Preserve the cached entry when the file still exists but is no
                // longer readable.
            }
            Err(error) => return Err(error.into()),
        }
    }

    for fh in hashes {
        let path_str = fh.path.to_string_lossy().to_string();
        let existing_hash = cache.get_file_hash(&path_str)?;

        if existing_hash.as_deref() != Some(fh.hash.as_str()) {
            let full_path = dir.join(&fh.path);
            let content = match fs::read(&full_path) {
                Ok(content) => content,
                Err(e) => {
                    eprintln!("Warning: could not read {}: {}", full_path.display(), e);
                    continue;
                }
            };

            // Delete old file to trigger cascade delete of symbols and imports.
            // Ignoring the error here is intentional: if the file does not yet exist in
            // the cache this is a no-op, which is the desired idempotent behaviour.
            let _ = cache.delete_file(&path_str);

            let file_id = cache.upsert_file(&path_str, &fh.hash, &content)?;

            let ext = fh.path.extension().unwrap_or_default().to_string_lossy();
            if let Some(spec) = get_spec_for_extension(&ext) {
                if let Ok(source) = String::from_utf8(content) {
                    let doc = parse_file(&source, &spec);
                    for sym in doc.symbols {
                        let kind_str = match sym.kind {
                            crate::parser::SymbolKind::Function => "Function",
                            crate::parser::SymbolKind::Method => "Method",
                            crate::parser::SymbolKind::Class => "Class",
                            crate::parser::SymbolKind::Struct => "Struct",
                            crate::parser::SymbolKind::Impl => "Impl",
                            crate::parser::SymbolKind::Interface => "Interface",
                        }
                        .to_string();

                        let cache_sym = CacheSymbol {
                            id: format!("{}::{}", path_str, sym.qualified_name),
                            file_id,
                            name: sym.qualified_name.clone(),
                            kind: kind_str,
                            byte_offset: sym.full_range.start,
                            byte_length: sym.full_range.end - sym.full_range.start,
                        };
                        cache.insert_symbol(&cache_sym)?;
                    }

                    // Store imports for this file, resolving against the full set
                    // of current on-disk paths so ordering doesn't affect resolution.
                    let source_dir = Path::new(&path_str)
                        .parent()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    for raw_import in doc.imports {
                        let target_path =
                            resolve_import(&raw_import, &source_dir, &current_paths);
                        cache.insert_import(file_id, &target_path, &raw_import)?;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Attempts to resolve a raw import string to a known file path in the cache.
///
/// Tries stripping `./` or `../` prefixes and appending common extensions against the
/// set of known file paths. Falls back to the raw import string if no match is found.
fn resolve_import(raw: &str, source_dir: &str, known_paths: &HashSet<String>) -> String {
    // Common extensions to try when no extension is present
    const EXTS: &[&str] = &[
        ".ts", ".tsx", ".js", ".jsx", ".py", ".rs", ".go", ".java", ".c", ".cpp", ".cs", ".rb",
        ".php", ".swift", ".h", ".hpp",
    ];

    // Strategy 1: Python dotted absolute import (e.g. `src.core.event` → `src/core/event`)
    // Only applies when the import contains dots but no path separators.
    if raw.contains('.') && !raw.contains('/') && !raw.starts_with('.') {
        let slash_path = raw.replace('.', "/");
        let with_py = format!("{}.py", slash_path);
        if known_paths.contains(&with_py) {
            return with_py;
        }
        let with_init = format!("{}/__init__.py", slash_path);
        if known_paths.contains(&with_init) {
            return with_init;
        }
    }

    // Strategy 2: Python single-dot relative import (e.g. `.event` in `src/core/tracer.py`)
    // Only applies to imports that start with exactly one dot (not `..`).
    if raw.starts_with('.') && !raw.starts_with("..") {
        let base = &raw[1..]; // strip the leading dot
        if !base.is_empty() {
            let relative_path = if source_dir.is_empty() {
                base.to_string()
            } else {
                format!("{}/{}", source_dir, base)
            };
            let with_py = format!("{}.py", relative_path);
            if known_paths.contains(&with_py) {
                return with_py;
            }
            let with_init = format!("{}/__init__.py", relative_path);
            if known_paths.contains(&with_init) {
                return with_init;
            }
        }
    }

    // Candidates to try:
    // 1. The raw import as-is
    // 2. Joined with source_dir (for relative imports starting with ./ or ../)
    // 3. Same candidates with common extensions appended

    let candidates: Vec<String> = {
        let mut v = Vec::new();

        // Relative import
        let joined = if source_dir.is_empty() {
            raw.to_string()
        } else {
            format!("{}/{}", source_dir, raw)
        };

        // Strip leading ./ for relative imports
        let stripped = if let Some(base) = raw.strip_prefix("./") {
            if source_dir.is_empty() {
                base.to_string()
            } else {
                format!("{}/{}", source_dir, base)
            }
        } else {
            joined.clone()
        };

        v.push(raw.to_string());
        v.push(joined.clone());
        v.push(stripped.clone());

        // With extensions
        for ext in EXTS {
            v.push(format!("{}{}", raw, ext));
            v.push(format!("{}{}", joined, ext));
            v.push(format!("{}{}", stripped, ext));
        }

        v
    };

    for candidate in &candidates {
        if known_paths.contains(candidate) {
            return candidate.clone();
        }
    }

    // No match — store raw import as the target path
    raw.to_string()
}

/// Returns the path to the database file, creating the `.codebones/` directory if needed.
fn db_path(dir: &Path) -> Result<std::path::PathBuf> {
    let dot_codebones = dir.join(".codebones");
    if !dot_codebones.exists() {
        fs::create_dir_all(&dot_codebones)?;
    }
    Ok(dot_codebones.join("codebones.db"))
}

/// Retrieves the raw source content of a symbol (using `::` notation) or a file path from the cache.
///
/// Returns an error if the symbol or path is not found; run `index` first to populate the cache.
///
/// # Security
///
/// Path lookup is performed against the SQLite cache only — no filesystem reads occur.
/// `.codebones/codebones.db` is a trust boundary: callers must ensure the database file has
/// appropriate filesystem permissions and has not been tampered with.
pub fn get(dir: &Path, symbol_or_path: &str) -> Result<String> {
    let db_path = db_path(dir)?;
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;

    // It's a symbol if it contains ::
    if symbol_or_path.contains("::") {
        if let Some(content) = cache.get_symbol_content(symbol_or_path)? {
            return Ok(String::from_utf8_lossy(&content).to_string());
        }
    } else {
        // Assume file path
        if let Some(content) = cache.get_file_content(symbol_or_path)? {
            return Ok(String::from_utf8_lossy(&content).to_string());
        }
    }

    anyhow::bail!("Symbol or path not found: {}", symbol_or_path)
}

/// Returns a skeleton view of a source file by eliding function and class bodies with `...`.
///
/// Falls back to the full raw source if the file's language is not supported by the parser.
///
/// # Security
///
/// Path lookup is performed against the SQLite cache only — no filesystem reads occur.
/// `codebones.db` is a trust boundary: callers must ensure the database file has
/// appropriate filesystem permissions and has not been tampered with.
pub fn outline(dir: &Path, path: &str) -> Result<String> {
    let db_path = db_path(dir)?;
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;

    if let Some(content) = cache.get_file_content(path)? {
        let source = String::from_utf8_lossy(&content).to_string();

        let ext = Path::new(path)
            .extension()
            .unwrap_or_default()
            .to_string_lossy();
        if let Some(spec) = get_spec_for_extension(&ext) {
            let doc = parse_file(&source, &spec);

            // elide document
            let mut result = String::new();
            let mut last_end = 0;

            let mut indices: Vec<usize> = (0..doc.symbols.len()).collect();
            indices.sort_by_key(|&i| doc.symbols[i].full_range.start);

            for i in &indices {
                let sym = &doc.symbols[*i];
                if let Some(body_range) = &sym.body_range {
                    if body_range.start >= last_end {
                        result.push_str(&source[last_end..body_range.start]);
                        result.push_str("...");
                        last_end = body_range.end;
                    }
                }
            }
            result.push_str(&source[last_end..]);
            return Ok(result);
        }

        return Ok(source);
    }

    anyhow::bail!("Path not found: {}", path)
}

/// Searches the cache for symbol IDs whose name contains `query` (substring match).
///
/// Returns a list of fully-qualified symbol ID strings; an empty vec means no matches.
pub fn search(dir: &Path, query: &str) -> Result<Vec<String>> {
    let db_path = db_path(dir)?;
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;

    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let like_query = format!("%{}%", escaped);
    cache.search_symbol_ids(&like_query).map_err(Into::into)
}

/// Returns all raw import strings recorded for the given file path.
///
/// Returns an empty vec if the file has no imports. Run `index` first to populate the cache.
pub fn get_imports(dir: &Path, file_path: &str) -> Result<Vec<String>> {
    let db_path = db_path(dir)?;
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;
    let pairs = cache.get_imports(file_path)?;
    Ok(pairs.into_iter().map(|(_, raw)| raw).collect())
}

/// Returns all file paths that import the given target path.
///
/// Returns an empty vec if nothing imports the target. Run `index` first to populate the cache.
pub fn get_importers(dir: &Path, file_path: &str) -> Result<Vec<String>> {
    let db_path = db_path(dir)?;
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;
    // We need to find files that have target_path matching this file_path,
    // but stored target_path may be the resolved path (e.g., "shared.ts")
    // while the query uses the file's actual path.
    cache.get_importers(file_path).map_err(Into::into)
}

/// Returns the full import dependency graph for the indexed directory.
///
/// Files are sorted by inbound import count (descending). Edges represent `from → to` import
/// relationships exactly as recorded in the cache.
pub fn graph(dir: &Path) -> Result<GraphResult> {
    let db_path = db_path(dir)?;
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;

    let all_paths = cache.list_file_paths()?;
    let all_edges_raw = cache.list_all_imports()?;

    // Count how many times each file appears as an import target.
    let mut import_count: HashMap<String, usize> = HashMap::new();
    for path in &all_paths {
        import_count.insert(path.clone(), 0);
    }
    for (_, target) in &all_edges_raw {
        let entry = import_count.entry(target.clone()).or_insert(0);
        *entry += 1;
    }

    let mut files: Vec<GraphFile> = all_paths
        .into_iter()
        .map(|path| {
            let count = import_count.get(&path).copied().unwrap_or(0);
            GraphFile {
                path,
                import_count: count,
            }
        })
        .collect();

    // Sort by import_count descending, then by path ascending for stable ordering.
    files.sort_by(|a, b| b.import_count.cmp(&a.import_count).then(a.path.cmp(&b.path)));

    let edges: Vec<GraphEdge> = all_edges_raw
        .into_iter()
        .map(|(from, to)| GraphEdge { from, to })
        .collect();

    Ok(GraphResult { files, edges })
}

/// Returns the blast radius for `file_path`: all files that (transitively) import it,
/// discovered via BFS up to `max_depth` hops on the reverse import graph.
pub fn graph_file(dir: &Path, file_path: &str, max_depth: usize) -> Result<BlastRadiusResult> {
    let db_path = db_path(dir)?;
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;

    // Build reverse adjacency map: target → set of source files that import it.
    let all_edges = cache.list_all_imports()?;
    let mut reverse: HashMap<String, Vec<String>> = HashMap::new();
    for (source, target) in all_edges {
        reverse.entry(target).or_default().push(source);
    }

    // BFS from file_path following reverse edges.
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(file_path.to_string());

    let mut queue: VecDeque<String> = VecDeque::new();
    queue.push_back(file_path.to_string());

    let mut affected: Vec<String> = Vec::new();
    let mut depth = 0usize;

    while !queue.is_empty() && depth < max_depth {
        let level_size = queue.len();
        for _ in 0..level_size {
            if let Some(current) = queue.pop_front() {
                if let Some(importers) = reverse.get(&current) {
                    for importer in importers {
                        if visited.insert(importer.clone()) {
                            affected.push(importer.clone());
                            queue.push_back(importer.clone());
                        }
                    }
                }
            }
        }
        depth += 1;
    }

    Ok(BlastRadiusResult {
        affected_files: affected,
    })
}

/// Registers the codebones MCP server entry in an AI tool's config file.
///
/// If the file does not exist, it is created. If `mcpServers.codebones` is
/// already present the function is a no-op (idempotent).
fn register_mcp_server(settings_path: &Path) -> Result<()> {
    let mut root: serde_json::Value = if settings_path.exists() {
        let text = fs::read_to_string(settings_path)?;
        serde_json::from_str(&text).unwrap_or(serde_json::Value::Object(Default::default()))
    } else {
        serde_json::Value::Object(Default::default())
    };

    // Ensure root is an object.
    if !root.is_object() {
        root = serde_json::Value::Object(Default::default());
    }

    // Ensure mcpServers object exists.
    if root.get("mcpServers").is_none_or(|v| !v.is_object()) {
        root["mcpServers"] = serde_json::Value::Object(Default::default());
    }

    // Only add the codebones entry if it isn't already there.
    if root["mcpServers"].get("codebones").is_none() {
        root["mcpServers"]["codebones"] = serde_json::json!({
            "command": "codebones-mcp",
            "args": [],
            "type": "stdio"
        });
    }

    let pretty = serde_json::to_string_pretty(&root)?;
    fs::write(settings_path, pretty)?;
    Ok(())
}

/// Registers the codebones MCP server with AI tools installed on the user's machine.
///
/// Checks for Claude Code (`~/.claude/`) and Cursor (`~/.cursor/`) and adds the
/// `codebones-mcp` entry to their respective config files if they are found.
/// Returns a list of human-readable status messages for the caller to display.
pub fn init(home_dir: &Path) -> Result<Vec<String>> {
    let mut messages = Vec::new();

    // Check for Claude Code
    let claude_dir = home_dir.join(".claude");
    if claude_dir.exists() {
        register_mcp_server(&claude_dir.join("settings.json"))?;
        messages.push("Claude Code: registered codebones-mcp".to_string());
    }

    // Check for Cursor
    let cursor_dir = home_dir.join(".cursor");
    if cursor_dir.exists() {
        register_mcp_server(&cursor_dir.join("mcp.json"))?;
        messages.push("Cursor: registered codebones-mcp".to_string());
    }

    if messages.is_empty() {
        messages.push("No supported AI tools found".to_string());
    }

    Ok(messages)
}

/// Options that control how `pack` filters and transforms files before bundling them.
///
/// Set boolean flags to strip comments, empty lines, or long base64 blobs; use `include`/`ignore` glob lists to narrow the file set.
pub struct PackOptions {
    pub no_file_summary: bool,
    pub no_files: bool,
    pub remove_comments: bool,
    pub remove_empty_lines: bool,
    pub truncate_base64: bool,
    pub include: Option<Vec<String>>,
    pub ignore: Option<Vec<String>>,
}

/// Bundles all indexed files in `dir` into a single AI-friendly document in Markdown or XML format.
///
/// Automatically re-indexes `dir` before packing; pass `max_tokens` to enable token-budget degradation that drops file bodies when the limit is exceeded.
pub fn pack(
    dir: &Path,
    format_str: &str,
    max_tokens: Option<usize>,
    options: PackOptions,
) -> Result<String> {
    // If the provided dir is actually a file, use its parent directory for the database
    let base_dir = if dir.is_file() {
        let parent = dir.parent().unwrap_or(Path::new("."));
        if parent.as_os_str().is_empty() {
            Path::new(".")
        } else {
            parent
        }
    } else {
        dir
    };

    // Ensure the cache is up to date before packing
    index(base_dir)?;

    let db_path = db_path(base_dir)?;
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;

    let format = OutputFormat::parse(format_str)?;

    // Get all files
    let mut paths = Vec::new();
    {
        let file_paths = cache.list_file_paths()?;

        let mut include_builder = globset::GlobSetBuilder::new();
        let mut has_includes = false;
        if let Some(includes) = &options.include {
            for pattern in includes {
                if let Ok(glob) = globset::Glob::new(pattern) {
                    include_builder.add(glob);
                    has_includes = true;
                }
            }
        }
        let include_set = include_builder.build().unwrap_or(globset::GlobSet::empty());

        let mut ignore_builder = globset::GlobSetBuilder::new();
        let mut has_ignores = false;
        if let Some(ignores) = &options.ignore {
            for pattern in ignores {
                if let Ok(glob) = globset::Glob::new(pattern) {
                    ignore_builder.add(glob);
                    has_ignores = true;
                }
            }
        }
        let ignore_set = ignore_builder.build().unwrap_or(globset::GlobSet::empty());

        // Security: canonicalize the base directory once before iterating files.
        // If this fails (e.g. the directory does not exist), propagate the error
        // rather than silently allowing all paths through the traversal guard.
        let base_canonical = base_dir.canonicalize().map_err(|e| {
            anyhow::anyhow!(
                "Cannot resolve base directory '{}': {}",
                base_dir.display(),
                e
            )
        })?;

        for path_str in file_paths {
            if has_includes && !include_set.is_match(&path_str) {
                continue;
            }
            if has_ignores && ignore_set.is_match(&path_str) {
                continue;
            }

            let file_path = base_dir.join(&path_str);

            // Security: verify the DB-stored path doesn't escape the base directory.
            // If canonicalize fails (e.g. broken symlink), skip the file to avoid
            // bypassing the traversal guard.
            let canonical = match file_path.canonicalize() {
                Ok(c) => c,
                Err(_) => continue,
            };
            if !canonical.starts_with(&base_canonical) {
                eprintln!("Warning: skipping path that escapes base dir: {}", path_str);
                continue;
            }

            // If the user specified a file rather than a directory, only include that specific file
            if dir.is_file() {
                let dir_canon = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
                let file_canon = file_path
                    .canonicalize()
                    .unwrap_or_else(|_| file_path.clone());
                if file_canon != dir_canon {
                    continue;
                }
            }

            if file_path.exists() {
                paths.push(file_path);
            }
        }
    }

    let packer = Packer::with_workspace_root(
        cache,
        crate::parser::Parser {},
        base_dir.to_path_buf(),
        format,
        max_tokens,
        options.no_file_summary,
        options.no_files,
        options.remove_comments,
        options.remove_empty_lines,
        options.truncate_base64,
    );

    packer.pack(&paths)
}
