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
    fs::write(root.join("base64.txt"), format!("let b64 = \"{}\";", long_b64)).unwrap();

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
    cmd.current_dir(root).args(["index", "."]).assert().success();

    // Outline
    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["outline", "dummy.rs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("pub fn hello_world()"));
        
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

    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["pack", ".", "--format", "xml"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("<repository>")
                .and(predicate::str::contains("</repository>"))
                .and(predicate::str::contains("<skeleton_map>"))
                .and(predicate::str::contains("<signature>Function hello_world</signature>"))
                .and(predicate::str::contains("<![CDATA["))
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
                .and(predicate::str::contains("  - Function hello_world"))
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
        .stdout(
            predicate::str::contains("<skeleton_map>").not()
        );
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
                .and(predicate::str::contains("</repository>"))
        );
}

#[test]
fn test_pack_flags_remove_comments_and_empty_lines() {
    let temp = setup_dummy_repo();
    let root = temp.path();

    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["pack", "dummy.rs", "--format", "xml", "--remove-comments", "--remove-empty-lines"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("A single line comment").not()
                .and(predicate::str::contains("A block comment").not())
                .and(predicate::str::contains("\n\n\n").not()) // Multiple newlines should be gone
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
                .and(predicate::str::contains("ABCDEFGHIJKLMNOPQRSTUVWXYZ").not())
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
            predicate::str::contains("dummy.toml")
                .and(predicate::str::contains("dummy.rs").not())
        );

    // Test ignore
    let mut cmd = Command::cargo_bin("codebones").unwrap();
    cmd.current_dir(root)
        .args(["pack", ".", "--format", "xml", "--ignore", "**/*.toml"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("dummy.rs")
                .and(predicate::str::contains("dummy.toml").not())
        );
}