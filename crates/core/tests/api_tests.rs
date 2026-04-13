/// Integration tests for codebones_core::api
///
/// These tests describe DESIRED behavior. Some currently fail — that is expected and intentional.
/// They are written TDD-style: tests first, implementation fixes second.
use codebones_core::api::{self, PackOptions};
use std::fs;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Shared fixture helpers
// ---------------------------------------------------------------------------

/// Write a realistic Rust source file with functions, structs, and an impl block.
fn write_rust_fixture(dir: &TempDir, filename: &str, content: &str) {
    let path = dir.path().join(filename);
    fs::write(&path, content).expect("failed to write fixture file");
}

/// A minimal but realistic Rust source snippet used across many tests.
const RUST_FIXTURE: &str = r#"
/// A simple greeting struct.
pub struct Greeter {
    name: String,
}

impl Greeter {
    /// Creates a new Greeter.
    pub fn new(name: &str) -> Self {
        Greeter { name: name.to_owned() }
    }

    /// Returns a greeting string.
    pub fn greet(&self) -> String {
        format!("Hello, {}!", self.name)
    }
}

/// A standalone helper function.
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;

/// A second Rust file to test multi-file scenarios.
const RUST_FIXTURE_B: &str = r#"
pub fn multiply(x: i32, y: i32) -> i32 {
    x * y
}
"#;

/// Returns a valid PackOptions with all features disabled (minimal defaults).
fn default_pack_options() -> PackOptions {
    PackOptions {
        no_file_summary: false,
        no_files: false,
        remove_comments: false,
        remove_empty_lines: false,
        truncate_base64: false,
        include: None,
        ignore: None,
    }
}

// ===========================================================================
// api::index() tests
// ===========================================================================

#[test]
fn index_creates_db_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    api::index(dir.path()).expect("index should succeed");

    assert!(
        dir.path().join(".codebones").join("codebones.db").exists(),
        ".codebones/codebones.db should be created after indexing"
    );
    Ok(())
}

#[test]
fn index_idempotent_on_unchanged_directory() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    api::index(dir.path()).expect("first index should succeed");
    api::index(dir.path()).expect("second index should succeed without error");

    // After two identical runs the DB should still have exactly one file entry.
    let db_path = dir.path().join(".codebones").join("codebones.db");
    let conn = rusqlite::Connection::open(&db_path)?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
    assert_eq!(
        count, 1,
        "re-indexing unchanged dir should not duplicate file rows"
    );
    Ok(())
}

#[test]
fn index_updates_cache_after_file_changes() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    api::index(dir.path()).expect("first index");

    // Modify the file.
    let modified = format!("{}\npub fn extra() {{}}", RUST_FIXTURE);
    fs::write(dir.path().join("lib.rs"), &modified).expect("write modified file");

    api::index(dir.path()).expect("second index after modification");

    // The search index should now include the new symbol.
    let results = api::search(dir.path(), "extra").expect("search after re-index");
    assert!(
        !results.is_empty(),
        "newly added symbol 'extra' should appear after re-indexing"
    );
    Ok(())
}

#[test]
fn index_prunes_deleted_files_on_reindex() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    fs::write(dir.path().join("lib.rs"), "pub fn compat() {}\n").expect("write initial file");

    api::index(dir.path()).expect("first index");

    let initial_results = api::search(dir.path(), "compat").expect("initial search");
    assert_eq!(
        initial_results,
        vec!["lib.rs::compat".to_string()],
        "initial index should contain the compat symbol"
    );

    fs::remove_file(dir.path().join("lib.rs")).expect("remove indexed file");
    api::index(dir.path()).expect("second index after delete");

    let db_path = dir.path().join(".codebones").join("codebones.db");
    let conn = rusqlite::Connection::open(&db_path)?;
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM files", [], |r| r.get(0))?;
    assert_eq!(count, 0, "deleted files should be pruned from the cache");

    let results = api::search(dir.path(), "compat").expect("search after delete");
    assert!(
        results.is_empty(),
        "deleted symbol 'compat' should not appear after re-indexing"
    );
    Ok(())
}

#[test]
fn index_handles_empty_directory_gracefully() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");

    // No files — must not panic or return an error.
    api::index(dir.path()).expect("indexing an empty directory should succeed");
    Ok(())
}

#[test]
fn index_skips_binary_files_without_error() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    // Write a file that looks binary (contains null bytes).
    let binary_content = b"binary\x00data\x00here";
    fs::write(dir.path().join("binary.bin"), binary_content).expect("write binary file");
    // Also write a normal text file so we can confirm it is indexed.
    write_rust_fixture(&dir, "normal.rs", RUST_FIXTURE);

    api::index(dir.path()).expect("indexing with binary files should succeed");

    // The normal text file should be indexed, binary skipped.
    let results = api::search(dir.path(), "add").expect("search after index");
    assert!(
        !results.is_empty(),
        "normal file should be indexed despite binary sibling"
    );
    Ok(())
}

#[test]
fn index_returns_error_for_nonexistent_directory() {
    let result = api::index(std::path::Path::new(
        "/nonexistent/path/that/does/not/exist/xyz",
    ));
    assert!(
        result.is_err(),
        "indexing nonexistent directory should return an error"
    );
}

// ---------------------------------------------------------------------------
// Permission-denied test — only meaningful on Unix; skip on Windows.
// ---------------------------------------------------------------------------

#[test]
#[cfg(unix)]
fn index_skips_permission_denied_file_and_continues() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "readable.rs", RUST_FIXTURE);

    let restricted = dir.path().join("restricted.rs");
    fs::write(&restricted, "pub fn secret() {}").expect("write restricted file");
    let mut perms = fs::metadata(&restricted)?.permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&restricted, perms)?;

    // Should not error; should index readable.rs and skip restricted.rs.
    api::index(dir.path()).expect("permission denied files should be skipped");

    // Restore permissions so TempDir cleanup doesn't fail.
    let mut perms = fs::metadata(&restricted)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&restricted, perms)?;

    let results = api::search(dir.path(), "greet").expect("search");
    assert!(
        !results.is_empty(),
        "readable file should be indexed even when sibling is unreadable"
    );
    Ok(())
}

#[test]
#[cfg(unix)]
fn index_preserves_cached_content_for_previously_indexed_unreadable_file_on_reindex(
) -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().expect("failed to create tempdir");
    let restricted = dir.path().join("restricted.rs");
    fs::write(&restricted, "pub fn secret() -> &'static str { \"ok\" }\n")
        .expect("write restricted file");

    api::index(dir.path()).expect("initial index should succeed");
    assert_eq!(
        api::search(dir.path(), "secret").expect("initial search"),
        vec!["restricted.rs::secret".to_string()]
    );

    let mut perms = fs::metadata(&restricted)?.permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&restricted, perms)?;

    api::index(dir.path()).expect("reindex should skip unreadable files without pruning cache");

    let mut perms = fs::metadata(&restricted)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&restricted, perms)?;

    let results = api::search(dir.path(), "secret").expect("search after unreadable reindex");
    assert_eq!(
        results,
        vec!["restricted.rs::secret".to_string()],
        "previously indexed unreadable files should keep their cached symbols"
    );

    let content = api::get(dir.path(), "restricted.rs").expect("cached file content");
    assert!(
        content.contains("pub fn secret()"),
        "cached file content should be preserved for unreadable files"
    );

    Ok(())
}

// ===========================================================================
// api::get() tests
// ===========================================================================

#[test]
fn get_returns_source_for_known_symbol() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    // The symbol id is built as "<rel_path>::<qualified_name>".
    // We must find a valid id from search first.
    let results = api::search(dir.path(), "add").expect("search for 'add'");
    assert!(!results.is_empty(), "should find 'add' symbol");

    let symbol_id = &results[0];
    let source =
        api::get(dir.path(), symbol_id).expect("get should return source for known symbol");
    assert!(
        source.contains("add"),
        "returned source should contain the symbol name 'add'; got: {source}"
    );
    Ok(())
}

#[test]
fn get_returns_file_content_for_path() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    // The file is stored in the DB by relative path.
    let content = api::get(dir.path(), "lib.rs").expect("get should return file content for path");
    assert!(
        content.contains("pub fn add"),
        "returned file content should contain expected function; got: {content}"
    );
    Ok(())
}

#[test]
fn get_returns_error_for_nonexistent_symbol() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    let result = api::get(dir.path(), "lib.rs::nonexistent_symbol_xyz");
    assert!(
        result.is_err(),
        "get with missing symbol should return an error"
    );
    Ok(())
}

#[test]
fn get_returns_error_when_no_index_exists() {
    let dir = TempDir::new().expect("failed to create tempdir");
    // No index was ever created.
    let result = api::get(dir.path(), "lib.rs");
    assert!(
        result.is_err(),
        "get without a prior index should return an error"
    );
}

#[test]
fn get_handles_unicode_in_symbol_name() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    // A symbol with a Unicode name will simply not be found — verify it errors cleanly.
    let result = api::get(dir.path(), "lib.rs::函数_unicode");
    assert!(
        result.is_err(),
        "get for unicode symbol that doesn't exist should return an error, not panic"
    );
    Ok(())
}

// ===========================================================================
// api::outline() tests
// ===========================================================================

#[test]
fn outline_elides_function_bodies_with_ellipsis() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    let outline = api::outline(dir.path(), "lib.rs").expect("outline should succeed");

    // Bodies should be replaced with "..."
    assert!(
        outline.contains("..."),
        "outline should contain '...' where function bodies were elided; got: {outline}"
    );
    // The literal body of `add` should NOT be in the skeleton.
    assert!(
        !outline.contains("a + b"),
        "function body 'a + b' should be elided in outline; got: {outline}"
    );
    Ok(())
}

#[test]
fn outline_falls_back_to_raw_content_for_unsupported_type() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = TempDir::new().expect("failed to create tempdir");
    let txt_path = dir.path().join("notes.txt");
    fs::write(&txt_path, "This is plain text.\nNo code here.").expect("write txt file");
    api::index(dir.path()).expect("index");

    // .txt is unsupported; outline should return raw content unchanged.
    let outline =
        api::outline(dir.path(), "notes.txt").expect("outline of txt file should succeed");
    assert!(
        outline.contains("This is plain text."),
        "outline of unsupported type should return raw content; got: {outline}"
    );
    Ok(())
}

#[test]
fn outline_returns_error_for_nonexistent_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    api::index(dir.path()).expect("index");

    let result = api::outline(dir.path(), "does_not_exist.rs");
    assert!(
        result.is_err(),
        "outline of nonexistent file should return an error"
    );
    Ok(())
}

// ===========================================================================
// AC: .codebones/ migration and first-run setup (RED — these tests must fail
//     until the implementation is updated)
// ===========================================================================

/// AC1: `index` creates the database at `.codebones/codebones.db`, not at the
/// project root.
#[test]
fn test_index_creates_db_in_dot_codebones_dir() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    api::index(dir.path()).expect("index should succeed");

    assert!(
        dir.path().join(".codebones").join("codebones.db").exists(),
        ".codebones/codebones.db must be created after indexing"
    );
    Ok(())
}

/// AC4: `index` creates `.codebones/` automatically when it does not exist.
#[test]
fn test_index_creates_dot_codebones_directory() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    // Pre-condition: directory must not exist yet.
    assert!(
        !dir.path().join(".codebones").exists(),
        ".codebones/ must not exist before first index"
    );

    api::index(dir.path()).expect("index should succeed");

    assert!(
        dir.path().join(".codebones").is_dir(),
        ".codebones/ must be created automatically by index"
    );
    Ok(())
}

/// AC2: `search` reads the database from `.codebones/codebones.db` after
/// `index` has been run.
#[test]
fn test_search_reads_db_from_dot_codebones() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    api::index(dir.path()).expect("index should succeed");

    // Confirm the db lives in .codebones/ — if it doesn't, this test is broken
    // by the wrong reason and we want to catch that.
    assert!(
        dir.path().join(".codebones").join("codebones.db").exists(),
        "pre-condition: .codebones/codebones.db must exist"
    );

    let results = api::search(dir.path(), "add").expect("search should succeed");
    assert!(
        !results.is_empty(),
        "search should find symbols indexed via .codebones/codebones.db"
    );
    Ok(())
}

/// AC3: If an old `codebones.db` exists at the project root, `index` deletes
/// it and creates a fresh database at `.codebones/codebones.db`.
#[test]
fn test_index_deletes_legacy_root_db_and_creates_new_in_dot_codebones(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    // Plant a legacy db at the root.
    let legacy_db = dir.path().join("codebones.db");
    fs::write(&legacy_db, b"legacy sqlite data").expect("write legacy db");
    assert!(legacy_db.exists(), "pre-condition: legacy db must exist");

    api::index(dir.path()).expect("index should succeed");

    assert!(
        !legacy_db.exists(),
        "legacy codebones.db at root must be deleted by index"
    );
    assert!(
        dir.path().join(".codebones").join("codebones.db").exists(),
        ".codebones/codebones.db must be created after legacy db removal"
    );
    Ok(())
}

/// AC5 + AC6: When `.git/` exists and `.gitignore` does not, `index` creates
/// `.gitignore` containing `.codebones/`.
#[test]
fn test_index_creates_gitignore_with_dot_codebones_when_git_exists(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    // Simulate a git repository (just the directory is enough for detection).
    fs::create_dir(dir.path().join(".git")).expect("create .git dir");

    // No .gitignore yet.
    assert!(
        !dir.path().join(".gitignore").exists(),
        "pre-condition: .gitignore must not exist"
    );

    api::index(dir.path()).expect("index should succeed");

    let gitignore_path = dir.path().join(".gitignore");
    assert!(
        gitignore_path.exists(),
        ".gitignore must be created when .git/ exists"
    );
    let contents = fs::read_to_string(&gitignore_path)?;
    assert!(
        contents.contains(".codebones/"),
        ".gitignore must contain '.codebones/' entry; got: {contents}"
    );
    Ok(())
}

/// AC5: When `.gitignore` already exists but lacks `.codebones/`, `index`
/// appends the entry.
#[test]
fn test_index_appends_dot_codebones_to_existing_gitignore() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    fs::create_dir(dir.path().join(".git")).expect("create .git dir");
    fs::write(dir.path().join(".gitignore"), "target/\n*.log\n")
        .expect("write existing .gitignore");

    api::index(dir.path()).expect("index should succeed");

    let contents = fs::read_to_string(dir.path().join(".gitignore"))?;
    assert!(
        contents.contains(".codebones/"),
        ".gitignore must contain '.codebones/' after index; got: {contents}"
    );
    // Original content must still be present.
    assert!(
        contents.contains("target/"),
        "original .gitignore content must be preserved; got: {contents}"
    );
    Ok(())
}

/// AC7: When `.gitignore` already contains `.codebones/`, `index` does NOT
/// add a duplicate entry.
#[test]
fn test_index_does_not_duplicate_gitignore_entry() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    fs::create_dir(dir.path().join(".git")).expect("create .git dir");
    fs::write(dir.path().join(".gitignore"), ".codebones/\ntarget/\n")
        .expect("write .gitignore with existing entry");

    // Run index twice to stress-test idempotency.
    api::index(dir.path()).expect("first index");
    api::index(dir.path()).expect("second index");

    let contents = fs::read_to_string(dir.path().join(".gitignore"))?;
    let occurrences = contents.matches(".codebones/").count();
    assert_eq!(
        occurrences, 1,
        ".codebones/ must appear exactly once in .gitignore; got {occurrences} occurrences"
    );
    Ok(())
}

/// AC8: When `.git/` does NOT exist, `index` must NOT create or modify
/// `.gitignore`.
#[test]
fn test_index_does_not_touch_gitignore_without_git_dir() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    // No .git directory.
    assert!(
        !dir.path().join(".git").exists(),
        "pre-condition: .git must not exist"
    );

    api::index(dir.path()).expect("index should succeed");

    assert!(
        !dir.path().join(".gitignore").exists(),
        ".gitignore must NOT be created when there is no .git/ directory"
    );
    Ok(())
}

/// AC9: When `CLAUDE.md` exists, `index` appends a codebones section on first
/// run.
#[test]
fn test_index_appends_to_claude_md_when_it_exists() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    let claude_md = dir.path().join("CLAUDE.md");
    fs::write(&claude_md, "# My Project\n\nSome existing content.\n").expect("write CLAUDE.md");

    api::index(dir.path()).expect("index should succeed");

    let contents = fs::read_to_string(&claude_md)?;
    assert!(
        contents.contains("codebones"),
        "CLAUDE.md must contain a codebones section after index; got: {contents}"
    );
    // Original content must still be present.
    assert!(
        contents.contains("My Project"),
        "original CLAUDE.md content must be preserved; got: {contents}"
    );
    Ok(())
}

/// AC9 (idempotency): `index` does NOT append a duplicate codebones section
/// to `CLAUDE.md` on repeated runs.
#[test]
fn test_index_does_not_duplicate_claude_md_section() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    let claude_md = dir.path().join("CLAUDE.md");
    fs::write(&claude_md, "# My Project\n").expect("write CLAUDE.md");

    api::index(dir.path()).expect("first index");
    api::index(dir.path()).expect("second index");

    let contents = fs::read_to_string(&claude_md)?;
    // Count how many times the codebones marker appears.
    let marker_count = contents.matches("codebones").count();
    assert!(
        marker_count >= 1,
        "codebones section must be present after index; got: {contents}"
    );
    // A naive implementation would double-append; verify the section is not duplicated.
    // We check there is at most one codebones header block.
    let section_starts = contents.matches("## codebones").count()
        + contents.matches("## Codebones").count()
        + contents.matches("<!-- codebones -->").count();
    assert!(
        section_starts <= 1,
        "codebones section must appear at most once in CLAUDE.md; found {section_starts} times"
    );
    Ok(())
}

/// AC10: When `AGENTS.md` exists, `index` appends a codebones section on
/// first run.
#[test]
fn test_index_appends_to_agents_md_when_it_exists() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    let agents_md = dir.path().join("AGENTS.md");
    fs::write(&agents_md, "# Agents\n\nExisting content.\n").expect("write AGENTS.md");

    api::index(dir.path()).expect("index should succeed");

    let contents = fs::read_to_string(&agents_md)?;
    assert!(
        contents.contains("codebones"),
        "AGENTS.md must contain a codebones section after index; got: {contents}"
    );
    assert!(
        contents.contains("Agents"),
        "original AGENTS.md content must be preserved; got: {contents}"
    );
    Ok(())
}

/// AC10 (idempotency): `index` does NOT append a duplicate codebones section
/// to `AGENTS.md` on repeated runs.
#[test]
fn test_index_does_not_duplicate_agents_md_section() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    let agents_md = dir.path().join("AGENTS.md");
    fs::write(&agents_md, "# Agents\n").expect("write AGENTS.md");

    api::index(dir.path()).expect("first index");
    api::index(dir.path()).expect("second index");

    let contents = fs::read_to_string(&agents_md)?;
    let section_starts = contents.matches("## codebones").count()
        + contents.matches("## Codebones").count()
        + contents.matches("<!-- codebones -->").count();
    assert!(
        section_starts <= 1,
        "codebones section must appear at most once in AGENTS.md; found {section_starts} times"
    );
    Ok(())
}

/// AC11: When neither `CLAUDE.md` nor `AGENTS.md` exists, `index` must NOT
/// create either file.
#[test]
fn test_index_does_not_create_claude_md_or_agents_md() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    assert!(
        !dir.path().join("CLAUDE.md").exists(),
        "pre-condition: CLAUDE.md must not exist"
    );
    assert!(
        !dir.path().join("AGENTS.md").exists(),
        "pre-condition: AGENTS.md must not exist"
    );

    api::index(dir.path()).expect("index should succeed");

    assert!(
        !dir.path().join("CLAUDE.md").exists(),
        "index must NOT create CLAUDE.md when it does not already exist"
    );
    assert!(
        !dir.path().join("AGENTS.md").exists(),
        "index must NOT create AGENTS.md when it does not already exist"
    );
    Ok(())
}

#[test]
fn outline_handles_empty_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    fs::write(dir.path().join("empty.rs"), "").expect("write empty file");
    api::index(dir.path()).expect("index");

    let outline =
        api::outline(dir.path(), "empty.rs").expect("outline of empty file should succeed");
    assert!(
        outline.is_empty() || outline.trim().is_empty(),
        "outline of empty file should be empty; got: '{outline}'"
    );
    Ok(())
}

#[test]
fn outline_handles_file_with_only_comments() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    let content = "// This file intentionally left blank.\n// Just comments here.\n";
    fs::write(dir.path().join("comments_only.rs"), content).expect("write comments-only file");
    api::index(dir.path()).expect("index");

    let outline = api::outline(dir.path(), "comments_only.rs")
        .expect("outline of comment-only file should succeed");
    // No bodies to elide — content should pass through verbatim (no panic).
    assert!(
        outline.contains("//"),
        "outline of comment-only file should preserve the comments; got: {outline}"
    );
    Ok(())
}

// ===========================================================================
// api::search() tests
// ===========================================================================

#[test]
fn search_returns_matching_symbols() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    let results = api::search(dir.path(), "greet").expect("search should succeed");
    assert!(
        !results.is_empty(),
        "search for 'greet' should return at least one result"
    );
    assert!(
        results.iter().any(|s| s.contains("greet")),
        "at least one result should contain 'greet'; got: {results:?}"
    );
    Ok(())
}

#[test]
fn search_returns_empty_vec_for_no_matches() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    let results =
        api::search(dir.path(), "zzznonexistentsymbolzzz").expect("search should succeed");
    assert!(
        results.is_empty(),
        "search with no matches should return empty vec, not an error"
    );
    Ok(())
}

#[test]
fn search_handles_percent_special_character() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    // '%' is a SQL LIKE wildcard; after escaping it must be treated as a literal '%'.
    // No symbol names contain a literal '%', so the result should be an empty vec.
    let results = api::search(dir.path(), "%");
    assert!(
        results.is_ok(),
        "search with '%' should not return an error; got: {:?}",
        results.err()
    );
    let results = results.expect("search with '%' must succeed");
    assert!(
        results.is_empty(),
        "search with '%' should return empty vec after escaping (no symbols contain literal '%'); got: {:?}",
        results
    );
    Ok(())
}

#[test]
fn search_handles_underscore_special_character() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    // '_' is also a SQL LIKE wildcard; should not cause an error.
    let result = api::search(dir.path(), "_");
    assert!(
        result.is_ok(),
        "search with '_' should not return an error; got: {:?}",
        result.err()
    );
    Ok(())
}

#[test]
fn search_handles_single_quote_in_query() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    // Single quote could break naive string interpolation — parameterized queries should handle it.
    let result = api::search(dir.path(), "it's");
    assert!(
        result.is_ok(),
        "search with single quote should not panic or return an error; got: {:?}",
        result.err()
    );
    Ok(())
}

#[test]
fn search_handles_double_quote_in_query() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    let result = api::search(dir.path(), "\"quoted\"");
    assert!(
        result.is_ok(),
        "search with double quote should not error; got: {:?}",
        result.err()
    );
    Ok(())
}

#[test]
fn search_handles_backslash_in_query() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    let result = api::search(dir.path(), "path\\to\\something");
    assert!(
        result.is_ok(),
        "search with backslash should not panic or error; got: {:?}",
        result.err()
    );
    // No symbol names contain a literal backslash, so the result must be empty.
    let results = result.expect("search with backslash must succeed");
    assert!(
        results.is_empty(),
        "search for a backslash-containing query should return empty vec (no symbols contain literal backslash); got: {results:?}"
    );
    Ok(())
}

#[test]
fn pack_markdown_fence_content_does_not_inject() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");

    // File whose content contains triple backticks — a potential fence injection vector.
    let content_with_backticks = "normal line\n```\ninjected content\n```\nend\n";
    fs::write(dir.path().join("tricky.txt"), content_with_backticks)
        .expect("write file with backtick content");

    let output = api::pack(dir.path(), "markdown", None, default_pack_options())
        .expect("pack with markdown format should succeed");

    // The dynamic fence must use 4 backticks (content has 3) to prevent injection.
    // The outer fence opener should be exactly 4 backticks — a 3-backtick opener would
    // be closed prematurely by the ``` inside the file content.
    // Verify that the file's code block does NOT open with a 3-backtick fence.
    // The correct pattern is: the code block for tricky.txt opens with "````" (4 backticks).
    assert!(
        !output.contains("\n```\nnormal line"),
        "markdown output must not open the file block with a 3-backtick fence —          that would be closeable by the ``` in the file content (injection); got:\n{output}"
    );

    // All original content must still be present in the output.
    assert!(
        output.contains("normal line"),
        "markdown output must preserve 'normal line' from the file; got:\n{output}"
    );
    assert!(
        output.contains("injected content"),
        "markdown output must preserve 'injected content' from the file (inside a code block); got:\n{output}"
    );
    assert!(
        output.contains("end"),
        "markdown output must preserve 'end' from the file; got:\n{output}"
    );

    // The outer fence must be 4 backticks (one more than the 3 in the file content).
    assert!(
        output.contains("````"),
        "markdown output should use a 4-backtick fence when file content contains 3 backticks; got:\n{output}"
    );

    Ok(())
}

#[test]
fn search_handles_empty_query_string() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    api::index(dir.path()).expect("index");

    // An empty query should match all symbols (LIKE %%) or return empty — must not error.
    let result = api::search(dir.path(), "");
    assert!(
        result.is_ok(),
        "empty search query should not return an error; got: {:?}",
        result.err()
    );
    let results = result.expect("empty search must succeed");
    // The desired behavior: empty query matches all indexed symbols.
    assert!(
        !results.is_empty(),
        "empty search query should match all indexed symbols; got empty vec"
    );
    Ok(())
}

#[test]
fn search_on_fresh_dir_returns_empty_not_error() {
    let dir = TempDir::new().expect("failed to create tempdir");
    // No codebones.db — SqliteCache::new() auto-creates the DB via Connection::open(),
    // so there is never a "no DB" state. A fresh directory has an empty index, not an error.
    let result = api::search(dir.path(), "anything");
    assert!(
        result.is_ok(),
        "search on a never-indexed directory should return Ok (DB is auto-created)"
    );
    let results = result.expect("search must succeed");
    assert!(
        results.is_empty(),
        "search on a never-indexed directory should return an empty vec; got: {results:?}"
    );
}

// ===========================================================================
// api::pack() tests
// ===========================================================================

#[test]
fn pack_produces_valid_xml_output_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    let output = api::pack(dir.path(), "xml", None, default_pack_options())
        .expect("pack with xml format should succeed");

    assert!(
        output.contains("<repository>"),
        "XML output should start with <repository> tag; got: {output}"
    );
    assert!(
        output.contains("</repository>"),
        "XML output should end with </repository> tag; got: {output}"
    );
    assert!(
        output.contains("<file"),
        "XML output should contain <file ...> elements; got: {output}"
    );
    Ok(())
}

#[test]
fn pack_produces_valid_markdown_output() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    let output = api::pack(dir.path(), "markdown", None, default_pack_options())
        .expect("pack with markdown format should succeed");

    // Markdown output should not contain XML tags.
    assert!(
        !output.contains("<repository>"),
        "Markdown output should not contain XML tags; got: {output}"
    );
    assert!(
        output.contains("## "),
        "Markdown output should contain ## headings for files; got: {output}"
    );
    assert!(
        output.contains("```"),
        "Markdown output should contain fenced code blocks; got: {output}"
    );
    Ok(())
}

#[test]
fn pack_rejects_invalid_output_format() {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    let error = api::pack(dir.path(), "json", None, default_pack_options())
        .expect_err("invalid output format should return an error");
    assert!(
        error.to_string().contains("Invalid output format: json"),
        "unexpected error: {error}"
    );
}

#[test]
fn pack_no_files_produces_skeleton_map_only() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    let options = PackOptions {
        no_files: true,
        ..default_pack_options()
    };
    let output =
        api::pack(dir.path(), "xml", None, options).expect("pack with no_files should succeed");

    assert!(
        output.contains("<skeleton_map>"),
        "no_files output should contain skeleton_map; got: {output}"
    );
    assert!(
        !output.contains("<content>") && !output.contains("<![CDATA["),
        "no_files output must not contain <content> blocks; got: {output}"
    );
    Ok(())
}

#[test]
fn pack_no_file_summary_omits_skeleton_map_and_includes_content(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    let options = PackOptions {
        no_file_summary: true,
        ..default_pack_options()
    };
    let output = api::pack(dir.path(), "xml", None, options)
        .expect("pack with no_file_summary should succeed");

    assert!(
        !output.contains("<skeleton_map>"),
        "no_file_summary output should NOT contain skeleton_map; got: {output}"
    );
    // File content blocks should still be present.
    assert!(
        output.contains("<content>") || output.contains("<![CDATA["),
        "no_file_summary output should still include file content; got: {output}"
    );
    Ok(())
}

#[test]
fn pack_include_glob_filters_to_matching_files() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    write_rust_fixture(&dir, "other.rs", RUST_FIXTURE_B);
    fs::write(dir.path().join("readme.txt"), "documentation").expect("write txt");

    let options = PackOptions {
        include: Some(vec!["*.rs".to_string()]),
        ..default_pack_options()
    };
    let output =
        api::pack(dir.path(), "xml", None, options).expect("pack with include glob should succeed");

    assert!(
        !output.contains("readme.txt"),
        "include '*.rs' should exclude readme.txt; got: {output}"
    );
    assert!(
        output.contains("lib.rs") || output.contains("other.rs"),
        "include '*.rs' should include at least one .rs file; got: {output}"
    );
    Ok(())
}

#[test]
fn pack_ignore_glob_excludes_matching_files() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    write_rust_fixture(&dir, "other.rs", RUST_FIXTURE_B);

    let options = PackOptions {
        ignore: Some(vec!["other.rs".to_string()]),
        ..default_pack_options()
    };
    let output =
        api::pack(dir.path(), "xml", None, options).expect("pack with ignore glob should succeed");

    assert!(
        !output.contains("other.rs"),
        "ignore 'other.rs' should exclude that file from output; got: {output}"
    );
    assert!(
        output.contains("lib.rs"),
        "lib.rs should still appear since it is not ignored; got: {output}"
    );
    Ok(())
}

#[test]
fn pack_max_tokens_causes_graceful_degradation() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    // Write a large enough file that even a low token budget gets exhausted.
    let large_content: String = (0..200)
        .map(|i| format!("pub fn func_{i}(x: i32) -> i32 {{ x + {i} }}\n"))
        .collect();
    fs::write(dir.path().join("large.rs"), &large_content).expect("write large file");

    let options = default_pack_options();
    // Set a very small token budget to force degradation.
    let output = api::pack(dir.path(), "xml", Some(5), options)
        .expect("pack with low max_tokens should not error");

    // When budget is exceeded, file content blocks should be omitted.
    // The skeleton map may still be present.
    assert!(
        !output.contains("<![CDATA["),
        "With max_tokens=5, file content must be omitted from output; got: {output}"
    );
    assert!(
        output.contains("<skeleton_map>"),
        "Skeleton map must still be present even when token budget exhausted; got: {output}"
    );
    Ok(())
}

#[test]
fn pack_remove_comments_strips_comments_from_output() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    let content_with_comments = r#"
// This is a line comment that should be stripped.
pub fn documented() -> i32 {
    /* block comment inside body */
    42
}
"#;
    fs::write(dir.path().join("commented.rs"), content_with_comments).expect("write");

    let options = PackOptions {
        remove_comments: true,
        ..default_pack_options()
    };
    let output = api::pack(dir.path(), "xml", None, options)
        .expect("pack with remove_comments should succeed");

    assert!(
        !output.contains("line comment that should be stripped"),
        "remove_comments should strip line comments; got: {output}"
    );
    assert!(
        !output.contains("block comment inside body"),
        "remove_comments should strip block comments; got: {output}"
    );
    Ok(())
}

#[test]
fn pack_truncate_base64_replaces_long_base64_strings() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    // A string that looks like base64 and is >100 chars.
    // Must be at module level (const/static) so the packer does not elide it as a function body.
    let long_b64 = "A".repeat(120);
    let content = format!("pub const ENCODED: &str = \"{long_b64}\";\n");
    fs::write(dir.path().join("b64.rs"), &content).expect("write b64 file");

    let options = PackOptions {
        truncate_base64: true,
        ..default_pack_options()
    };
    let output = api::pack(dir.path(), "xml", None, options)
        .expect("pack with truncate_base64 should succeed");

    assert!(
        !output.contains(&long_b64),
        "truncate_base64 should replace long base64-like strings; got: {output}"
    );
    assert!(
        output.contains("[TRUNCATED_BASE64]"),
        "truncate_base64 should insert [TRUNCATED_BASE64] placeholder; got: {output}"
    );
    Ok(())
}

// ===========================================================================
// Additional adversarial / edge-case tests
// ===========================================================================

#[test]
fn pack_on_empty_directory_returns_well_formed_xml() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");

    let output = api::pack(dir.path(), "xml", None, default_pack_options())
        .expect("pack on empty directory should succeed");

    // Even with no files the XML envelope must be well-formed.
    assert!(
        output.contains("<repository>") && output.contains("</repository>"),
        "empty directory pack should still produce valid XML envelope; got: {output}"
    );
    Ok(())
}

#[test]
fn search_finds_symbol_across_multiple_indexed_files() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    write_rust_fixture(&dir, "math.rs", RUST_FIXTURE_B);
    api::index(dir.path()).expect("index");

    let results = api::search(dir.path(), "multiply").expect("search");
    assert!(
        !results.is_empty(),
        "symbol 'multiply' from math.rs should be found; got: {results:?}"
    );
    Ok(())
}

#[test]
fn get_returns_full_source_for_file_with_unicode_content() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = TempDir::new().expect("failed to create tempdir");
    let unicode_content =
        "// 日本語コメント\npub fn hello() -> &'static str {\n    \"こんにちは\"\n}\n";
    fs::write(dir.path().join("unicode.rs"), unicode_content).expect("write unicode file");
    api::index(dir.path()).expect("index");

    let content =
        api::get(dir.path(), "unicode.rs").expect("get file with unicode content should succeed");

    assert!(
        content.contains("日本語コメント"),
        "get should preserve Unicode content; got: {content}"
    );
    Ok(())
}

#[test]
fn outline_returns_error_when_no_index_exists() {
    let dir = TempDir::new().expect("failed to create tempdir");
    // No index — outline should fail.
    let result = api::outline(dir.path(), "missing.rs");
    assert!(
        result.is_err(),
        "outline without prior index should return an error"
    );
}

#[test]
fn pack_with_xml_special_chars_in_symbol_names() -> Result<(), Box<dyn std::error::Error>> {
    // Rust identifiers cannot contain <>&, but FILE PATHS can on most filesystems.
    // Write a fixture file and give it a name containing '&' so the path attribute
    // in the XML output must be escaped as &amp;.
    let dir = TempDir::new().expect("failed to create tempdir");

    // Create a file whose name contains an ampersand (legal on Linux/macOS).
    let file_name = "module&utils.rs";
    let path = dir.path().join(file_name);
    fs::write(&path, RUST_FIXTURE).expect("failed to write fixture file with & in name");

    // Index and pack — no prior index() call required; pack() indexes on-the-fly via the packer.
    let output = api::pack(dir.path(), "xml", None, default_pack_options())
        .expect("pack should succeed even with & in file path");

    // The output must not contain a bare & outside of XML entity references.
    // Strip all well-formed XML entities first, then check no bare & remains.
    //
    // Well-formed XML entity references: &amp; &lt; &gt; &quot; &apos; &#NNN; &#xNNN;
    // We replace those then assert no bare & is left.
    let entities_stripped = output
        .replace("&amp;", "AMP")
        .replace("&lt;", "LT")
        .replace("&gt;", "GT")
        .replace("&quot;", "QUOT")
        .replace("&apos;", "APOS");

    // Any remaining & is unescaped — that is a malformed XML document.
    assert!(
        !entities_stripped.contains('&'),
        "Bare & found in XML output after stripping well-formed entities. \
         File path containing '&' must be escaped as &amp; in XML attributes; got:\n{}",
        output
    );

    // Confirm the escaped form is present (the & from the filename must be &amp;).
    assert!(
        output.contains("&amp;"),
        "Expected &amp; in XML output for a filename containing '&'; got:\n{}",
        output
    );

    // The document envelope must be well-formed.
    assert!(
        output.contains("<repository>") && output.contains("</repository>"),
        "XML output must have well-formed <repository> envelope; got:\n{}",
        output
    );

    Ok(())
}

// ===========================================================================
// Import parsing infrastructure — failing tests
// ===========================================================================

#[test]
fn index_populates_imports_table_for_typescript_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    fs::write(
        dir.path().join("main.ts"),
        "import { readFile } from 'fs';\nimport './styles.css';\n\nexport function main() {}\n",
    )
    .expect("write main.ts");

    api::index(dir.path()).expect("index should succeed");

    let imports: Vec<String> =
        api::get_imports(dir.path(), "main.ts").expect("get_imports should succeed");
    assert!(
        !imports.is_empty(),
        "imports table should be populated for TypeScript file after indexing; got empty list"
    );
    assert!(
        imports.iter().any(|i| i.contains("fs")),
        "should have import targeting 'fs'; got: {:?}",
        imports
    );
    Ok(())
}

#[test]
fn reindex_updates_imports_when_file_changes() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    fs::write(
        dir.path().join("app.ts"),
        "import { foo } from './foo';\n\nexport const x = 1;\n",
    )
    .expect("write initial app.ts");

    api::index(dir.path()).expect("first index");

    let initial_imports: Vec<String> =
        api::get_imports(dir.path(), "app.ts").expect("initial get_imports");
    assert!(
        initial_imports.iter().any(|i| i.contains("foo")),
        "initial index should record './foo' import; got: {:?}",
        initial_imports
    );

    // Replace the import with a different one
    fs::write(
        dir.path().join("app.ts"),
        "import { bar } from './bar';\n\nexport const x = 1;\n",
    )
    .expect("write updated app.ts");

    api::index(dir.path()).expect("second index after file change");

    let updated_imports: Vec<String> =
        api::get_imports(dir.path(), "app.ts").expect("updated get_imports");
    assert!(
        !updated_imports.iter().any(|i| i.contains("foo")),
        "stale './foo' import should be removed after re-index; got: {:?}",
        updated_imports
    );
    assert!(
        updated_imports.iter().any(|i| i.contains("bar")),
        "new './bar' import should appear after re-index; got: {:?}",
        updated_imports
    );
    Ok(())
}

#[test]
fn get_imports_returns_empty_for_file_with_no_imports() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    fs::write(
        dir.path().join("standalone.rs"),
        "pub fn standalone() -> i32 { 42 }\n",
    )
    .expect("write standalone.rs");

    api::index(dir.path()).expect("index");

    let imports: Vec<String> =
        api::get_imports(dir.path(), "standalone.rs").expect("get_imports should succeed");
    assert!(
        imports.is_empty(),
        "file with no import statements should return empty import list; got: {:?}",
        imports
    );
    Ok(())
}

#[test]
fn get_importers_returns_files_that_import_a_given_file() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = TempDir::new().expect("failed to create tempdir");

    // shared.ts is imported by both a.ts and b.ts
    fs::write(dir.path().join("shared.ts"), "export const shared = 1;\n").expect("write shared.ts");
    fs::write(
        dir.path().join("a.ts"),
        "import { shared } from './shared';\nexport const a = shared + 1;\n",
    )
    .expect("write a.ts");
    fs::write(
        dir.path().join("b.ts"),
        "import { shared } from './shared';\nexport const b = shared + 2;\n",
    )
    .expect("write b.ts");

    api::index(dir.path()).expect("index");

    let importers: Vec<String> =
        api::get_importers(dir.path(), "shared.ts").expect("get_importers should succeed");
    assert_eq!(
        importers.len(),
        2,
        "both a.ts and b.ts should appear as importers of shared.ts; got: {:?}",
        importers
    );
    assert!(
        importers.iter().any(|p| p.contains("a.ts")),
        "a.ts should be listed as an importer; got: {:?}",
        importers
    );
    assert!(
        importers.iter().any(|p| p.contains("b.ts")),
        "b.ts should be listed as an importer; got: {:?}",
        importers
    );
    Ok(())
}

// ===========================================================================
// api::graph() and api::graph_file() — failing tests (RED)
//
// These tests express the acceptance criteria for the graph command.
// They will fail until the implementation is added.
// ===========================================================================

/// Helper: write a small TypeScript project with a known import graph.
///
///   src/main.ts   imports ./utils and ./db
///   src/utils.ts  imports ./db
///   src/db.ts     (no imports)
///
/// Import counts (how many files import each file):
///   db.ts    -> 2  (main.ts and utils.ts both import it)
///   utils.ts -> 1  (main.ts imports it)
///   main.ts  -> 0  (nothing imports it)
fn write_ts_graph_fixture(dir: &TempDir) {
    let src = dir.path().join("src");
    fs::create_dir_all(&src).expect("create src/");

    fs::write(src.join("db.ts"), "export const db = { connect() {} };\n").expect("write db.ts");

    fs::write(
        src.join("utils.ts"),
        "import { db } from './db';\nexport function query() { return db.connect(); }\n",
    )
    .expect("write utils.ts");

    fs::write(
        src.join("main.ts"),
        "import { query } from './utils';\nimport { db } from './db';\nexport function main() { query(); db.connect(); }\n",
    )
    .expect("write main.ts");
}

// ---------------------------------------------------------------------------
// AC1: api::graph() returns a structured result with a `files` list (sorted
//      by import count descending) and a full edge list.
// ---------------------------------------------------------------------------

#[test]
fn graph_returns_files_sorted_by_import_count_descending() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = TempDir::new().expect("failed to create tempdir");
    write_ts_graph_fixture(&dir);
    api::index(dir.path()).expect("index");

    let result = api::graph(dir.path()).expect("graph should succeed");

    // Must have a `files` field with at least three entries.
    assert!(
        !result.files.is_empty(),
        "graph result must contain at least one file entry; got empty list"
    );

    // The hottest file (db.ts, imported by 2 files) must appear first.
    let first = &result.files[0];
    assert!(
        first.path.contains("db.ts"),
        "db.ts (imported by 2 files) must be the first entry when sorted by count; got: {:?}",
        result.files
    );
    assert_eq!(
        first.import_count, 2,
        "db.ts must have import_count=2; got: {}",
        first.import_count
    );

    // utils.ts must appear second with count=1.
    let second = result
        .files
        .iter()
        .find(|f| f.path.contains("utils.ts"))
        .expect("utils.ts must appear in graph result");
    assert_eq!(
        second.import_count, 1,
        "utils.ts must have import_count=1; got: {}",
        second.import_count
    );

    // main.ts must appear with count=0.
    let main_entry = result
        .files
        .iter()
        .find(|f| f.path.contains("main.ts"))
        .expect("main.ts must appear in graph result");
    assert_eq!(
        main_entry.import_count, 0,
        "main.ts must have import_count=0 (nothing imports it); got: {}",
        main_entry.import_count
    );

    Ok(())
}

#[test]
fn graph_returns_full_edge_list() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_ts_graph_fixture(&dir);
    api::index(dir.path()).expect("index");

    let result = api::graph(dir.path()).expect("graph should succeed");

    // The edge list must include at least the 3 known import edges.
    assert!(
        result.edges.len() >= 3,
        "graph must have at least 3 edges (main->utils, main->db, utils->db); got: {:?}",
        result.edges
    );

    // Each edge has a `from` and `to` field.
    // Verify the main.ts -> db.ts edge exists.
    let main_to_db = result
        .edges
        .iter()
        .any(|e| e.from.contains("main.ts") && e.to.contains("db.ts"));
    assert!(
        main_to_db,
        "edge from main.ts to db.ts must be present; edges: {:?}",
        result.edges
    );

    // Verify the utils.ts -> db.ts edge exists.
    let utils_to_db = result
        .edges
        .iter()
        .any(|e| e.from.contains("utils.ts") && e.to.contains("db.ts"));
    assert!(
        utils_to_db,
        "edge from utils.ts to db.ts must be present; edges: {:?}",
        result.edges
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// AC2: api::graph_file() returns the blast radius (transitively affected files).
// ---------------------------------------------------------------------------

#[test]
fn graph_file_returns_affected_files_for_direct_importers() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = TempDir::new().expect("failed to create tempdir");
    write_ts_graph_fixture(&dir);
    api::index(dir.path()).expect("index");

    // Changing db.ts should affect main.ts and utils.ts (both import it directly).
    let result = api::graph_file(dir.path(), "src/db.ts", 1).expect("graph_file should succeed");

    assert!(
        !result.affected_files.is_empty(),
        "changing db.ts must affect at least one file"
    );
    assert!(
        result.affected_files.iter().any(|f| f.contains("utils.ts")),
        "utils.ts must be in the blast radius of db.ts; got: {:?}",
        result.affected_files
    );
    assert!(
        result.affected_files.iter().any(|f| f.contains("main.ts")),
        "main.ts must be in the blast radius of db.ts (direct import); got: {:?}",
        result.affected_files
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// AC3: BFS follows reverse import edges (if A imports B, changing B affects A).
// ---------------------------------------------------------------------------

#[test]
fn graph_file_blast_radius_follows_reverse_edges() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_ts_graph_fixture(&dir);
    api::index(dir.path()).expect("index");

    // utils.ts imports db.ts. main.ts imports utils.ts.
    // Changing utils.ts should affect main.ts (reverse edge: main -> utils).
    let result = api::graph_file(dir.path(), "src/utils.ts", 3).expect("graph_file should succeed");

    assert!(
        result.affected_files.iter().any(|f| f.contains("main.ts")),
        "main.ts must be in the blast radius of utils.ts (main imports utils); got: {:?}",
        result.affected_files
    );

    // db.ts is NOT affected by changing utils.ts (db doesn't import utils).
    assert!(
        !result.affected_files.iter().any(|f| f.contains("db.ts")),
        "db.ts must NOT be in the blast radius of utils.ts; got: {:?}",
        result.affected_files
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// AC4: max_depth limits the BFS traversal depth.
// ---------------------------------------------------------------------------

#[test]
fn graph_file_respects_max_depth_limit() -> Result<(), Box<dyn std::error::Error>> {
    // Create a deeper chain: a.ts -> b.ts -> c.ts -> d.ts
    let dir = TempDir::new().expect("failed to create tempdir");

    fs::write(dir.path().join("d.ts"), "export const d = 4;\n").expect("write d.ts");
    fs::write(
        dir.path().join("c.ts"),
        "import { d } from './d';\nexport const c = 3;\n",
    )
    .expect("write c.ts");
    fs::write(
        dir.path().join("b.ts"),
        "import { c } from './c';\nexport const b = 2;\n",
    )
    .expect("write b.ts");
    fs::write(
        dir.path().join("a.ts"),
        "import { b } from './b';\nexport const a = 1;\n",
    )
    .expect("write a.ts");

    api::index(dir.path()).expect("index");

    // Changing d.ts with depth=1: only c.ts (direct importer) should be in blast radius.
    let result_depth1 =
        api::graph_file(dir.path(), "d.ts", 1).expect("graph_file depth=1 should succeed");
    assert!(
        result_depth1
            .affected_files
            .iter()
            .any(|f| f.contains("c.ts")),
        "c.ts must be in depth=1 blast radius of d.ts; got: {:?}",
        result_depth1.affected_files
    );
    assert!(
        !result_depth1
            .affected_files
            .iter()
            .any(|f| f.contains("a.ts")),
        "a.ts must NOT be in depth=1 blast radius of d.ts (too deep); got: {:?}",
        result_depth1.affected_files
    );

    // Changing d.ts with depth=3: a.ts, b.ts, c.ts should all be in blast radius.
    let result_depth3 =
        api::graph_file(dir.path(), "d.ts", 3).expect("graph_file depth=3 should succeed");
    assert!(
        result_depth3
            .affected_files
            .iter()
            .any(|f| f.contains("a.ts")),
        "a.ts must be in depth=3 blast radius of d.ts; got: {:?}",
        result_depth3.affected_files
    );

    Ok(())
}

#[test]
fn graph_file_returns_empty_for_file_with_no_importers() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_ts_graph_fixture(&dir);
    api::index(dir.path()).expect("index");

    // main.ts is not imported by anything — blast radius is empty.
    let result = api::graph_file(dir.path(), "src/main.ts", 3).expect("graph_file should succeed");

    assert!(
        result.affected_files.is_empty(),
        "main.ts has no importers, so blast radius must be empty; got: {:?}",
        result.affected_files
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// init command — AC 1-7, 9
// ---------------------------------------------------------------------------

/// AC 1: init detects Claude Code by checking whether ~/.claude/ exists.
/// When ~/.claude/ is present, the function must create/update settings.json
/// (tested separately); here we verify the call itself succeeds.
#[test]
fn init_succeeds_when_claude_dir_exists() -> Result<(), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    fs::create_dir_all(home.path().join(".claude"))?;

    api::init(home.path())?;

    Ok(())
}

/// AC 2: init detects Cursor by checking whether ~/.cursor/ exists.
#[test]
fn init_succeeds_when_cursor_dir_exists() -> Result<(), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    fs::create_dir_all(home.path().join(".cursor"))?;

    api::init(home.path())?;

    Ok(())
}

/// AC 3: When ~/.claude/ exists, init creates ~/.claude/settings.json with
/// the codebones-mcp entry under `mcpServers`.
#[test]
fn init_creates_claude_settings_json_when_missing() -> Result<(), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    fs::create_dir_all(home.path().join(".claude"))?;

    api::init(home.path())?;

    let settings_path = home.path().join(".claude").join("settings.json");
    assert!(
        settings_path.exists(),
        "~/.claude/settings.json must be created; path: {}",
        settings_path.display()
    );

    let content = fs::read_to_string(&settings_path)?;
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("settings.json must contain valid JSON");

    assert!(
        json["mcpServers"]["codebones"]["command"] == "codebones-mcp",
        "mcpServers.codebones.command must equal \"codebones-mcp\"; got: {}",
        json
    );
    assert!(
        json["mcpServers"]["codebones"]["type"] == "stdio",
        "mcpServers.codebones.type must equal \"stdio\"; got: {}",
        json
    );

    Ok(())
}

/// AC 4: If ~/.claude/settings.json already has other MCP servers configured,
/// they are preserved after init runs.
#[test]
fn init_preserves_existing_mcp_servers_in_claude_settings() -> Result<(), Box<dyn std::error::Error>>
{
    let home = TempDir::new()?;
    let claude_dir = home.path().join(".claude");
    fs::create_dir_all(&claude_dir)?;

    // Pre-populate with an existing MCP server
    let existing = serde_json::json!({
        "mcpServers": {
            "other-tool": {
                "command": "other-mcp",
                "args": [],
                "type": "stdio"
            }
        }
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&existing)?,
    )?;

    api::init(home.path())?;

    let content = fs::read_to_string(claude_dir.join("settings.json"))?;
    let json: serde_json::Value = serde_json::from_str(&content)?;

    assert!(
        json["mcpServers"]["other-tool"]["command"] == "other-mcp",
        "pre-existing server 'other-tool' must be preserved; got: {}",
        json
    );
    assert!(
        json["mcpServers"]["codebones"]["command"] == "codebones-mcp",
        "codebones-mcp entry must also be present; got: {}",
        json
    );

    Ok(())
}

/// AC 5: If codebones-mcp is already registered in ~/.claude/settings.json,
/// init does not duplicate the entry.
#[test]
fn init_does_not_duplicate_codebones_entry_in_claude_settings(
) -> Result<(), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    let claude_dir = home.path().join(".claude");
    fs::create_dir_all(&claude_dir)?;

    // Pre-populate with codebones already registered
    let existing = serde_json::json!({
        "mcpServers": {
            "codebones": {
                "command": "codebones-mcp",
                "args": [],
                "type": "stdio"
            }
        }
    });
    fs::write(
        claude_dir.join("settings.json"),
        serde_json::to_string_pretty(&existing)?,
    )?;

    // Run init twice
    api::init(home.path())?;
    api::init(home.path())?;

    let content = fs::read_to_string(claude_dir.join("settings.json"))?;
    let json: serde_json::Value = serde_json::from_str(&content)?;

    // mcpServers must remain a flat object with exactly one "codebones" key
    let mcp_servers = json["mcpServers"]
        .as_object()
        .expect("mcpServers must be an object");
    assert_eq!(
        mcp_servers.keys().filter(|k| *k == "codebones").count(),
        1,
        "codebones must appear exactly once in mcpServers; got: {}",
        json
    );

    Ok(())
}

/// AC 6: When ~/.cursor/ exists, init creates ~/.cursor/mcp.json with the
/// codebones-mcp entry under `mcpServers`.
#[test]
fn init_creates_cursor_mcp_json_when_missing() -> Result<(), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    fs::create_dir_all(home.path().join(".cursor"))?;

    api::init(home.path())?;

    let mcp_path = home.path().join(".cursor").join("mcp.json");
    assert!(
        mcp_path.exists(),
        "~/.cursor/mcp.json must be created; path: {}",
        mcp_path.display()
    );

    let content = fs::read_to_string(&mcp_path)?;
    let json: serde_json::Value =
        serde_json::from_str(&content).expect("mcp.json must contain valid JSON");

    assert!(
        json["mcpServers"]["codebones"]["command"] == "codebones-mcp",
        "mcpServers.codebones.command must equal \"codebones-mcp\"; got: {}",
        json
    );
    assert!(
        json["mcpServers"]["codebones"]["type"] == "stdio",
        "mcpServers.codebones.type must equal \"stdio\"; got: {}",
        json
    );

    Ok(())
}

/// AC 7: Existing MCP servers in ~/.cursor/mcp.json are preserved when init runs.
#[test]
fn init_preserves_existing_mcp_servers_in_cursor_config() -> Result<(), Box<dyn std::error::Error>>
{
    let home = TempDir::new()?;
    let cursor_dir = home.path().join(".cursor");
    fs::create_dir_all(&cursor_dir)?;

    let existing = serde_json::json!({
        "mcpServers": {
            "cursor-tool": {
                "command": "cursor-mcp",
                "args": [],
                "type": "stdio"
            }
        }
    });
    fs::write(
        cursor_dir.join("mcp.json"),
        serde_json::to_string_pretty(&existing)?,
    )?;

    api::init(home.path())?;

    let content = fs::read_to_string(cursor_dir.join("mcp.json"))?;
    let json: serde_json::Value = serde_json::from_str(&content)?;

    assert!(
        json["mcpServers"]["cursor-tool"]["command"] == "cursor-mcp",
        "pre-existing server 'cursor-tool' must be preserved; got: {}",
        json
    );
    assert!(
        json["mcpServers"]["codebones"]["command"] == "codebones-mcp",
        "codebones-mcp entry must also be present; got: {}",
        json
    );

    Ok(())
}

/// AC 9: If no supported AI tools are found (neither ~/.claude/ nor ~/.cursor/
/// exists), init returns Ok and does not create any files.
#[test]
fn init_exits_successfully_when_no_tools_detected() -> Result<(), Box<dyn std::error::Error>> {
    let home = TempDir::new()?;
    // Neither .claude nor .cursor directory is created

    api::init(home.path())?;

    // No config files should have been created
    assert!(
        !home.path().join(".claude").join("settings.json").exists(),
        "settings.json must not be created when .claude/ is absent"
    );
    assert!(
        !home.path().join(".cursor").join("mcp.json").exists(),
        "mcp.json must not be created when .cursor/ is absent"
    );

    Ok(())
}

// ===========================================================================
// Python module-style import resolution — RED (failing) tests
//
// These tests describe DESIRED behavior for Python dotted-module imports.
// They will fail until resolve_import() is taught to convert dotted module
// paths into file paths (e.g., `src.core.event` → `src/core/event.py`).
// ===========================================================================

/// Helper: build the multi-file Python fixture described in the ticket.
///
/// Layout:
///   src/__init__.py            (empty)
///   src/core/__init__.py       (empty)
///   src/core/event.py          (no imports — the shared target)
///   src/core/tracer.py         (from .event import Event  ← relative import)
///   src/agent/__init__.py      (empty)
///   src/agent/base.py          (from src.core.event import Event  ← absolute dotted import)
fn write_python_fixture(dir: &TempDir) {
    let src = dir.path().join("src");
    let core = src.join("core");
    let agent = src.join("agent");
    fs::create_dir_all(&core).expect("create src/core/");
    fs::create_dir_all(&agent).expect("create src/agent/");

    fs::write(src.join("__init__.py"), "").expect("write src/__init__.py");
    fs::write(core.join("__init__.py"), "").expect("write src/core/__init__.py");
    fs::write(agent.join("__init__.py"), "").expect("write src/agent/__init__.py");

    fs::write(core.join("event.py"), "class Event:\n    pass\n").expect("write event.py");

    fs::write(
        core.join("tracer.py"),
        "from .event import Event\n\nclass Tracer:\n    pass\n",
    )
    .expect("write tracer.py");

    fs::write(
        agent.join("base.py"),
        "from src.core.event import Event\n\nclass BaseAgent:\n    pass\n",
    )
    .expect("write base.py");
}

// ---------------------------------------------------------------------------
// AC1 + AC2: Dotted absolute and relative Python imports resolve to file paths
// ---------------------------------------------------------------------------

/// AC1: `from src.core.event import Event` (absolute dotted) should resolve to
/// `src/core/event.py` (dots → slashes, append `.py`).
///
/// AC2: `from .event import Event` (relative, dot-prefixed) in `src/core/tracer.py`
/// should resolve relative to `src/core/` → `src/core/event.py`.
///
/// Verified by checking that `get_importers("src/core/event.py")` returns both
/// `src/core/tracer.py` and `src/agent/base.py`.
#[test]
fn python_dotted_imports_resolve_to_file_paths() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_python_fixture(&dir);

    api::index(dir.path()).expect("index should succeed");

    let importers =
        api::get_importers(dir.path(), "src/core/event.py").expect("get_importers should succeed");

    assert_eq!(
        importers.len(),
        2,
        "both src/core/tracer.py (relative import) and src/agent/base.py (absolute dotted import) \
         should appear as importers of src/core/event.py; got: {:?}",
        importers
    );
    assert!(
        importers.iter().any(|p| p.contains("tracer.py")),
        "src/core/tracer.py must be listed as an importer of event.py \
         (via relative `from .event import Event`); got: {:?}",
        importers
    );
    assert!(
        importers.iter().any(|p| p.contains("base.py")),
        "src/agent/base.py must be listed as an importer of event.py \
         (via absolute `from src.core.event import Event`); got: {:?}",
        importers
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// AC3: Bare stdlib/external imports do NOT resolve to local file paths
// ---------------------------------------------------------------------------

/// AC3: `import os` is a bare Python import with no dots mapping to a local
/// file.  After indexing a file that only contains `import os`, there must be
/// no edge whose target is `os.py` (or any local variant).
#[test]
fn python_bare_stdlib_import_does_not_resolve_to_local_file(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");

    fs::write(
        dir.path().join("main.py"),
        "import os\nimport sys\n\ndef run():\n    print(os.getcwd())\n",
    )
    .expect("write main.py");

    api::index(dir.path()).expect("index should succeed");

    // There is no `os.py` in the fixture, so get_importers should return empty.
    let os_importers = api::get_importers(dir.path(), "os.py")
        .expect("get_importers should not error for a non-existent file");

    assert!(
        os_importers.is_empty(),
        "bare `import os` must not produce a local-file edge to `os.py`; got: {:?}",
        os_importers
    );

    // Also verify the graph does not contain a phantom `os.py` node.
    let graph_result = api::graph(dir.path()).expect("graph should succeed");
    let has_os_node = graph_result.files.iter().any(|f| f.path == "os.py");
    assert!(
        !has_os_node,
        "`os.py` must not appear as a node in the import graph; got: {:?}",
        graph_result.files
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// AC4 + AC5: graph() shows non-zero import counts for Python files
// ---------------------------------------------------------------------------

/// AC4 + AC5: After indexing the multi-file Python fixture, `graph()` must
/// report `event.py` with `import_count >= 2` (it is imported by both
/// `tracer.py` and `base.py`).
#[test]
fn python_graph_shows_nonzero_import_counts() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_python_fixture(&dir);

    api::index(dir.path()).expect("index should succeed");

    let graph_result = api::graph(dir.path()).expect("graph should succeed");

    let event_entry = graph_result
        .files
        .iter()
        .find(|f| f.path.contains("event.py"));

    assert!(
        event_entry.is_some(),
        "src/core/event.py must appear in the graph; got files: {:?}",
        graph_result.files
    );

    let event_count = event_entry.unwrap().import_count;
    assert!(
        event_count >= 2,
        "src/core/event.py should have import_count >= 2 (imported by tracer.py and base.py); \
         got import_count = {}",
        event_count
    );

    // Sanity: tracer.py and base.py should have import_count = 0 (nothing imports them).
    let tracer_entry = graph_result
        .files
        .iter()
        .find(|f| f.path.contains("tracer.py"));
    let base_entry = graph_result
        .files
        .iter()
        .find(|f| f.path.contains("base.py"));

    if let Some(tracer) = tracer_entry {
        assert_eq!(
            tracer.import_count, 0,
            "tracer.py is not imported by any file in the fixture; got import_count = {}",
            tracer.import_count
        );
    }
    if let Some(base) = base_entry {
        assert_eq!(
            base.import_count, 0,
            "base.py is not imported by any file in the fixture; got import_count = {}",
            base.import_count
        );
    }

    Ok(())
}

// ===========================================================================
// Skeleton-budget degradation by import count — failing tests (RED)
//
// Acceptance criteria:
//   AC1: pack --max-tokens N produces output whose token count does not exceed N
//        (within 10% tolerance)
//   AC2: when the skeleton is truncated, high-import-count files survive and
//        low-import-count files are dropped
//   AC3: when the budget is large enough for skeleton + all bodies, no truncation
//   AC5: pack --no-files (skeleton-only / map mode) also respects the budget
// ===========================================================================

/// Write a multi-file TypeScript project with a known import graph.
///
/// Import topology:
///   a.ts  imports hot.ts and warm.ts
///   b.ts  imports hot.ts
///   c.ts  imports hot.ts
///   hot.ts  — no imports (import_count = 3: imported by a, b, c)
///   warm.ts — no imports (import_count = 1: imported by a)
///   cold1.ts — no imports (import_count = 0)
///   cold2.ts — no imports (import_count = 0)
///
/// Each file is padded with enough exported functions that the combined skeleton
/// map exceeds 200 tokens, so a max_tokens=200 budget forces truncation.
fn write_import_budget_fixture(dir: &TempDir) {
    // hot.ts — heavily imported utility; padded so its skeleton chunk is non-trivial
    fs::write(
        dir.path().join("hot.ts"),
        r#"export function hotAlpha(x: number): number { return x * 2; }
export function hotBeta(x: number): number { return x * 3; }
export function hotGamma(x: number): number { return x * 4; }
export function hotDelta(x: number): number { return x * 5; }
export function hotEpsilon(x: number): number { return x * 6; }
"#,
    )
    .expect("write hot.ts");

    // warm.ts — moderately imported; padded similarly
    fs::write(
        dir.path().join("warm.ts"),
        r#"export function warmAlpha(x: number): number { return x + 10; }
export function warmBeta(x: number): number { return x + 20; }
export function warmGamma(x: number): number { return x + 30; }
export function warmDelta(x: number): number { return x + 40; }
export function warmEpsilon(x: number): number { return x + 50; }
"#,
    )
    .expect("write warm.ts");

    // cold1.ts — never imported; padded with content
    fs::write(
        dir.path().join("cold1.ts"),
        r#"export function coldOneAlpha(x: number): number { return x - 1; }
export function coldOneBeta(x: number): number { return x - 2; }
export function coldOneGamma(x: number): number { return x - 3; }
export function coldOneDelta(x: number): number { return x - 4; }
export function coldOneEpsilon(x: number): number { return x - 5; }
"#,
    )
    .expect("write cold1.ts");

    // cold2.ts — never imported; padded with content
    fs::write(
        dir.path().join("cold2.ts"),
        r#"export function coldTwoAlpha(x: number): number { return x / 2; }
export function coldTwoBeta(x: number): number { return x / 3; }
export function coldTwoGamma(x: number): number { return x / 4; }
export function coldTwoDelta(x: number): number { return x / 5; }
export function coldTwoEpsilon(x: number): number { return x / 6; }
"#,
    )
    .expect("write cold2.ts");

    // a.ts — imports both hot and warm (contributes 1 each to their import_count)
    fs::write(
        dir.path().join("a.ts"),
        r#"import { hotAlpha } from './hot';
import { warmAlpha } from './warm';
export function useHotAndWarm(x: number): number { return hotAlpha(x) + warmAlpha(x); }
"#,
    )
    .expect("write a.ts");

    // b.ts — imports only hot (contributes 1 to hot's import_count)
    fs::write(
        dir.path().join("b.ts"),
        r#"import { hotBeta } from './hot';
export function useHot(x: number): number { return hotBeta(x); }
"#,
    )
    .expect("write b.ts");

    // c.ts — imports only hot (contributes 1 to hot's import_count)
    fs::write(
        dir.path().join("c.ts"),
        r#"import { hotGamma } from './hot';
export function alsoUseHot(x: number): number { return hotGamma(x); }
"#,
    )
    .expect("write c.ts");
}

// ---------------------------------------------------------------------------
// AC1: output token count does not exceed max_tokens (within 10% tolerance)
// ---------------------------------------------------------------------------

#[test]
fn pack_with_tight_budget_produces_output_within_token_limit(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_import_budget_fixture(&dir);

    let max_tokens: usize = 200;
    let output = api::pack(dir.path(), "xml", Some(max_tokens), default_pack_options())
        .expect("pack with max_tokens should not error");

    let bpe = tiktoken_rs::cl100k_base().expect("initialize tokenizer");
    let actual_tokens = bpe.encode_with_special_tokens(&output).len();

    // Allow 10% over-budget tolerance (the ticket spec allows "within 10% of N")
    let tolerance = max_tokens + max_tokens / 10;
    assert!(
        actual_tokens <= tolerance,
        "pack output must not exceed {tolerance} tokens (max_tokens={max_tokens} + 10%); \
         got {actual_tokens} tokens.\nOutput:\n{output}"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// AC2: when skeleton is truncated, high-import files survive; low-import dropped
// ---------------------------------------------------------------------------

#[test]
fn pack_skeleton_truncation_keeps_high_import_files_and_drops_cold_files(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_import_budget_fixture(&dir);

    // With a tight budget the skeleton must be truncated. hot.ts (import_count=3)
    // and warm.ts (import_count=1) must survive; cold1.ts and cold2.ts
    // (import_count=0) must be absent.
    let max_tokens: usize = 200;
    let output = api::pack(dir.path(), "xml", Some(max_tokens), default_pack_options())
        .expect("pack with max_tokens should not error");

    assert!(
        output.contains("hot.ts"),
        "hot.ts (import_count=3) must appear in the truncated output; got:\n{output}"
    );
    assert!(
        output.contains("warm.ts"),
        "warm.ts (import_count=1) must appear in the truncated output; got:\n{output}"
    );
    assert!(
        !output.contains("cold1.ts"),
        "cold1.ts (import_count=0) must be dropped when skeleton is truncated; got:\n{output}"
    );
    assert!(
        !output.contains("cold2.ts"),
        "cold2.ts (import_count=0) must be dropped when skeleton is truncated; got:\n{output}"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// AC3: with a generous budget, all files appear — no truncation
// ---------------------------------------------------------------------------

#[test]
fn pack_with_generous_budget_includes_all_files() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_import_budget_fixture(&dir);

    // A very large budget: nothing should be dropped.
    let output = api::pack(dir.path(), "xml", Some(100_000), default_pack_options())
        .expect("pack with large max_tokens should not error");

    for name in &["hot.ts", "warm.ts", "cold1.ts", "cold2.ts"] {
        assert!(
            output.contains(name),
            "with a generous budget, {name} must be present in the output; got:\n{output}"
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// AC5: pack --no-files (map mode) also respects the token budget
// ---------------------------------------------------------------------------

#[test]
fn pack_no_files_skeleton_map_respects_token_budget() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new().expect("failed to create tempdir");
    write_import_budget_fixture(&dir);

    let max_tokens: usize = 200;
    let options = PackOptions {
        no_files: true,
        ..default_pack_options()
    };
    let output = api::pack(dir.path(), "xml", Some(max_tokens), options)
        .expect("pack --no-files with max_tokens should not error");

    // The skeleton map itself must respect the budget.
    let bpe = tiktoken_rs::cl100k_base().expect("initialize tokenizer");
    let actual_tokens = bpe.encode_with_special_tokens(&output).len();
    let tolerance = max_tokens + max_tokens / 10;

    assert!(
        actual_tokens <= tolerance,
        "pack --no-files output must not exceed {tolerance} tokens; got {actual_tokens}.\n\
         Output:\n{output}"
    );

    // hot.ts (highest import count) must be present; cold files must be absent.
    assert!(
        output.contains("hot.ts"),
        "hot.ts must survive skeleton budget truncation in --no-files mode; got:\n{output}"
    );
    assert!(
        !output.contains("cold1.ts"),
        "cold1.ts must be dropped from skeleton map when budget is tight; got:\n{output}"
    );
    assert!(
        !output.contains("cold2.ts"),
        "cold2.ts must be dropped from skeleton map when budget is tight; got:\n{output}"
    );

    Ok(())
}

// ===========================================================================
// Auto-reindex: ensure_fresh() — AC1-5, AC7
//
// These tests FAIL today because search/get/outline/graph don't call
// ensure_fresh() (or index()) before querying. They will pass once
// ensure_fresh() is wired into every read command.
// ===========================================================================

/// AC1: `search` auto-reindexes and returns results for a newly added file
/// without the user running `index` first.
#[test]
fn search_auto_reindexes_newly_added_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;

    // Seed with one file and perform the initial index.
    fs::write(dir.path().join("lib.rs"), "pub fn existing() {}\n")?;
    api::index(dir.path())?;

    // Add a new file AFTER the initial index — no manual re-index.
    fs::write(dir.path().join("new.rs"), "pub fn brand_new() {}\n")?;

    // search must detect the new file and auto-reindex before querying.
    let results = api::search(dir.path(), "brand_new")?;
    assert!(
        !results.is_empty(),
        "search should auto-reindex and find 'brand_new' without a manual index call; got: {:?}",
        results
    );
    Ok(())
}

/// AC2: `get` auto-reindexes and retrieves a newly added symbol without manual `index`.
#[test]
fn get_auto_reindexes_newly_added_symbol() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;

    fs::write(dir.path().join("lib.rs"), "pub fn existing() {}\n")?;
    api::index(dir.path())?;

    // Add a new file with a new symbol AFTER the initial index.
    fs::write(dir.path().join("extra.rs"), "pub fn fresh_symbol() {}\n")?;

    // get must auto-reindex so the symbol is present.
    let content = api::get(dir.path(), "extra.rs::fresh_symbol")?;
    assert!(
        content.contains("fresh_symbol"),
        "get should auto-reindex and return content for 'fresh_symbol'; got: {content}"
    );
    Ok(())
}

/// AC3: `outline` auto-reindexes and works on a newly added file without manual `index`.
#[test]
fn outline_auto_reindexes_newly_added_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;

    fs::write(dir.path().join("lib.rs"), "pub fn existing() {}\n")?;
    api::index(dir.path())?;

    // Add a new file AFTER the initial index.
    fs::write(
        dir.path().join("new_module.rs"),
        "pub fn outlined_fn() { let x = 1; }\n",
    )?;

    // outline must auto-reindex and succeed on the new file.
    let result = api::outline(dir.path(), "new_module.rs");
    assert!(
        result.is_ok(),
        "outline should auto-reindex and not error on a newly added file; got: {:?}",
        result.err()
    );
    let text = result.unwrap();
    assert!(
        text.contains("outlined_fn"),
        "outline output should contain the function name 'outlined_fn'; got: {text}"
    );
    Ok(())
}

/// AC4: `graph` auto-reindexes and includes a newly added file in the graph
/// without manual `index`.
#[test]
fn graph_auto_reindexes_newly_added_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;

    fs::write(dir.path().join("lib.rs"), "pub fn existing() {}\n")?;
    api::index(dir.path())?;

    // Add a new file AFTER the initial index.
    fs::write(dir.path().join("graph_new.rs"), "pub fn graph_fn() {}\n")?;

    // graph must auto-reindex so the new file appears in the result.
    let result = api::graph(dir.path())?;
    let file_paths: Vec<&str> = result.files.iter().map(|f| f.path.as_str()).collect();
    assert!(
        file_paths.iter().any(|p| p.contains("graph_new.rs")),
        "graph should auto-reindex and include 'graph_new.rs'; files: {:?}",
        file_paths
    );
    Ok(())
}

/// AC5a: `pack` still works correctly after the ensure_fresh migration.
#[test]
fn pack_works_correctly_via_ensure_fresh() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;

    fs::write(dir.path().join("lib.rs"), "pub fn pack_me() {}\n")?;
    // No manual index — pack must index internally via ensure_fresh.
    let output = api::pack(dir.path(), "markdown", None, default_pack_options())?;
    assert!(
        output.contains("pack_me"),
        "pack output should contain 'pack_me'; got: {output}"
    );
    Ok(())
}

/// AC5b: `pack` picks up a newly added file without manual re-index.
#[test]
fn pack_picks_up_newly_added_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;

    fs::write(dir.path().join("lib.rs"), "pub fn first() {}\n")?;
    api::index(dir.path())?;

    // Add a new file after the initial index.
    fs::write(dir.path().join("second.rs"), "pub fn second_fn() {}\n")?;

    // pack must pick it up without a manual re-index call.
    let output = api::pack(dir.path(), "markdown", None, default_pack_options())?;
    assert!(
        output.contains("second_fn"),
        "pack should auto-reindex and include 'second_fn'; got: {output}"
    );
    Ok(())
}

/// AC7: When files change after the initial index, the very next read command
/// picks up those changes automatically (not just pack).
#[test]
fn search_picks_up_changes_after_initial_index() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;

    // Create and index a file.
    fs::write(dir.path().join("lib.rs"), "pub fn original() {}\n")?;
    api::index(dir.path())?;

    // Modify the existing file to add a new symbol — no manual re-index.
    fs::write(
        dir.path().join("lib.rs"),
        "pub fn original() {}\npub fn modified_symbol() {}\n",
    )?;

    // search must pick up the modified file.
    let results = api::search(dir.path(), "modified_symbol")?;
    assert!(
        !results.is_empty(),
        "search should detect the changed file and find 'modified_symbol' without manual re-index; got: {:?}",
        results
    );
    Ok(())
}

/// AC7 (get variant): `get` picks up a symbol added to an already-indexed file.
#[test]
fn get_picks_up_changes_after_initial_index() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;

    fs::write(dir.path().join("lib.rs"), "pub fn original() {}\n")?;
    api::index(dir.path())?;

    // Add a new symbol to the existing file — no manual re-index.
    fs::write(
        dir.path().join("lib.rs"),
        "pub fn original() {}\npub fn added_later() {}\n",
    )?;

    let content = api::get(dir.path(), "lib.rs::added_later")?;
    assert!(
        content.contains("added_later"),
        "get should detect the changed file and return 'added_later' without manual re-index; got: {content}"
    );
    Ok(())
}

/// AC1 (no prior index): `search` works even when `.codebones/codebones.db`
/// does not exist yet — ensure_fresh must run a full index in this case.
#[test]
fn search_works_with_no_prior_index() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    fs::write(dir.path().join("lib.rs"), "pub fn never_indexed() {}\n")?;

    // No index call — the db doesn't exist yet.
    assert!(
        !dir.path().join(".codebones").join("codebones.db").exists(),
        "pre-condition: db must not exist before this test"
    );

    let results = api::search(dir.path(), "never_indexed")?;
    assert!(
        !results.is_empty(),
        "search should auto-index from scratch when no db exists; got: {:?}",
        results
    );
    Ok(())
}

// ===========================================================================
// Git-based fast path: ensure_fresh() — AC1-6
//
// These tests FAIL today because ensure_fresh() does not exist yet. They will
// pass once ensure_fresh() is implemented and wired into the read commands.
//
// AC7 (read commands use ensure_fresh instead of raw index) is already covered
// by the auto-reindex tests above — not duplicated here.
// ===========================================================================

use std::process::Command as StdCommand;

/// Sets up a minimal git repo in `dir` with all current files committed.
fn init_git_repo(dir: &std::path::Path) {
    StdCommand::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(dir)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["config", "user.name", "test"])
        .current_dir(dir)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output()
        .unwrap();
    StdCommand::new("git")
        .args(["-c", "commit.gpgsign=false", "commit", "-m", "init"])
        .current_dir(dir)
        .output()
        .unwrap();
}

/// Returns the current HEAD commit hash for `dir` (trimmed).
fn git_head(dir: &std::path::Path) -> String {
    let out = StdCommand::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("git rev-parse HEAD must succeed");
    String::from_utf8(out.stdout)
        .expect("HEAD hash must be valid UTF-8")
        .trim()
        .to_string()
}

/// AC6: After `index()` completes on a git repo, `.codebones/last_commit`
/// exists and its contents equal the current HEAD commit hash.
#[test]
fn index_writes_last_commit_file_in_git_repo() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    init_git_repo(dir.path());

    api::index(dir.path())?;

    let last_commit_path = dir.path().join(".codebones").join("last_commit");
    assert!(
        last_commit_path.exists(),
        ".codebones/last_commit must exist after index() on a git repo"
    );

    let stored = fs::read_to_string(&last_commit_path)?.trim().to_string();
    let head = git_head(dir.path());

    assert_eq!(
        stored, head,
        ".codebones/last_commit must contain the current HEAD hash; stored={stored}, HEAD={head}"
    );
    Ok(())
}

/// AC6 (no git): When there is no `.git/` directory, `index()` must NOT
/// create `.codebones/last_commit` (nothing to record).
#[test]
fn index_does_not_write_last_commit_file_without_git() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    // No git init — plain directory.

    api::index(dir.path())?;

    let last_commit_path = dir.path().join(".codebones").join("last_commit");
    assert!(
        !last_commit_path.exists(),
        ".codebones/last_commit must NOT be created when there is no .git/ directory"
    );
    Ok(())
}

/// AC1: When the git repo is clean and HEAD has not changed since the last
/// index, `ensure_fresh()` skips re-indexing. Verified by checking that
/// `.codebones/last_commit` exists and still matches HEAD after calling a
/// read command (search) without any file changes.
#[test]
fn ensure_fresh_skips_indexing_when_git_is_clean_and_head_unchanged(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    init_git_repo(dir.path());

    // First full index — populates DB and writes last_commit.
    api::index(dir.path())?;

    let head_before = git_head(dir.path());
    let last_commit_before = fs::read_to_string(dir.path().join(".codebones").join("last_commit"))?
        .trim()
        .to_string();
    assert_eq!(
        last_commit_before, head_before,
        "pre-condition: last_commit must equal HEAD before the skip test"
    );

    // Call a read command — no files changed, repo is clean, HEAD unchanged.
    // ensure_fresh must detect the fast-path condition and skip indexing.
    let results = api::search(dir.path(), "add")?;
    assert!(
        !results.is_empty(),
        "search must still return results when indexing is skipped; got: {:?}",
        results
    );

    // The last_commit file must still match HEAD (no rewrite from a spurious re-index).
    let last_commit_after = fs::read_to_string(dir.path().join(".codebones").join("last_commit"))?
        .trim()
        .to_string();
    assert_eq!(
        last_commit_after, head_before,
        "last_commit must remain equal to HEAD after a clean-repo skip; \
         stored={last_commit_after}, HEAD={head_before}"
    );
    Ok(())
}

/// AC2: When a file has been modified after the last index (dirty working
/// tree), `ensure_fresh()` must run a full re-index so that `search()` finds
/// the new content.
#[test]
fn ensure_fresh_reindexes_when_working_tree_is_dirty() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    init_git_repo(dir.path());

    // Initial index — sets up DB and last_commit.
    api::index(dir.path())?;

    // Modify a tracked file without committing — dirty working tree.
    let modified = format!("{}\npub fn dirty_fn() {{}}", RUST_FIXTURE);
    fs::write(dir.path().join("lib.rs"), &modified)?;

    // search must re-index and find the new symbol.
    let results = api::search(dir.path(), "dirty_fn")?;
    assert!(
        !results.is_empty(),
        "search must re-index when working tree is dirty and find 'dirty_fn'; got: {:?}",
        results
    );
    Ok(())
}

/// AC3: When HEAD advances (a new commit is made) after the last index,
/// `ensure_fresh()` must re-index so the next read command sees the new
/// content.
#[test]
fn ensure_fresh_reindexes_when_head_changes() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    init_git_repo(dir.path());

    // Initial index.
    api::index(dir.path())?;

    let head_before = git_head(dir.path());

    // Add a new file and make a second commit — HEAD advances.
    fs::write(
        dir.path().join("new_commit.rs"),
        "pub fn committed_later() {}\n",
    )?;
    StdCommand::new("git")
        .args(["add", "new_commit.rs"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    StdCommand::new("git")
        .args([
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "second commit",
        ])
        .current_dir(dir.path())
        .output()
        .unwrap();

    let head_after = git_head(dir.path());
    assert_ne!(
        head_before, head_after,
        "pre-condition: HEAD must have advanced after the second commit"
    );

    // search must re-index because HEAD changed and find the new symbol.
    let results = api::search(dir.path(), "committed_later")?;
    assert!(
        !results.is_empty(),
        "search must re-index when HEAD changes and find 'committed_later'; got: {:?}",
        results
    );

    // last_commit must have been updated to the new HEAD.
    let stored = fs::read_to_string(dir.path().join(".codebones").join("last_commit"))?
        .trim()
        .to_string();
    assert_eq!(
        stored, head_after,
        "last_commit must be updated to the new HEAD after re-indexing; \
         stored={stored}, new_HEAD={head_after}"
    );
    Ok(())
}

/// AC4: When `.git/` does NOT exist (plain directory, not a git repo),
/// `ensure_fresh()` must always run a full index so that read commands
/// reflect the current state of the filesystem.
#[test]
fn ensure_fresh_always_reindexes_in_non_git_directory() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    // No git init — plain directory.
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);

    // Initial index.
    api::index(dir.path())?;

    // Add a new file — no git, no commit.
    fs::write(dir.path().join("plain.rs"), "pub fn plain_fn() {}\n")?;

    // search must re-index (no fast-path possible without git) and find the new file.
    let results = api::search(dir.path(), "plain_fn")?;
    assert!(
        !results.is_empty(),
        "search must re-index in a non-git directory and find 'plain_fn'; got: {:?}",
        results
    );
    Ok(())
}

/// AC5: When `.codebones/codebones.db` does not exist (first run),
/// `ensure_fresh()` must run a full index regardless of git state.
#[test]
fn ensure_fresh_runs_full_index_when_db_missing() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    write_rust_fixture(&dir, "lib.rs", RUST_FIXTURE);
    init_git_repo(dir.path());

    // Explicitly confirm no DB exists yet.
    assert!(
        !dir.path().join(".codebones").join("codebones.db").exists(),
        "pre-condition: codebones.db must not exist before first search"
    );

    // Calling search with no prior index must trigger a full index from scratch.
    let results = api::search(dir.path(), "add")?;
    assert!(
        !results.is_empty(),
        "search must run a full index when db is missing and find 'add'; got: {:?}",
        results
    );

    // DB must now exist.
    assert!(
        dir.path().join(".codebones").join("codebones.db").exists(),
        ".codebones/codebones.db must be created after the first-run index"
    );
    Ok(())
}
