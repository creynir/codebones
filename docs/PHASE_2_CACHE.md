# Phase 2: SQLite Cache Layer Design Document

## 1. Overview
This document outlines the design for the SQLite-based caching layer in `codebones`. 

Our research into `jcodemunch-mcp` revealed that they use a JSON-based index file paired with raw source files stored in a directory structure. They handle incremental updates by hashing file contents (SHA-256) and comparing them against stored hashes. For symbol retrieval, they store `byte_offset` and `byte_length`, then open the raw file, `seek()` to the offset, and read the bytes.

**Our Improvement:** We will use SQLite (`rusqlite` crate) to store both the metadata and the raw file contents as `BLOB`s. This provides atomic writes, avoids filesystem path traversal vulnerabilities, and allows us to use SQLite's built-in `substr()` function to retrieve symbol contents in O(1) time without loading the entire file into memory.

## 2. SQLite Schema

We will use two primary tables: `files` and `symbols`.

```sql
-- Store files and their raw content
CREATE TABLE files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    hash TEXT NOT NULL,          -- SHA-256 hash for incremental updates
    content BLOB NOT NULL        -- Raw file content
);

-- Store symbols with byte offsets for O(1) retrieval
CREATE TABLE symbols (
    id TEXT PRIMARY KEY,         -- Unique symbol identifier
    file_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    byte_offset INTEGER NOT NULL,
    byte_length INTEGER NOT NULL,
    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE
);

-- Indexes for fast lookups
CREATE INDEX idx_symbols_file_id ON symbols(file_id);
CREATE INDEX idx_symbols_name ON symbols(name);
```

### O(1) Symbol Retrieval
To retrieve a symbol's content without loading the full file, we will use:
```sql
SELECT substr(f.content, s.byte_offset + 1, s.byte_length) 
FROM symbols s
JOIN files f ON s.file_id = f.id
WHERE s.id = ?;
```
*(Note: SQLite's `substr` is 1-indexed, so we add 1 to the 0-indexed `byte_offset`)*

## 3. Rust Data Structures & Traits

We will use the `rusqlite` crate for SQLite interactions.

```rust
use rusqlite::Connection;

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
}
```

## 4. TDD Unit Tests

The test-writing agent MUST write the following exact tests *before* the implementation agent writes the logic:

1.  **Test: Should initialize schema successfully**
    *   *Action:* Call `init()` on an in-memory SQLite connection.
    *   *Assert:* The `files` and `symbols` tables exist in `sqlite_master`.
2.  **Test: Should upsert a file and return its ID**
    *   *Action:* Call `upsert_file("src/main.rs", "hash123", b"fn main() {}")`.
    *   *Assert:* Returns a valid `i64` ID. Querying the `files` table directly returns the correct path and hash.
3.  **Test: Should return correct file hash for existing file**
    *   *Action:* Insert a file, then call `get_file_hash("src/main.rs")`.
    *   *Assert:* Returns `Some("hash123")`.
4.  **Test: Should return None for missing file hash**
    *   *Action:* Call `get_file_hash("missing.rs")`.
    *   *Assert:* Returns `None`.
5.  **Test: Should insert a symbol and retrieve its content via substr**
    *   *Action:* Insert a file with content `b"pub fn foo() {}\npub fn bar() {}"`. Insert a symbol for `bar` with correct offset and length. Call `get_symbol_content`.
    *   *Assert:* Returns `Some(b"pub fn bar() {}")`.
6.  **Test: Should return None for missing symbol content**
    *   *Action:* Call `get_symbol_content("missing_id")`.
    *   *Assert:* Returns `None`.
7.  **Test: Should cascade delete symbols when file is deleted**
    *   *Action:* Insert a file and a symbol. Call `delete_file`.
    *   *Assert:* Querying the `symbols` table directly shows 0 rows.
