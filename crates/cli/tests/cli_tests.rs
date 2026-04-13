use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

// A helper function to create a dummy repository for testing
fn setup_dummy_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    // Create a dummy rust file
    let rs_content = r#"
// A single line comment
/* A block comment */
pub fn hello_world() {
    println!("Hello");
}



pub struct DummyStruct;
"#;
    fs::write(root.join("dummy.rs"), rs_content).unwrap();

    // Create a dummy toml file
    fs::write(root.join("dummy.toml"), "[package]\nname = \"dummy\"").unwrap();

    // Create a dummy base64 file
    let long_b64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    fs::write(
        root.join("base64.txt"),
        format!("let b64 = \"{}\";", long_b64),
    )
    .unwrap();

    temp
}

#[test]
fn test_index_and_search() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    // Index
    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    // Search
    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["search", "hello_world"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello_world"));
}

#[test]
fn test_get_and_outline() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    // Outline
    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["outline", "dummy.rs"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("pub fn hello_world()")
                // Body elision must be present: the function body should be replaced with ...
                .and(predicate::str::contains("...")),
        );

    // Get file
    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["get", "dummy.rs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("println!"));
}

#[test]
fn test_pack_base_xml() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let output = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["pack", ".", "--format", "xml"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("<repository>")
                .and(predicate::str::contains("</repository>"))
                .and(predicate::str::contains("<skeleton_map>"))
                .and(predicate::str::contains(
                    "<signature>Function hello_world</signature>",
                ))
                .and(predicate::str::contains("<![CDATA[")),
        )
        .get_output()
        .stdout
        .clone();

    let xml_str = String::from_utf8_lossy(&output);

    // Structural XML validation: output must start with <repository> and end with </repository>
    let trimmed = xml_str.trim();
    assert!(
        trimmed.starts_with("<repository>"),
        "XML output must start with <repository>, got: {}",
        &trimmed[..trimmed.len().min(80)]
    );
    assert!(
        trimmed.ends_with("</repository>"),
        "XML output must end with </repository>, got: ...{}",
        &trimmed[trimmed.len().saturating_sub(80)..]
    );

    // Every opening <file tag must have a corresponding </file> closing tag
    let open_file_tags = xml_str.matches("<file ").count();
    let close_file_tags = xml_str.matches("</file>").count();
    assert_eq!(
        open_file_tags, close_file_tags,
        "Every <file ...> tag must have a matching </file> ({} open vs {} close)",
        open_file_tags, close_file_tags
    );
}

#[test]
fn test_pack_markdown() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["pack", ".", "--format", "markdown"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("## Skeleton Map")
                .and(predicate::str::contains("- ./dummy.rs"))
                .and(predicate::str::contains("  - Function hello_world")),
        );
}

#[test]
fn test_pack_flags_no_file_summary() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["pack", ".", "--format", "xml", "--no-file-summary"])
        .assert()
        .success()
        .stdout(predicate::str::contains("<skeleton_map>").not());
}

#[test]
fn test_pack_flags_no_files() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["pack", ".", "--format", "xml", "--no-files"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("<skeleton_map>")
                .and(predicate::str::contains("<content>").not())
                .and(predicate::str::contains("</repository>")),
        );
}

#[test]
fn test_pack_flags_remove_comments() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["pack", "dummy.rs", "--format", "xml", "--remove-comments"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("A single line comment")
                .not()
                .and(predicate::str::contains("A block comment").not())
                // The exact comment string from the fixture must not appear after stripping
                .and(predicate::str::contains("// A single line comment").not()),
        );
}

#[test]
fn test_pack_flags_remove_empty_lines() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args([
            "pack",
            "dummy.rs",
            "--format",
            "xml",
            "--remove-empty-lines",
        ])
        .assert()
        .success()
        .stdout(
            // Multiple consecutive newlines should be collapsed
            predicate::str::contains("\n\n\n").not(),
        );
}

#[test]
fn test_pack_flags_truncate_base64() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["pack", ".", "--format", "xml", "--truncate-base64"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("[TRUNCATED_BASE64]")
                .and(predicate::str::contains("ABCDEFGHIJKLMNOPQRSTUVWXYZ").not()),
        );
}

#[test]
fn test_pack_flags_include_ignore() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    // Test include
    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["pack", ".", "--format", "xml", "--include", "**/*.toml"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("dummy.toml").and(predicate::str::contains("dummy.rs").not()),
        );

    // Test ignore
    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["pack", ".", "--format", "xml", "--ignore", "**/*.toml"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("dummy.rs").and(predicate::str::contains("dummy.toml").not()),
        );
}

#[test]
fn test_pack_multiple_files() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    // setup_dummy_repo creates dummy.rs AND dummy.toml; both must appear in the skeleton map
    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["pack", ".", "--format", "xml"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dummy.rs").and(predicate::str::contains("dummy.toml")));
}

#[test]
fn test_index_creates_db() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    // The DB must not exist before indexing
    assert!(
        !root.join(".codebones").join("codebones.db").exists(),
        ".codebones/codebones.db should not exist before indexing"
    );

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    // After indexing the DB file must be present
    assert!(
        root.join(".codebones").join("codebones.db").exists(),
        ".codebones/codebones.db must be created after running 'codebones index'"
    );
}

#[test]
fn test_search_can_target_indexed_repo_outside_cwd() {
    let temp = setup_dummy_repo();
    let root = temp.path();
    let outside = TempDir::new().unwrap();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(outside.path())
        .args(["index", root.to_str().unwrap()])
        .assert()
        .success();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(outside.path())
        .args(["search", "--dir", root.to_str().unwrap(), "hello_world"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello_world"));
}

#[test]
fn test_get_can_target_indexed_repo_outside_cwd() {
    let temp = setup_dummy_repo();
    let root = temp.path();
    let outside = TempDir::new().unwrap();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(outside.path())
        .args(["index", root.to_str().unwrap()])
        .assert()
        .success();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(outside.path())
        .args(["get", "--dir", root.to_str().unwrap(), "dummy.rs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("println!(\"Hello\")"));
}

#[test]
fn test_outline_can_target_indexed_repo_outside_cwd() {
    let temp = setup_dummy_repo();
    let root = temp.path();
    let outside = TempDir::new().unwrap();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(outside.path())
        .args(["index", root.to_str().unwrap()])
        .assert()
        .success();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(outside.path())
        .args(["outline", "--dir", root.to_str().unwrap(), "dummy.rs"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("pub fn hello_world()").and(predicate::str::contains("...")),
        );
}

#[test]
fn test_pack_rejects_invalid_format() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["pack", ".", "--format", "json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Invalid output format"));
}

// ---------------------------------------------------------------------------
// map command tests (AC1–AC7)
// ---------------------------------------------------------------------------

/// AC1: `codebones map` outputs the same result as `codebones pack --no-files`
/// (defaults to current directory, xml format)
#[test]
fn test_map_default_equals_pack_no_files() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let map_out = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["map"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let pack_out = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["pack", ".", "--no-files"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(
        map_out, pack_out,
        "`codebones map` must produce identical output to `codebones pack --no-files`"
    );
}

/// AC2: `codebones map .` is equivalent to `codebones map` (explicit dir)
#[test]
fn test_map_explicit_dot_equals_implicit_default() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let implicit_out = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["map"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let explicit_out = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["map", "."])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert_eq!(
        implicit_out, explicit_out,
        "`codebones map .` must produce identical output to `codebones map`"
    );
}

/// AC3: `codebones map --format markdown` produces markdown skeleton output
#[test]
fn test_map_format_markdown() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["map", "--format", "markdown"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("## Skeleton Map")
                .and(predicate::str::contains("- ./dummy.rs")),
        );
}

/// AC4: `codebones map --format xml` produces xml skeleton output
#[test]
fn test_map_format_xml() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["map", "--format", "xml"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("<skeleton_map>")
                .and(predicate::str::contains("</repository>")),
        );
}

/// AC5: `codebones map --max-tokens N` respects the token budget
#[test]
fn test_map_max_tokens_respected() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    // With a very small budget the output should be truncated / shorter than without a budget.
    let small_out = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["map", "--max-tokens", "5"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let full_out = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["map"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    assert!(
        small_out.len() <= full_out.len(),
        "`--max-tokens 5` output ({} bytes) must not exceed unrestricted output ({} bytes)",
        small_out.len(),
        full_out.len()
    );
}

/// AC6: `codebones map` output contains skeleton_map but NOT file content blocks
#[test]
fn test_map_contains_skeleton_map_not_content() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["map"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("<skeleton_map>")
                .and(predicate::str::contains("<content>").not()),
        );
}

/// AC7: `codebones map` passes through include glob option to pack
#[test]
fn test_map_include_glob_passthrough() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["map", "--include", "**/*.toml"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("dummy.toml").and(predicate::str::contains("dummy.rs").not()),
        );
}

/// AC7: `codebones map` passes through ignore glob option to pack
#[test]
fn test_map_ignore_glob_passthrough() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["map", "--ignore", "**/*.toml"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("dummy.rs").and(predicate::str::contains("dummy.toml").not()),
        );
}

// ===========================================================================
// AC: .codebones/ migration and first-run setup — CLI-level failing tests
// (RED — these tests must fail until the implementation is updated)
// ===========================================================================

/// AC1: `codebones index .` creates the database at `.codebones/codebones.db`
/// and NOT at the project root.
#[test]
fn test_index_creates_db_at_dot_codebones_path() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    assert!(
        root.join(".codebones").join("codebones.db").exists(),
        ".codebones/codebones.db must be created after 'codebones index .'"
    );
    assert!(
        !root.join("codebones.db").exists(),
        "codebones.db must NOT be created at the project root"
    );
}

/// AC4: `index` creates the `.codebones/` directory automatically when it
/// does not already exist.
#[test]
fn test_index_auto_creates_dot_codebones_directory() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    assert!(
        !root.join(".codebones").exists(),
        "pre-condition: .codebones/ must not exist before first run"
    );

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    assert!(
        root.join(".codebones").is_dir(),
        ".codebones/ directory must be created automatically by 'codebones index'"
    );
}

/// AC2: `search` succeeds after `index` writes to `.codebones/codebones.db`.
#[test]
fn test_search_uses_db_in_dot_codebones() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    // Pre-condition: db is now in .codebones/
    assert!(
        root.join(".codebones").join("codebones.db").exists(),
        "pre-condition: .codebones/codebones.db must exist after index"
    );

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["search", "hello_world"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello_world"));
}

/// AC3: If a legacy `codebones.db` exists at the project root, `index`
/// deletes it and creates a fresh database at `.codebones/codebones.db`.
#[test]
fn test_index_removes_legacy_root_db() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    // Plant a legacy db at root.
    let legacy_db = root.join("codebones.db");
    fs::write(&legacy_db, b"legacy data").expect("write legacy db");
    assert!(legacy_db.exists(), "pre-condition: legacy db must exist");

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    assert!(
        !legacy_db.exists(),
        "legacy codebones.db at root must be deleted by 'codebones index'"
    );
    assert!(
        root.join(".codebones").join("codebones.db").exists(),
        ".codebones/codebones.db must be created after legacy db removal"
    );
}

/// AC5 + AC6: When `.git/` exists and `.gitignore` is absent, `index`
/// creates `.gitignore` containing `.codebones/`.
#[test]
fn test_index_creates_gitignore_in_git_repo() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    fs::create_dir(root.join(".git")).expect("create .git dir");
    assert!(
        !root.join(".gitignore").exists(),
        "pre-condition: .gitignore must not exist"
    );

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    let gitignore_path = root.join(".gitignore");
    assert!(
        gitignore_path.exists(),
        ".gitignore must be created when .git/ exists"
    );
    let contents = fs::read_to_string(&gitignore_path).expect("read .gitignore");
    assert!(
        contents.contains(".codebones/"),
        ".gitignore must contain '.codebones/' entry; got: {contents}"
    );
}

/// AC5: When `.gitignore` already exists but lacks `.codebones/`, `index`
/// appends the entry without destroying existing content.
#[test]
fn test_index_appends_dot_codebones_to_existing_gitignore() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    fs::create_dir(root.join(".git")).expect("create .git dir");
    fs::write(root.join(".gitignore"), "target/\n*.log\n").expect("write .gitignore");

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    let contents = fs::read_to_string(root.join(".gitignore")).expect("read .gitignore");
    assert!(
        contents.contains(".codebones/"),
        ".gitignore must contain '.codebones/' after index; got: {contents}"
    );
    assert!(
        contents.contains("target/"),
        "original .gitignore content must be preserved; got: {contents}"
    );
}

/// AC7: When `.gitignore` already contains `.codebones/`, `index` does NOT
/// add a duplicate entry.
#[test]
fn test_index_no_duplicate_gitignore_entry() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    fs::create_dir(root.join(".git")).expect("create .git dir");
    fs::write(root.join(".gitignore"), ".codebones/\ntarget/\n").expect("write .gitignore");

    // Run twice to stress idempotency.
    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();
    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    let contents = fs::read_to_string(root.join(".gitignore")).expect("read .gitignore");
    let occurrences = contents.matches(".codebones/").count();
    assert_eq!(
        occurrences, 1,
        ".codebones/ must appear exactly once in .gitignore; got {occurrences}"
    );
}

/// AC8: When there is NO `.git/` directory, `index` must NOT create or touch
/// `.gitignore`.
#[test]
fn test_index_does_not_create_gitignore_without_git_dir() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    assert!(
        !root.join(".git").exists(),
        "pre-condition: .git must not exist"
    );

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    assert!(
        !root.join(".gitignore").exists(),
        ".gitignore must NOT be created when there is no .git/ directory"
    );
}

/// AC9: When `CLAUDE.md` exists, `index` appends a codebones section on
/// first run.
#[test]
fn test_index_appends_to_claude_md() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let claude_md = root.join("CLAUDE.md");
    fs::write(&claude_md, "# My Project\n\nExisting content.\n").expect("write CLAUDE.md");

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    let contents = fs::read_to_string(&claude_md).expect("read CLAUDE.md");
    assert!(
        contents.contains("codebones"),
        "CLAUDE.md must contain a codebones section after index; got: {contents}"
    );
    assert!(
        contents.contains("My Project"),
        "original CLAUDE.md content must be preserved; got: {contents}"
    );
}

/// AC9 (idempotency): `index` does NOT append a duplicate codebones section
/// to `CLAUDE.md` on repeated runs.
#[test]
fn test_index_no_duplicate_claude_md_section() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    fs::write(root.join("CLAUDE.md"), "# My Project\n").expect("write CLAUDE.md");

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();
    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    let contents = fs::read_to_string(root.join("CLAUDE.md")).expect("read CLAUDE.md");
    let section_starts = contents.matches("## codebones").count()
        + contents.matches("## Codebones").count()
        + contents.matches("<!-- codebones -->").count();
    assert!(
        section_starts <= 1,
        "codebones section must appear at most once in CLAUDE.md; found {section_starts} times"
    );
}

/// AC10: When `AGENTS.md` exists, `index` appends a codebones section on
/// first run.
#[test]
fn test_index_appends_to_agents_md() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let agents_md = root.join("AGENTS.md");
    fs::write(&agents_md, "# Agents\n\nExisting content.\n").expect("write AGENTS.md");

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    let contents = fs::read_to_string(&agents_md).expect("read AGENTS.md");
    assert!(
        contents.contains("codebones"),
        "AGENTS.md must contain a codebones section after index; got: {contents}"
    );
    assert!(
        contents.contains("Agents"),
        "original AGENTS.md content must be preserved; got: {contents}"
    );
}

/// AC10 (idempotency): `index` does NOT duplicate the codebones section in
/// `AGENTS.md` on repeated runs.
#[test]
fn test_index_no_duplicate_agents_md_section() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    fs::write(root.join("AGENTS.md"), "# Agents\n").expect("write AGENTS.md");

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();
    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    let contents = fs::read_to_string(root.join("AGENTS.md")).expect("read AGENTS.md");
    let section_starts = contents.matches("## codebones").count()
        + contents.matches("## Codebones").count()
        + contents.matches("<!-- codebones -->").count();
    assert!(
        section_starts <= 1,
        "codebones section must appear at most once in AGENTS.md; found {section_starts} times"
    );
}

/// AC11: When neither `CLAUDE.md` nor `AGENTS.md` exists, `index` must NOT
/// create either file.
#[test]
fn test_index_does_not_create_claude_or_agents_md() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    assert!(
        !root.join("CLAUDE.md").exists(),
        "pre-condition: CLAUDE.md must not exist"
    );
    assert!(
        !root.join("AGENTS.md").exists(),
        "pre-condition: AGENTS.md must not exist"
    );

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    assert!(
        !root.join("CLAUDE.md").exists(),
        "index must NOT create CLAUDE.md when it does not already exist"
    );
    assert!(
        !root.join("AGENTS.md").exists(),
        "index must NOT create AGENTS.md when it does not already exist"
    );
}

// ===========================================================================
// graph command tests (AC 5–10) — failing tests (RED)
//
// These tests express the CLI acceptance criteria for `codebones graph`.
// They will fail until the implementation is added.
// ===========================================================================

/// Helper: write a small TypeScript project with a known import graph.
///
///   src/main.ts   imports ./utils and ./db
///   src/utils.ts  imports ./db
///   src/db.ts     (no imports)
///
/// Import counts:
///   db.ts    -> 2 (hottest)
///   utils.ts -> 1
///   main.ts  -> 0
fn setup_graph_repo() -> TempDir {
    let temp = TempDir::new().unwrap();
    let src = temp.path().join("src");
    fs::create_dir_all(&src).unwrap();

    fs::write(src.join("db.ts"), "export const db = { connect() {} };\n").unwrap();
    fs::write(
        src.join("utils.ts"),
        "import { db } from './db';\nexport function query() { return db.connect(); }\n",
    )
    .unwrap();
    fs::write(
        src.join("main.ts"),
        "import { query } from './utils';\nimport { db } from './db';\nexport function main() { query(); db.connect(); }\n",
    )
    .unwrap();

    // Index the repo so the graph command has data to work with.
    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(temp.path())
        .args(["index", "."])
        .assert()
        .success();

    temp
}

/// AC5: `codebones graph` outputs the full import graph in markdown (default format).
/// Output must contain file names and their import counts.
#[test]
fn test_graph_default_outputs_markdown_with_file_and_counts() {
    let temp = setup_graph_repo();
    let root = temp.path();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["graph"])
        .assert()
        .success()
        .stdout(
            // Must mention db.ts (the hottest file) and its count
            predicate::str::contains("db.ts")
                .and(predicate::str::contains("2"))
                // Must also mention utils.ts
                .and(predicate::str::contains("utils.ts"))
                // Must mention main.ts
                .and(predicate::str::contains("main.ts")),
        );
}

/// AC6: `codebones graph <file>` outputs the blast radius for that file.
#[test]
fn test_graph_file_outputs_blast_radius() {
    let temp = setup_graph_repo();
    let root = temp.path();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["graph", "src/db.ts"])
        .assert()
        .success()
        .stdout(
            // Both main.ts and utils.ts import db.ts transitively
            predicate::str::contains("utils.ts").and(predicate::str::contains("main.ts")),
        );
}

/// AC7: `codebones graph --format json` outputs JSON format.
#[test]
fn test_graph_format_json() {
    let temp = setup_graph_repo();
    let root = temp.path();

    let output = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["graph", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    // JSON output must start with '{' or '[' and contain the file names.
    assert!(
        stdout.trim_start().starts_with('{') || stdout.trim_start().starts_with('['),
        "graph --format json must produce JSON output; got: {}",
        stdout
    );
    assert!(
        stdout.contains("db.ts"),
        "JSON graph output must contain db.ts; got: {}",
        stdout
    );
}

/// AC8: `codebones graph --format xml` outputs XML format.
#[test]
fn test_graph_format_xml() {
    let temp = setup_graph_repo();
    let root = temp.path();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["graph", "--format", "xml"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("<")
                .and(predicate::str::contains(">"))
                .and(predicate::str::contains("db.ts")),
        );
}

/// AC9: `codebones graph --top 1` shows only the single most-imported file.
#[test]
fn test_graph_top_n_limits_output_to_n_hottest_files() {
    let temp = setup_graph_repo();
    let root = temp.path();

    let output = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["graph", "--top", "1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    // With --top 1, only db.ts (count=2) should appear.
    assert!(
        stdout.contains("db.ts"),
        "--top 1 must include db.ts (hottest file); got: {}",
        stdout
    );
    // utils.ts (count=1) must NOT appear when top=1.
    assert!(
        !stdout.contains("utils.ts"),
        "--top 1 must not include utils.ts (second hottest); got: {}",
        stdout
    );
}

/// AC10: `codebones graph <file> --depth 2` limits the blast radius BFS depth.
#[test]
fn test_graph_file_depth_flag_limits_blast_radius() {
    // Chain: a.ts -> b.ts -> c.ts -> d.ts
    let temp = TempDir::new().unwrap();
    let root = temp.path();

    fs::write(root.join("d.ts"), "export const d = 4;\n").unwrap();
    fs::write(
        root.join("c.ts"),
        "import { d } from './d';\nexport const c = 3;\n",
    )
    .unwrap();
    fs::write(
        root.join("b.ts"),
        "import { c } from './c';\nexport const b = 2;\n",
    )
    .unwrap();
    fs::write(
        root.join("a.ts"),
        "import { b } from './b';\nexport const a = 1;\n",
    )
    .unwrap();

    // Index first.
    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["index", "."])
        .assert()
        .success();

    // With --depth 1, only c.ts (direct importer of d.ts) should appear.
    let output_depth1 = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["graph", "d.ts", "--depth", "1"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout1 = String::from_utf8_lossy(&output_depth1);
    assert!(
        stdout1.contains("c.ts"),
        "--depth 1 must include c.ts (direct importer); got: {}",
        stdout1
    );
    assert!(
        !stdout1.contains("a.ts"),
        "--depth 1 must NOT include a.ts (too deep); got: {}",
        stdout1
    );

    // With --depth 3, all of a.ts, b.ts, c.ts should appear.
    let output_depth3 = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(root)
        .args(["graph", "d.ts", "--depth", "3"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout3 = String::from_utf8_lossy(&output_depth3);
    assert!(
        stdout3.contains("a.ts"),
        "--depth 3 must include a.ts; got: {}",
        stdout3
    );
}

// ---------------------------------------------------------------------------
// init command — AC 8
// ---------------------------------------------------------------------------

/// AC 8 (claude detected): `codebones init --home <dir>` reports that Claude
/// Code was detected and configured.
#[test]
fn init_reports_claude_code_detected_and_configured() {
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".claude")).unwrap();

    let output = Command::cargo_bin("codebones")
        .unwrap()
        .args(["init", "--home", home.path().to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    assert!(
        stdout.to_lowercase().contains("claude"),
        "`codebones init` must mention Claude Code in output when detected; got: {}",
        stdout
    );
}

/// AC 8 (cursor detected): `codebones init --home <dir>` reports that Cursor
/// was detected and configured.
#[test]
fn init_reports_cursor_detected_and_configured() {
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".cursor")).unwrap();

    let output = Command::cargo_bin("codebones")
        .unwrap()
        .args(["init", "--home", home.path().to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    assert!(
        stdout.to_lowercase().contains("cursor"),
        "`codebones init` must mention Cursor in output when detected; got: {}",
        stdout
    );
}

/// AC 8 (both detected): When both ~/.claude/ and ~/.cursor/ exist, the output
/// mentions both tools.
#[test]
fn init_reports_both_tools_when_both_detected() {
    let home = TempDir::new().unwrap();
    fs::create_dir_all(home.path().join(".claude")).unwrap();
    fs::create_dir_all(home.path().join(".cursor")).unwrap();

    let output = Command::cargo_bin("codebones")
        .unwrap()
        .args(["init", "--home", home.path().to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    assert!(
        stdout.to_lowercase().contains("claude"),
        "output must mention Claude Code; got: {}",
        stdout
    );
    assert!(
        stdout.to_lowercase().contains("cursor"),
        "output must mention Cursor; got: {}",
        stdout
    );
}

/// AC 8 + AC 9 (none detected): When no tools are found, init exits
/// successfully and its output indicates no tools were found.
#[test]
fn init_reports_no_tools_found_when_none_detected() {
    let home = TempDir::new().unwrap();
    // Neither .claude nor .cursor is created

    let output = Command::cargo_bin("codebones")
        .unwrap()
        .args(["init", "--home", home.path().to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    // Output should indicate nothing was found — accept any phrasing that
    // conveys "no tools" / "none" / "not found" / "no supported".
    let lower = stdout.to_lowercase();
    assert!(
        lower.contains("no") || lower.contains("none") || lower.contains("not found"),
        "`codebones init` must report that no tools were found; got: {}",
        stdout
    );
}

// ===========================================================================
// CLI: search --expand flag
// ===========================================================================

/// `codebones search <query> --expand` outputs both the symbol ID and source code.
#[test]
fn test_search_expand_returns_id_and_source() {
    let temp = setup_dummy_repo();

    // Index first so the DB exists.
    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(temp.path())
        .args(["index", "."])
        .assert()
        .success();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(temp.path())
        .args(["search", "hello_world", "--expand"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("hello_world")
                .and(predicate::str::contains("pub fn")),
        );
}

/// `codebones search <query> --expand` includes the actual function body in output.
#[test]
fn test_search_expand_source_contains_function_body() {
    let temp = setup_dummy_repo();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(temp.path())
        .args(["index", "."])
        .assert()
        .success();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(temp.path())
        .args(["search", "hello_world", "--expand"])
        .assert()
        .success()
        // The function body must appear — not just the signature.
        .stdout(predicate::str::contains("println!"));
}

/// `codebones search <query>` without --expand returns only symbol IDs (existing behavior).
#[test]
fn test_search_without_expand_returns_only_symbol_ids() {
    let temp = setup_dummy_repo();

    Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(temp.path())
        .args(["index", "."])
        .assert()
        .success();

    let output = Command::cargo_bin("codebones")
        .unwrap()
        .current_dir(temp.path())
        .args(["search", "hello_world"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let stdout = String::from_utf8_lossy(&output);
    // Must contain the symbol ID.
    assert!(
        stdout.contains("hello_world"),
        "search without --expand must still return the symbol ID; got: {}",
        stdout
    );
    // Must NOT contain source code keywords that would only appear with --expand.
    assert!(
        !stdout.contains("println!"),
        "search without --expand must not include function body source; got: {}",
        stdout
    );
}
