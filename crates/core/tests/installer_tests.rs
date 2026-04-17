/// Integration tests for codebones_core::installer.
///
/// These are Red-phase tests for the installer API. The implementation is
/// expected to be added later, so unresolved imports are an acceptable initial
/// failure mode.
use codebones_core::installer::{
    apply_init_actions, ensure_canonical_skill, install_skill_target, register_mcp_target,
    FileActionStatus, InitAction, InitContext, InstallMethod, InstallerError, McpTargetKind,
    SkillTargetKind, CODEBONES_MANAGED_MARKER, CODEBONES_SKILL_NAME, EMBEDDED_CODEBONES_SKILL,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const USER_AUTHORED_CODEBONES_SKILL: &str = r#"---
name: codebones
description: User-authored local skill.
---

These are my private Codebones instructions.
"#;

const OLD_MANAGED_CODEBONES_SKILL: &str = r#"---
name: codebones
description: Old managed Codebones skill.
---
<!-- codebones-managed-skill:v1 -->

Old managed body.
"#;

fn ctx(home: &TempDir, project: &TempDir) -> InitContext {
    InitContext {
        home_dir: home.path().to_path_buf(),
        project_dir: project.path().to_path_buf(),
    }
}

fn canonical_skill_path(home_dir: &Path) -> PathBuf {
    home_dir
        .join(".codebones")
        .join("skills")
        .join("codebones")
        .join("SKILL.md")
}

fn claude_mcp_config_path(home_dir: &Path) -> PathBuf {
    home_dir.join(".claude").join("settings.json")
}

fn cursor_mcp_config_path(home_dir: &Path) -> PathBuf {
    home_dir.join(".cursor").join("mcp.json")
}

fn target_skill_path(home_dir: &Path, project_dir: &Path, target: SkillTargetKind) -> PathBuf {
    match target {
        SkillTargetKind::ClaudeGlobal => home_dir
            .join(".claude")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
        SkillTargetKind::ClaudeProject => project_dir
            .join(".claude")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
        SkillTargetKind::CodexGlobal => home_dir
            .join(".agents")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
        SkillTargetKind::UniversalProject => project_dir
            .join(".agents")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
        SkillTargetKind::CursorProject => project_dir
            .join(".cursor")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
        SkillTargetKind::OpenCodeGlobal => home_dir
            .join(".config")
            .join("opencode")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
        SkillTargetKind::GeminiProject => project_dir
            .join(".gemini")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
        SkillTargetKind::DroidGlobal => home_dir
            .join(".factory")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
        SkillTargetKind::PiGlobal => home_dir
            .join(".pi")
            .join("agent")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
    }
}

fn write_file(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("test path should have parent"))
        .expect("failed to create test parent directory");
    fs::write(path, content).expect("failed to write test file");
}

fn desired_mcp_entry() -> serde_json::Value {
    serde_json::json!({
        "command": "codebones-mcp",
        "args": [],
        "type": "stdio",
    })
}

fn read_json_file(path: &Path) -> serde_json::Value {
    let content = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("{} should be readable: {error}", path.display()));
    serde_json::from_str(&content)
        .unwrap_or_else(|error| panic!("{} should contain valid JSON: {error}", path.display()))
}

fn assert_user_authored_conflict(error: InstallerError, expected_path: &Path) {
    match error {
        InstallerError::UserAuthoredCodebonesConflict { path } => {
            assert_eq!(path, expected_path);
        }
        other => panic!("expected UserAuthoredCodebonesConflict, got {other:?}"),
    }
}

fn assert_copy_installs_to_path(ctx: &InitContext, target: SkillTargetKind, expected_path: &Path) {
    let status = install_skill_target(ctx, target, InstallMethod::Copy, false)
        .expect("provider copy install should succeed");

    assert_eq!(status, FileActionStatus::Created);
    assert_eq!(
        fs::read_to_string(expected_path)
            .unwrap_or_else(|error| panic!("{} should exist: {error}", expected_path.display())),
        EMBEDDED_CODEBONES_SKILL,
        "{} should contain the embedded skill",
        expected_path.display()
    );
}

#[test]
fn register_mcp_target_creates_missing_claude_config() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let settings_path = claude_mcp_config_path(home.path());

    let status = register_mcp_target(&ctx, McpTargetKind::ClaudeGlobal, false)
        .expect("Claude MCP registration should succeed");

    assert_eq!(status, FileActionStatus::Created);
    assert!(
        settings_path.exists(),
        "Claude settings should be created at {}",
        settings_path.display()
    );

    let json = read_json_file(&settings_path);
    assert_eq!(json["mcpServers"]["codebones"], desired_mcp_entry());
}

#[test]
fn register_mcp_target_updates_cursor_config_and_preserves_other_servers() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let mcp_path = cursor_mcp_config_path(home.path());
    let existing_other = serde_json::json!({
        "command": "other-mcp",
        "args": ["--flag"],
        "type": "stdio",
    });
    let stale_codebones = serde_json::json!({
        "command": "old-codebones-mcp",
        "args": ["--legacy"],
        "type": "stdio",
    });
    write_file(
        &mcp_path,
        &serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": {
                "other-tool": existing_other,
                "codebones": stale_codebones,
            },
            "unrelated": {
                "enabled": true,
            },
        }))
        .expect("existing Cursor MCP config should serialize"),
    );

    let status = register_mcp_target(&ctx, McpTargetKind::CursorGlobal, false)
        .expect("Cursor MCP registration should succeed");

    assert_eq!(status, FileActionStatus::Updated);
    let json = read_json_file(&mcp_path);
    assert_eq!(
        json["mcpServers"]["other-tool"],
        serde_json::json!({
            "command": "other-mcp",
            "args": ["--flag"],
            "type": "stdio",
        })
    );
    assert_eq!(json["mcpServers"]["codebones"], desired_mcp_entry());
    assert_eq!(json["unrelated"]["enabled"], true);
}

#[test]
fn register_mcp_target_leaves_equal_claude_entry_unchanged() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let settings_path = claude_mcp_config_path(home.path());
    let existing = serde_json::to_string_pretty(&serde_json::json!({
        "mcpServers": {
            "codebones": desired_mcp_entry(),
        },
    }))
    .expect("existing Claude MCP config should serialize");
    write_file(&settings_path, &existing);

    let status = register_mcp_target(&ctx, McpTargetKind::ClaudeGlobal, false)
        .expect("matching Claude MCP registration should succeed");

    assert_eq!(status, FileActionStatus::Unchanged);
    assert_eq!(
        fs::read_to_string(&settings_path).expect("Claude settings should be readable"),
        existing
    );
}

#[test]
fn register_mcp_target_rejects_malformed_claude_json_without_modifying_file() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let settings_path = claude_mcp_config_path(home.path());
    let malformed = r#"{"mcpServers": {"codebones": "#;
    write_file(&settings_path, malformed);

    let error = register_mcp_target(&ctx, McpTargetKind::ClaudeGlobal, false)
        .expect_err("malformed Claude JSON should be rejected");

    match error {
        InstallerError::InvalidMcpJson { path, source: _ } => {
            assert_eq!(path, settings_path);
        }
        other => panic!("expected InvalidMcpJson, got {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(&settings_path).expect("Claude settings should be readable"),
        malformed
    );
}

#[test]
fn register_mcp_target_dry_run_missing_cursor_config_would_create_no_file() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let mcp_path = cursor_mcp_config_path(home.path());

    let status = register_mcp_target(&ctx, McpTargetKind::CursorGlobal, true)
        .expect("Cursor MCP dry-run registration should succeed");

    assert_eq!(status, FileActionStatus::WouldCreate);
    assert!(
        !mcp_path.exists(),
        "dry run should not create {}",
        mcp_path.display()
    );
}

#[test]
fn apply_init_actions_dry_run_register_mcp_returns_would_create_and_creates_no_file() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let mcp_path = cursor_mcp_config_path(home.path());

    let results = apply_init_actions(
        &ctx,
        &[InitAction::RegisterMcp {
            target: McpTargetKind::CursorGlobal,
        }],
        true,
    )
    .expect("dry-run MCP init action should succeed");

    assert_eq!(
        results,
        vec![(
            InitAction::RegisterMcp {
                target: McpTargetKind::CursorGlobal
            },
            FileActionStatus::WouldCreate
        )]
    );
    assert!(
        !mcp_path.exists(),
        "dry run should not create {}",
        mcp_path.display()
    );
}

#[test]
fn ensure_canonical_skill_creates_embedded_managed_skill() {
    let home = TempDir::new().expect("failed to create temp home");
    let canonical_path = canonical_skill_path(home.path());

    let status =
        ensure_canonical_skill(home.path(), false).expect("canonical skill should be created");

    assert_eq!(CODEBONES_SKILL_NAME, "codebones");
    assert_eq!(
        CODEBONES_MANAGED_MARKER,
        "<!-- codebones-managed-skill:v1 -->"
    );
    assert_eq!(status, FileActionStatus::Created);
    assert_eq!(
        fs::read_to_string(&canonical_path).expect("canonical skill should be readable"),
        EMBEDDED_CODEBONES_SKILL
    );
    assert!(
        EMBEDDED_CODEBONES_SKILL.contains(CODEBONES_MANAGED_MARKER),
        "embedded skill should contain the managed marker"
    );
    assert!(
        EMBEDDED_CODEBONES_SKILL.contains("name: codebones"),
        "embedded skill should declare the codebones skill name"
    );
    assert!(
        EMBEDDED_CODEBONES_SKILL.contains("codebones search")
            || EMBEDDED_CODEBONES_SKILL.contains("codebones map")
            || EMBEDDED_CODEBONES_SKILL.contains("codebones get"),
        "embedded skill should include a codebones CLI usage hint"
    );
}

#[test]
fn ensure_canonical_skill_is_unchanged_when_content_already_matches() {
    let home = TempDir::new().expect("failed to create temp home");
    let canonical_path = canonical_skill_path(home.path());
    write_file(&canonical_path, EMBEDDED_CODEBONES_SKILL);

    let status =
        ensure_canonical_skill(home.path(), false).expect("matching canonical skill should pass");

    assert_eq!(status, FileActionStatus::Unchanged);
    assert_eq!(
        fs::read_to_string(&canonical_path).expect("canonical skill should be readable"),
        EMBEDDED_CODEBONES_SKILL
    );
}

#[test]
fn ensure_canonical_skill_updates_previous_managed_version() {
    let home = TempDir::new().expect("failed to create temp home");
    let canonical_path = canonical_skill_path(home.path());
    write_file(&canonical_path, OLD_MANAGED_CODEBONES_SKILL);

    let status =
        ensure_canonical_skill(home.path(), false).expect("managed canonical skill should update");

    assert_eq!(status, FileActionStatus::Updated);
    assert_eq!(
        fs::read_to_string(&canonical_path).expect("canonical skill should be readable"),
        EMBEDDED_CODEBONES_SKILL
    );
}

#[test]
fn ensure_canonical_skill_refuses_user_authored_codebones_skill() {
    let home = TempDir::new().expect("failed to create temp home");
    let canonical_path = canonical_skill_path(home.path());
    write_file(&canonical_path, USER_AUTHORED_CODEBONES_SKILL);

    let error = ensure_canonical_skill(home.path(), false)
        .expect_err("user-authored canonical skill should be refused");

    assert_user_authored_conflict(error, &canonical_path);
    assert_eq!(
        fs::read_to_string(&canonical_path).expect("canonical skill should be readable"),
        USER_AUTHORED_CODEBONES_SKILL
    );
}

#[test]
fn install_codex_global_copy_creates_official_agents_skill_and_canonical_skill() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let canonical_path = canonical_skill_path(home.path());
    let target_path = home
        .path()
        .join(".agents")
        .join("skills")
        .join("codebones")
        .join("SKILL.md");

    let status = install_skill_target(
        &ctx,
        SkillTargetKind::CodexGlobal,
        InstallMethod::Copy,
        false,
    )
    .expect("Codex global copy install should succeed");

    assert_eq!(status, FileActionStatus::Created);
    assert_eq!(
        fs::read_to_string(&canonical_path).expect("canonical skill should be readable"),
        EMBEDDED_CODEBONES_SKILL
    );
    assert_eq!(
        fs::read_to_string(&target_path).expect("Codex global skill should be readable"),
        EMBEDDED_CODEBONES_SKILL
    );
    assert!(
        fs::symlink_metadata(&target_path)
            .expect("Codex global skill metadata should be readable")
            .file_type()
            .is_file(),
        "Codex global copy should be a regular file"
    );
    assert!(
        !home.path().join(".codex").join("skills").exists(),
        "CodexGlobal must use ~/.agents/skills, not the legacy ~/.codex/skills path"
    );
}

#[test]
fn install_claude_global_symlink_points_to_canonical_skill() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let canonical_path = canonical_skill_path(home.path());
    let target_path = target_skill_path(home.path(), project.path(), SkillTargetKind::ClaudeGlobal);

    let status = install_skill_target(
        &ctx,
        SkillTargetKind::ClaudeGlobal,
        InstallMethod::Symlink,
        false,
    )
    .expect("Claude global symlink install should succeed");

    assert_eq!(status, FileActionStatus::Created);
    assert!(
        fs::symlink_metadata(&target_path)
            .expect("Claude global skill metadata should be readable")
            .file_type()
            .is_symlink(),
        "Claude global install should create SKILL.md as a symlink"
    );
    assert_eq!(
        fs::canonicalize(&target_path).expect("target symlink should resolve"),
        fs::canonicalize(&canonical_path).expect("canonical skill should resolve")
    );
    assert_eq!(
        fs::read_to_string(&target_path).expect("Claude global skill should be readable"),
        EMBEDDED_CODEBONES_SKILL
    );
}

#[test]
fn install_skill_target_symlink_replaces_codebones_owned_regular_provider_skill() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let canonical_path = canonical_skill_path(home.path());
    let target_path = target_skill_path(home.path(), project.path(), SkillTargetKind::CodexGlobal);
    write_file(&canonical_path, EMBEDDED_CODEBONES_SKILL);
    write_file(&target_path, EMBEDDED_CODEBONES_SKILL);

    let status = install_skill_target(
        &ctx,
        SkillTargetKind::CodexGlobal,
        InstallMethod::Symlink,
        false,
    )
    .expect("Codebones-owned regular provider skill should be replaced with a symlink");

    let provider_is_symlink = fs::symlink_metadata(&target_path)
        .expect("provider skill metadata should be readable")
        .file_type()
        .is_symlink();
    let provider_resolves_to_canonical = fs::canonicalize(&target_path)
        .expect("provider skill should resolve")
        == fs::canonicalize(&canonical_path).expect("canonical skill should resolve");

    assert_eq!(
        (status, provider_is_symlink, provider_resolves_to_canonical),
        (FileActionStatus::ReplacedCodebonesOwned, true, true)
    );
}

#[test]
fn install_skill_target_symlink_dry_run_would_replace_codebones_owned_regular_provider_skill() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let canonical_path = canonical_skill_path(home.path());
    let target_path = target_skill_path(home.path(), project.path(), SkillTargetKind::CodexGlobal);
    write_file(&canonical_path, EMBEDDED_CODEBONES_SKILL);
    write_file(&target_path, EMBEDDED_CODEBONES_SKILL);

    let status = install_skill_target(
        &ctx,
        SkillTargetKind::CodexGlobal,
        InstallMethod::Symlink,
        true,
    )
    .expect("dry run should report Codebones-owned regular provider replacement");

    assert_eq!(status, FileActionStatus::WouldReplaceCodebonesOwned);
    assert!(
        fs::symlink_metadata(&target_path)
            .expect("provider skill metadata should be readable")
            .file_type()
            .is_file(),
        "dry run should leave the provider skill as a regular file"
    );
    assert_eq!(
        fs::read_to_string(&target_path).expect("provider skill should be readable"),
        EMBEDDED_CODEBONES_SKILL
    );
}

#[test]
fn install_skill_target_uses_expected_provider_destination_paths() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);

    assert_copy_installs_to_path(
        &ctx,
        SkillTargetKind::ClaudeGlobal,
        &home
            .path()
            .join(".claude")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
    );
    assert_copy_installs_to_path(
        &ctx,
        SkillTargetKind::ClaudeProject,
        &project
            .path()
            .join(".claude")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
    );
    assert_copy_installs_to_path(
        &ctx,
        SkillTargetKind::CodexGlobal,
        &home
            .path()
            .join(".agents")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
    );
    assert_copy_installs_to_path(
        &ctx,
        SkillTargetKind::UniversalProject,
        &project
            .path()
            .join(".agents")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
    );
    assert_copy_installs_to_path(
        &ctx,
        SkillTargetKind::CursorProject,
        &project
            .path()
            .join(".cursor")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
    );
    assert_copy_installs_to_path(
        &ctx,
        SkillTargetKind::OpenCodeGlobal,
        &home
            .path()
            .join(".config")
            .join("opencode")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
    );
    assert_copy_installs_to_path(
        &ctx,
        SkillTargetKind::GeminiProject,
        &project
            .path()
            .join(".gemini")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
    );
    assert_copy_installs_to_path(
        &ctx,
        SkillTargetKind::DroidGlobal,
        &home
            .path()
            .join(".factory")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
    );
    assert_copy_installs_to_path(
        &ctx,
        SkillTargetKind::PiGlobal,
        &home
            .path()
            .join(".pi")
            .join("agent")
            .join("skills")
            .join("codebones")
            .join("SKILL.md"),
    );
}

#[test]
fn install_skill_target_copy_refuses_user_authored_provider_skill() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let target_path = target_skill_path(home.path(), project.path(), SkillTargetKind::CodexGlobal);
    write_file(&target_path, USER_AUTHORED_CODEBONES_SKILL);

    let error = install_skill_target(
        &ctx,
        SkillTargetKind::CodexGlobal,
        InstallMethod::Copy,
        false,
    )
    .expect_err("user-authored provider skill should be refused");

    assert_user_authored_conflict(error, &target_path);
    assert_eq!(
        fs::read_to_string(&target_path).expect("provider skill should be readable"),
        USER_AUTHORED_CODEBONES_SKILL
    );
}

#[test]
fn apply_init_actions_dry_run_prepends_canonical_action_and_creates_no_files() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);
    let canonical_path = canonical_skill_path(home.path());
    let codex_path = target_skill_path(home.path(), project.path(), SkillTargetKind::CodexGlobal);

    let results = apply_init_actions(
        &ctx,
        &[InitAction::InstallSkill {
            target: SkillTargetKind::CodexGlobal,
            method: InstallMethod::Copy,
        }],
        true,
    )
    .expect("dry-run init should succeed");

    assert_eq!(
        results,
        vec![
            (
                InitAction::MaterializeCanonicalSkill,
                FileActionStatus::WouldCreate
            ),
            (
                InitAction::InstallSkill {
                    target: SkillTargetKind::CodexGlobal,
                    method: InstallMethod::Copy
                },
                FileActionStatus::WouldCreate
            ),
        ]
    );
    assert!(
        !canonical_path.exists(),
        "dry run should not create the canonical skill"
    );
    assert!(
        !codex_path.exists(),
        "dry run should not create provider skill"
    );
}

#[test]
fn apply_init_actions_deduplicates_exact_duplicate_installs() {
    let home = TempDir::new().expect("failed to create temp home");
    let project = TempDir::new().expect("failed to create temp project");
    let ctx = ctx(&home, &project);

    let results = apply_init_actions(
        &ctx,
        &[
            InitAction::InstallSkill {
                target: SkillTargetKind::CodexGlobal,
                method: InstallMethod::Copy,
            },
            InitAction::InstallSkill {
                target: SkillTargetKind::ClaudeGlobal,
                method: InstallMethod::Copy,
            },
            InitAction::InstallSkill {
                target: SkillTargetKind::CodexGlobal,
                method: InstallMethod::Copy,
            },
        ],
        true,
    )
    .expect("dry-run init should succeed");

    assert_eq!(
        results,
        vec![
            (
                InitAction::MaterializeCanonicalSkill,
                FileActionStatus::WouldCreate
            ),
            (
                InitAction::InstallSkill {
                    target: SkillTargetKind::CodexGlobal,
                    method: InstallMethod::Copy
                },
                FileActionStatus::WouldCreate
            ),
            (
                InitAction::InstallSkill {
                    target: SkillTargetKind::ClaudeGlobal,
                    method: InstallMethod::Copy
                },
                FileActionStatus::WouldCreate
            ),
        ],
        "exact duplicate install actions should be removed while preserving first occurrence"
    );
}
