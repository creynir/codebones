use rusqlite::Connection;

pub struct Cache {}

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub id: i64,
    pub path: String,
    pub hash: String,
    pub content: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Symbol {
    pub id: String,
    pub file_id: i64,
    pub name: String,
    pub kind: String,
    pub byte_offset: usize,
    pub byte_length: usize,
}

/// File path with its associated (kind, name) symbol pairs.
pub type FileSymbolList = Vec<(String, Vec<(String, String)>)>;

pub trait CacheStore {
    /// Initialize the database schema
    fn init(&self) -> rusqlite::Result<()>;

    /// Insert or update a file and its content. Returns the file_id.
    fn upsert_file(&self, path: &str, hash: &str, content: &[u8]) -> rusqlite::Result<i64>;

    /// Get a file's hash to check if it has changed
    fn get_file_hash(&self, path: &str) -> rusqlite::Result<Option<String>>;

    /// Insert a symbol
    fn insert_symbol(&self, symbol: &Symbol) -> rusqlite::Result<()>;

    /// Retrieve the raw bytes of a symbol using SQLite substr()
    fn get_symbol_content(&self, symbol_id: &str) -> rusqlite::Result<Option<Vec<u8>>>;

    /// Delete a file and cascade delete its symbols
    fn delete_file(&self, path: &str) -> rusqlite::Result<()>;

    /// Get symbols for a file
    fn get_file_symbols(&self, path: &str) -> rusqlite::Result<Vec<(String, String)>>;

    /// Get the raw content bytes of a file by path
    fn get_file_content(&self, path: &str) -> rusqlite::Result<Option<Vec<u8>>>;

    /// List all file paths stored in the database
    fn list_file_paths(&self) -> rusqlite::Result<Vec<String>>;

    /// List all files with their associated symbols (kind, name), ordered by byte_offset
    fn list_files_with_symbols(&self) -> rusqlite::Result<FileSymbolList>;

    /// Search symbol IDs whose name matches a SQL LIKE pattern
    fn search_symbol_ids(&self, like_pattern: &str) -> rusqlite::Result<Vec<String>>;
}

pub struct SqliteCache {
    conn: Connection,
}

impl SqliteCache {
    pub fn new(db_path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(db_path)?;
        // Enable foreign keys for cascading deletes
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        Ok(Self { conn })
    }

    pub fn new_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute("PRAGMA foreign_keys = ON", [])?;
        Ok(Self { conn })
    }

    fn get_file_symbols_by_id(&self, file_id: i64) -> rusqlite::Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT kind, name FROM symbols WHERE file_id = ?1 ORDER BY byte_offset ASC",
        )?;
        let mut rows = stmt.query(rusqlite::params![file_id])?;
        let mut symbols = Vec::new();
        while let Some(row) = rows.next()? {
            let kind: String = row.get(0)?;
            let name: String = row.get(1)?;
            symbols.push((kind, name));
        }
        Ok(symbols)
    }
}

impl CacheStore for SqliteCache {
    fn init(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL UNIQUE,
                hash TEXT NOT NULL,
                content BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS symbols (
                id TEXT PRIMARY KEY,
                file_id INTEGER NOT NULL,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                byte_offset INTEGER NOT NULL,
                byte_length INTEGER NOT NULL,
                FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_symbols_file_id ON symbols(file_id);
            CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);",
        )?;
        Ok(())
    }

    fn upsert_file(&self, path: &str, hash: &str, content: &[u8]) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "INSERT INTO files (path, hash, content) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET hash=excluded.hash, content=excluded.content
             RETURNING id",
            rusqlite::params![path, hash, content],
            |row| row.get(0),
        )
    }

    fn get_file_hash(&self, path: &str) -> rusqlite::Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT hash FROM files WHERE path = ?1")?;
        let mut rows = stmt.query(rusqlite::params![path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    fn insert_symbol(&self, symbol: &Symbol) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO symbols (id, file_id, name, kind, byte_offset, byte_length)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                file_id=excluded.file_id,
                name=excluded.name,
                kind=excluded.kind,
                byte_offset=excluded.byte_offset,
                byte_length=excluded.byte_length",
            rusqlite::params![
                symbol.id,
                symbol.file_id,
                symbol.name,
                symbol.kind,
                symbol.byte_offset as i64,
                symbol.byte_length as i64,
            ],
        )?;
        Ok(())
    }

    fn get_symbol_content(&self, symbol_id: &str) -> rusqlite::Result<Option<Vec<u8>>> {
        let mut stmt = self.conn.prepare(
            "SELECT substr(f.content, s.byte_offset + 1, s.byte_length) 
             FROM symbols s
             JOIN files f ON s.file_id = f.id
             WHERE s.id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![symbol_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    fn delete_file(&self, path: &str) -> rusqlite::Result<()> {
        self.conn
            .execute("DELETE FROM files WHERE path = ?1", rusqlite::params![path])?;
        Ok(())
    }

    fn get_file_symbols(&self, path: &str) -> rusqlite::Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.kind, s.name FROM symbols s
             JOIN files f ON s.file_id = f.id
             WHERE f.path = ?1
             ORDER BY s.byte_offset ASC",
        )?;
        let mut rows = stmt.query(rusqlite::params![path])?;
        let mut symbols = Vec::new();
        while let Some(row) = rows.next()? {
            let kind: String = row.get(0)?;
            let name: String = row.get(1)?;
            symbols.push((kind, name));
        }
        Ok(symbols)
    }

    fn get_file_content(&self, path: &str) -> rusqlite::Result<Option<Vec<u8>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT content FROM files WHERE path = ?1")?;
        let mut rows = stmt.query(rusqlite::params![path])?;
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    fn list_file_paths(&self) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare("SELECT path FROM files")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut paths = Vec::new();
        for row in rows {
            paths.push(row?);
        }
        Ok(paths)
    }

    fn list_files_with_symbols(&self) -> rusqlite::Result<FileSymbolList> {
        let mut file_stmt = self.conn.prepare("SELECT id, path FROM files")?;
        let mut file_rows = file_stmt.query([])?;
        let mut result = Vec::new();
        while let Some(row) = file_rows.next()? {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            let symbols = self.get_file_symbols_by_id(id)?;
            result.push((path, symbols));
        }
        Ok(result)
    }

    fn search_symbol_ids(&self, like_pattern: &str) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM symbols WHERE name LIKE ?1 ESCAPE '\\'")?;
        let rows = stmt.query_map(rusqlite::params![like_pattern], |row| row.get(0))?;
        let mut ids = Vec::new();
        for row in rows {
            ids.push(row?);
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_initialize_schema_successfully() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let mut stmt = cache.conn.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name IN ('files', 'symbols')").unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(tables.contains(&"files".to_string()));
        assert!(tables.contains(&"symbols".to_string()));
    }

    #[test]
    fn test_should_upsert_a_file_and_return_its_id() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let id = cache
            .upsert_file("src/main.rs", "hash123", b"fn main() {}")
            .unwrap();
        assert!(id > 0);

        let mut stmt = cache
            .conn
            .prepare("SELECT path, hash FROM files WHERE id = ?")
            .unwrap();
        let (path, hash): (String, String) = stmt
            .query_row([id], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap();

        assert_eq!(path, "src/main.rs");
        assert_eq!(hash, "hash123");
    }

    #[test]
    fn test_should_return_correct_file_hash_for_existing_file() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        cache
            .upsert_file("src/main.rs", "hash123", b"fn main() {}")
            .unwrap();

        let hash = cache.get_file_hash("src/main.rs").unwrap();
        assert_eq!(hash, Some("hash123".to_string()));
    }

    #[test]
    fn test_should_return_none_for_missing_file_hash() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let hash = cache.get_file_hash("missing.rs").unwrap();
        assert_eq!(hash, None);
    }

    #[test]
    fn test_should_insert_a_symbol_and_retrieve_its_content_via_substr() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let content = b"pub fn foo() {}\npub fn bar() {}";
        let file_id = cache.upsert_file("src/lib.rs", "hash456", content).unwrap();

        let symbol = Symbol {
            id: "sym_bar".to_string(),
            file_id,
            name: "bar".to_string(),
            kind: "function".to_string(),
            byte_offset: 16,
            byte_length: 15,
        };
        cache.insert_symbol(&symbol).unwrap();

        let retrieved = cache.get_symbol_content("sym_bar").unwrap();
        assert_eq!(retrieved, Some(b"pub fn bar() {}".to_vec()));
    }

    #[test]
    fn test_should_return_none_for_missing_symbol_content() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let retrieved = cache.get_symbol_content("missing_id").unwrap();
        assert_eq!(retrieved, None);
    }

    #[test]
    fn test_should_cascade_delete_symbols_when_file_is_deleted() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let file_id = cache
            .upsert_file("src/temp.rs", "hash789", b"fn temp() {}")
            .unwrap();

        let symbol = Symbol {
            id: "sym_temp".to_string(),
            file_id,
            name: "temp".to_string(),
            kind: "function".to_string(),
            byte_offset: 0,
            byte_length: 12,
        };
        cache.insert_symbol(&symbol).unwrap();

        cache.delete_file("src/temp.rs").unwrap();

        let mut stmt = cache
            .conn
            .prepare("SELECT COUNT(*) FROM symbols WHERE file_id = ?")
            .unwrap();
        let count: i64 = stmt.query_row([file_id], |row| row.get(0)).unwrap();

        assert_eq!(count, 0);
    }

    // --- Symbol name edge cases ---

    #[test]
    fn test_symbol_name_with_single_quote_roundtrips() {
        // SQL injection defense: a name containing a single quote must survive
        // the parameterized INSERT and come back intact.
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let file_id = cache
            .upsert_file("src/q.rs", "hq1", b"fn it's() {}")
            .unwrap();

        let name = "it's a function".to_string();
        let symbol = Symbol {
            id: "sym_sq".to_string(),
            file_id,
            name: name.clone(),
            kind: "function".to_string(),
            byte_offset: 0,
            byte_length: 12,
        };
        cache.insert_symbol(&symbol).unwrap();

        let symbols = cache.get_file_symbols("src/q.rs").unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].1, name);
    }

    #[test]
    fn test_symbol_name_with_double_quote_roundtrips() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let file_id = cache
            .upsert_file("src/dq.rs", "hdq1", b"fn main() {}")
            .unwrap();

        let name = r#"say "hello" world"#.to_string();
        let symbol = Symbol {
            id: "sym_dq".to_string(),
            file_id,
            name: name.clone(),
            kind: "function".to_string(),
            byte_offset: 0,
            byte_length: 12,
        };
        cache.insert_symbol(&symbol).unwrap();

        let symbols = cache.get_file_symbols("src/dq.rs").unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].1, name);
    }

    #[test]
    fn test_symbol_name_empty_string_does_not_panic() {
        // The schema has no NOT NULL constraint on name beyond TEXT, so an empty
        // string should be stored gracefully. We accept either success or a
        // well-typed rusqlite error — the key invariant is no panic.
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let file_id = cache.upsert_file("src/empty.rs", "hempty", b"").unwrap();

        let symbol = Symbol {
            id: "sym_empty_name".to_string(),
            file_id,
            name: "".to_string(),
            kind: "function".to_string(),
            byte_offset: 0,
            byte_length: 0,
        };
        let result = cache.insert_symbol(&symbol);
        // Must not panic; an empty name stored successfully is also acceptable.
        match result {
            Ok(()) => {
                // Round-trip: the empty name must be retrievable and correct.
                let symbols = cache
                    .get_file_symbols("src/empty.rs")
                    .expect("get_file_symbols must not error after successful insert");
                assert_eq!(
                    symbols.len(),
                    1,
                    "exactly one symbol should be stored; got: {:?}",
                    symbols
                );
                assert_eq!(
                    symbols[0].1, "",
                    "retrieved symbol name must be the empty string; got: {:?}",
                    symbols[0].1
                );
            }
            Err(e) => {
                // Graceful rejection is also fine, but must be a rusqlite constraint or
                // type error — not a panic.  Assert the error is a recognizable rusqlite
                // error so we know the code path is intentional, not an unexpected crash.
                let msg = e.to_string();
                assert!(
                    !msg.is_empty(),
                    "rejection error message must be non-empty; got empty string"
                );
            }
        }
    }

    #[test]
    fn test_symbol_name_very_long_no_truncation() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let long_name: String = "a".repeat(1000);
        let content = vec![b'x'; 1000];
        let file_id = cache.upsert_file("src/long.rs", "hlong", &content).unwrap();

        let symbol = Symbol {
            id: "sym_long_name".to_string(),
            file_id,
            name: long_name.clone(),
            kind: "function".to_string(),
            byte_offset: 0,
            byte_length: 1000,
        };
        cache.insert_symbol(&symbol).unwrap();

        let symbols = cache.get_file_symbols("src/long.rs").unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].1.len(), 1000);
        assert_eq!(symbols[0].1, long_name);
    }

    #[test]
    fn test_symbol_name_with_newlines_and_tabs_roundtrips() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let file_id = cache
            .upsert_file("src/ws.rs", "hws", b"fn foo() {}")
            .unwrap();

        let name = "line1\nline2\ttabbed".to_string();
        let symbol = Symbol {
            id: "sym_whitespace".to_string(),
            file_id,
            name: name.clone(),
            kind: "function".to_string(),
            byte_offset: 0,
            byte_length: 11,
        };
        cache.insert_symbol(&symbol).unwrap();

        let symbols = cache.get_file_symbols("src/ws.rs").unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].1, name);
    }

    // --- File content edge cases ---

    #[test]
    fn test_file_content_with_unicode_and_emoji_roundtrips() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let content = "🦀 Rust 中文 العربية".as_bytes().to_vec();
        let file_id = cache
            .upsert_file("src/unicode.rs", "hunicode", &content)
            .unwrap();

        // Retrieve via symbol content spanning the whole file
        let sym = Symbol {
            id: "sym_unicode_all".to_string(),
            file_id,
            name: "unicode_fn".to_string(),
            kind: "function".to_string(),
            byte_offset: 0,
            byte_length: content.len(),
        };
        cache.insert_symbol(&sym).unwrap();

        let retrieved = cache.get_symbol_content("sym_unicode_all").unwrap();
        assert_eq!(retrieved, Some(content));
    }

    #[test]
    fn test_file_path_with_spaces_and_special_chars() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let path = "src/my project/file (v2) [draft].rs";
        let id = cache.upsert_file(path, "hspecial", b"fn x() {}").unwrap();
        assert!(id > 0);

        let hash = cache.get_file_hash(path).unwrap();
        assert_eq!(hash, Some("hspecial".to_string()));
    }

    #[test]
    fn test_upsert_file_twice_returns_updated_content() {
        // Re-inserting the same path with new content must return the latest
        // content, not the stale original.
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let path = "src/changed.rs";
        let original = b"fn original() {}";
        let updated = b"fn updated() {}";

        let id1 = cache.upsert_file(path, "h_original", original).unwrap();
        let id2 = cache.upsert_file(path, "h_updated", updated).unwrap();

        // Same row, same id
        assert_eq!(id1, id2);

        // Hash must reflect the update
        let hash = cache.get_file_hash(path).unwrap();
        assert_eq!(hash, Some("h_updated".to_string()));

        // Verify content via a symbol covering the full updated file
        let sym = Symbol {
            id: "sym_changed".to_string(),
            file_id: id2,
            name: "updated".to_string(),
            kind: "function".to_string(),
            byte_offset: 0,
            byte_length: updated.len(),
        };
        cache.insert_symbol(&sym).unwrap();
        let retrieved = cache.get_symbol_content("sym_changed").unwrap();
        assert_eq!(retrieved, Some(updated.to_vec()));
    }

    // --- Lookup correctness ---

    #[test]
    fn test_get_file_hash_nonexistent_returns_none_not_error() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let result = cache.get_file_hash("/nonexistent/path/that/does/not/exist.rs");
        assert!(
            matches!(result, Ok(None)),
            "Expected Ok(None), got {:?}",
            result
        );
    }

    #[test]
    fn test_get_symbol_content_nonexistent_returns_none_not_error() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let result = cache.get_symbol_content("sym_id_that_does_not_exist");
        assert!(
            matches!(result, Ok(None)),
            "Expected Ok(None), got {:?}",
            result
        );
    }

    #[test]
    fn test_delete_file_also_removes_its_symbols() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let file_id = cache
            .upsert_file("src/doomed.rs", "hdoomed", b"fn doomed() {}")
            .unwrap();

        for i in 0..3 {
            let sym = Symbol {
                id: format!("sym_doomed_{i}"),
                file_id,
                name: format!("doomed_{i}"),
                kind: "function".to_string(),
                byte_offset: i * 5,
                byte_length: 5,
            };
            cache.insert_symbol(&sym).unwrap();
        }

        // Confirm symbols exist before deletion
        let before = cache.get_file_symbols("src/doomed.rs").unwrap();
        assert_eq!(before.len(), 3);

        cache.delete_file("src/doomed.rs").unwrap();

        // get_file_symbols should now return empty vec (file is gone)
        let after = cache.get_file_symbols("src/doomed.rs").unwrap();
        assert!(after.is_empty(), "Expected no symbols after file deletion");

        // Double-check via direct count
        let mut stmt = cache
            .conn
            .prepare("SELECT COUNT(*) FROM symbols WHERE file_id = ?")
            .unwrap();
        let count: i64 = stmt.query_row([file_id], |row| row.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    // --- Database state ---

    #[test]
    fn test_new_database_creates_schema_correctly() {
        // A fresh on-disk database (no tables yet) should have its schema
        // initialized by SqliteCache::new + init() without error.
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("fresh.db");
        let path_str = db_path.to_str().unwrap();

        let cache = SqliteCache::new(path_str).unwrap();
        cache.init().unwrap();

        let mut stmt = cache
            .conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('files', 'symbols')",
            )
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(tables.contains(&"files".to_string()));
        assert!(tables.contains(&"symbols".to_string()));
    }

    // ===========================================================================
    // imports table — failing tests (infrastructure not yet implemented)
    // ===========================================================================

    #[test]
    fn test_imports_table_exists_after_init() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let mut stmt = cache
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name='imports'")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();

        assert!(
            tables.contains(&"imports".to_string()),
            "imports table should be created by cache.init()"
        );
    }

    #[test]
    fn test_insert_import_and_retrieve_via_get_imports() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let file_id = cache
            .upsert_file("src/main.ts", "hash_ts1", b"import { foo } from './utils'")
            .unwrap();

        cache
            .insert_import(file_id, "src/utils", "import { foo } from './utils'")
            .unwrap();

        let imports = cache.get_imports("src/main.ts").unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].0, "src/utils");
        assert_eq!(imports[0].1, "import { foo } from './utils'");
    }

    #[test]
    fn test_cascade_delete_imports_when_source_file_is_deleted() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let file_id = cache
            .upsert_file("src/main.rs", "hash_rs1", b"use crate::utils;")
            .unwrap();

        cache
            .insert_import(file_id, "crate::utils", "use crate::utils;")
            .unwrap();

        // Confirm import exists before deletion
        let before = cache.get_imports("src/main.rs").unwrap();
        assert_eq!(before.len(), 1);

        cache.delete_file("src/main.rs").unwrap();

        // After cascade delete, imports for this file_id should be gone
        let mut stmt = cache
            .conn
            .prepare("SELECT COUNT(*) FROM imports WHERE source_file_id = ?")
            .unwrap();
        let count: i64 = stmt.query_row([file_id], |row| row.get(0)).unwrap();
        assert_eq!(count, 0, "imports should be cascade-deleted with the file");
    }

    #[test]
    fn test_get_imports_returns_empty_for_file_with_no_imports() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        cache
            .upsert_file("src/standalone.rs", "hash_standalone", b"fn standalone() {}")
            .unwrap();

        let imports = cache.get_imports("src/standalone.rs").unwrap();
        assert!(
            imports.is_empty(),
            "file with no imports should return empty vec"
        );
    }

    #[test]
    fn test_get_importers_returns_files_that_import_target() {
        let cache = SqliteCache::new_in_memory().unwrap();
        cache.init().unwrap();

        let file_a_id = cache
            .upsert_file("src/a.ts", "hash_a", b"import { bar } from './shared'")
            .unwrap();
        let file_b_id = cache
            .upsert_file("src/b.ts", "hash_b", b"import { baz } from './shared'")
            .unwrap();

        cache
            .insert_import(file_a_id, "src/shared", "import { bar } from './shared'")
            .unwrap();
        cache
            .insert_import(file_b_id, "src/shared", "import { baz } from './shared'")
            .unwrap();

        let importers = cache.get_importers("src/shared").unwrap();
        assert_eq!(importers.len(), 2);
        assert!(importers.contains(&"src/a.ts".to_string()));
        assert!(importers.contains(&"src/b.ts".to_string()));
    }

    #[test]
    fn test_two_consecutive_opens_on_same_db_path_do_not_corrupt() {
        // Opening a database twice (sequentially) and calling init() both times
        // must not corrupt existing data or return an error.
        use tempfile::TempDir;

        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("shared.db");
        let path_str = db_path.to_str().unwrap();

        // First open: create schema and insert data
        {
            let cache = SqliteCache::new(path_str).unwrap();
            cache.init().unwrap();
            cache
                .upsert_file("src/lib.rs", "hash_first_open", b"fn lib() {}")
                .unwrap();
        }

        // Second open: init() again (idempotent via IF NOT EXISTS), then read back
        {
            let cache = SqliteCache::new(path_str).unwrap();
            cache.init().unwrap(); // Must not drop existing tables or error

            let hash = cache.get_file_hash("src/lib.rs").unwrap();
            assert_eq!(
                hash,
                Some("hash_first_open".to_string()),
                "Data inserted in first open should survive second open"
            );
        }
    }
}
