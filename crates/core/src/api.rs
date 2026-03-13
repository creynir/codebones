use std::path::Path;
use anyhow::Result;
use std::fs;
use crate::cache::{SqliteCache, CacheStore, Symbol as CacheSymbol};
use crate::indexer::{DefaultIndexer, Indexer, IndexerOptions};
use crate::parser::{parse_file, get_spec_for_extension};
use crate::plugin::{Packer, OutputFormat};

pub fn index(dir: &Path) -> Result<()> {
    let db_path = dir.join("codebones.db");
    let cache = SqliteCache::new(db_path.to_str().unwrap())?;
    cache.init()?;

    let indexer = DefaultIndexer;
    let hashes = indexer.index(dir, &IndexerOptions::default())?;

    for fh in hashes {
        let path_str = fh.path.to_string_lossy().to_string();
        let existing_hash = cache.get_file_hash(&path_str)?;
        
        if existing_hash.as_deref() != Some(fh.hash.as_str()) {
            let full_path = dir.join(&fh.path);
            let content = fs::read(&full_path).unwrap_or_default();
            
            // Delete old file to trigger cascade delete of symbols
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
                        }.to_string();
                        
                        let cache_sym = CacheSymbol {
                            id: format!("{}::{}", path_str, sym.qualified_name),
                            file_id,
                            name: sym.qualified_name.clone(),
                            kind: kind_str,
                            byte_offset: sym.full_range.start,
                            byte_length: sym.full_range.end - sym.full_range.start,
                        };
                        let _ = cache.insert_symbol(&cache_sym);
                    }
                }
            }
        }
    }
    
    Ok(())
}

pub fn get(dir: &Path, symbol_or_path: &str) -> Result<String> {
    let db_path = dir.join("codebones.db");
    let cache = SqliteCache::new(db_path.to_str().unwrap())?;
    cache.init()?;

    // It's a symbol if it contains ::
    if symbol_or_path.contains("::") {
        if let Some(content) = cache.get_symbol_content(symbol_or_path)? {
            return Ok(String::from_utf8_lossy(&content).to_string());
        }
    } else {
        // Assume file path
        let mut stmt = cache.conn.prepare("SELECT content FROM files WHERE path = ?1")?;
        let mut rows = stmt.query([symbol_or_path])?;
        if let Some(row) = rows.next()? {
            let content: Vec<u8> = row.get(0)?;
            return Ok(String::from_utf8_lossy(&content).to_string());
        }
    }
    
    anyhow::bail!("Symbol or path not found: {}", symbol_or_path)
}

pub fn outline(dir: &Path, path: &str) -> Result<String> {
    let db_path = dir.join("codebones.db");
    let cache = SqliteCache::new(db_path.to_str().unwrap())?;
    cache.init()?;

    let mut stmt = cache.conn.prepare("SELECT content FROM files WHERE path = ?1")?;
    let mut rows = stmt.query([path])?;
    if let Some(row) = rows.next()? {
        let content: Vec<u8> = row.get(0)?;
        let source = String::from_utf8_lossy(&content).to_string();
        
        let ext = Path::new(path).extension().unwrap_or_default().to_string_lossy();
        if let Some(spec) = get_spec_for_extension(&ext) {
            let doc = parse_file(&source, &spec);
            
            // elide document
            let mut result = String::new();
            let mut last_end = 0;
            
            let mut sorted_symbols = doc.symbols.clone();
            sorted_symbols.sort_by_key(|s| s.full_range.start);

            for sym in sorted_symbols {
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

pub fn search(dir: &Path, query: &str) -> Result<Vec<String>> {
    let db_path = dir.join("codebones.db");
    let cache = SqliteCache::new(db_path.to_str().unwrap())?;
    cache.init()?;
    
    // Naive search over symbols name using LIKE
    let mut stmt = cache.conn.prepare("SELECT id FROM symbols WHERE name LIKE ?1")?;
    let like_query = format!("%{}%", query);
    let rows = stmt.query_map([like_query], |row| row.get::<_, String>(0))?;
    
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    
    Ok(results)
}

pub fn pack(dir: &Path, format_str: &str) -> Result<String> {
    let db_path = dir.join("codebones.db");
    let cache = SqliteCache::new(db_path.to_str().unwrap())?;
    cache.init()?;

    let format = match format_str.to_lowercase().as_str() {
        "xml" => OutputFormat::Xml,
        _ => OutputFormat::Markdown,
    };
    
    let packer = Packer::new(crate::cache::Cache {}, crate::parser::Parser {}, format, None);
    
    // Get all files
    let mut stmt = cache.conn.prepare("SELECT path FROM files")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut paths = Vec::new();
    for row in rows {
        paths.push(dir.join(row?));
    }
    
    packer.pack(&paths)
}
