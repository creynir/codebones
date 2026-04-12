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
            predicate::str::contains("dummy.toml")
                .and(predicate::str::contains("dummy.rs").not()),
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
            predicate::str::contains("dummy.rs")
                .and(predicate::str::contains("dummy.toml").not()),
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
