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
    pub conn: Connection,
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
}
