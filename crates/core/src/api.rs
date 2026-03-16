use crate::cache::{CacheStore, SqliteCache, Symbol as CacheSymbol};
use crate::indexer::{DefaultIndexer, Indexer, IndexerOptions};
use crate::parser::{get_spec_for_extension, parse_file};
use crate::plugin::{OutputFormat, Packer};
use anyhow::Result;
use std::fs;
use std::path::Path;

/// Walks `dir`, hashes every eligible file, and upserts changed files and their symbols into the local SQLite cache.
///
/// Must be called before `get`, `outline`, or `search`; those functions read from the cache `index` populates.
pub fn index(dir: &Path) -> Result<()> {
    let db_path = dir.join("codebones.db");
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;

    let indexer = DefaultIndexer;
    let hashes = indexer.index(dir, &IndexerOptions::default())?;

    for fh in hashes {
        let path_str = fh.path.to_string_lossy().to_string();
        let existing_hash = cache.get_file_hash(&path_str)?;

        if existing_hash.as_deref() != Some(fh.hash.as_str()) {
            let full_path = dir.join(&fh.path);
            let content = fs::read(&full_path).unwrap_or_else(|e| {
                eprintln!("Warning: could not read {}: {}", full_path.display(), e);
                vec![]
            });

            // Delete old file to trigger cascade delete of symbols.
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
                }
            }
        }
    }

    Ok(())
}

/// Retrieves the raw source content of a symbol (using `::` notation) or a file path from the cache.
///
/// Returns an error if the symbol or path is not found; run `index` first to populate the cache.
pub fn get(dir: &Path, symbol_or_path: &str) -> Result<String> {
    let db_path = dir.join("codebones.db");
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
pub fn outline(dir: &Path, path: &str) -> Result<String> {
    let db_path = dir.join("codebones.db");
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
    let db_path = dir.join("codebones.db");
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;

    let like_query = format!("%{}%", query);
    cache.search_symbol_ids(&like_query).map_err(Into::into)
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

    let db_path = base_dir.join("codebones.db");
    let db_path_str = db_path
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Database path contains invalid UTF-8: {:?}", db_path))?;
    let cache = SqliteCache::new(db_path_str)?;
    cache.init()?;

    let format = match format_str.to_lowercase().as_str() {
        "xml" => OutputFormat::Xml,
        _ => OutputFormat::Markdown,
    };

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

        for path_str in file_paths {

            if has_includes && !include_set.is_match(&path_str) {
                continue;
            }
            if has_ignores && ignore_set.is_match(&path_str) {
                continue;
            }

            let file_path = base_dir.join(&path_str);

            // Security: verify the DB-stored path doesn't escape the base directory
            if let Ok(canonical) = file_path.canonicalize() {
                if let Ok(base_canonical) = base_dir.canonicalize() {
                    if !canonical.starts_with(&base_canonical) {
                        eprintln!("Warning: skipping path that escapes base dir: {}", path_str);
                        continue;
                    }
                }
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

    let packer = Packer::new(
        cache,
        crate::parser::Parser {},
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
