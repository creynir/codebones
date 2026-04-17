use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub const CODEBONES_SKILL_NAME: &str = "codebones";
pub const CODEBONES_MANAGED_MARKER: &str = "<!-- codebones-managed-skill:v1 -->";
pub const EMBEDDED_CODEBONES_SKILL: &str = include_str!("../assets/skills/codebones/SKILL.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstallMethod {
    Copy,
    Symlink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillTargetKind {
    ClaudeGlobal,
    ClaudeProject,
    CodexGlobal,
    UniversalProject,
    CursorProject,
    OpenCodeGlobal,
    GeminiProject,
    DroidGlobal,
    PiGlobal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum McpTargetKind {
    ClaudeGlobal,
    CursorGlobal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InitAction {
    MaterializeCanonicalSkill,
    InstallSkill {
        target: SkillTargetKind,
        method: InstallMethod,
    },
    RegisterMcp {
        target: McpTargetKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitContext {
    pub home_dir: PathBuf,
    pub project_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileActionStatus {
    Created,
    Updated,
    Unchanged,
    ReplacedCodebonesOwned,
    WouldCreate,
    WouldUpdate,
    WouldReplaceCodebonesOwned,
    WouldLeaveUnchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingSkillState {
    MatchingCodebonesManagedSkill,
    StaleCodebonesManagedSkill,
    UserAuthoredCodebonesSkill,
    RegularFile,
    SymlinkToCanonical,
    Symlink,
    Directory,
    Other,
}

#[derive(Debug, thiserror::Error)]
pub enum InstallerError {
    #[error("refusing to overwrite user-authored codebones skill at {}", path.display())]
    UserAuthoredCodebonesConflict { path: PathBuf },

    #[error(
        "refusing to overwrite existing skill destination at {} ({state:?})",
        path.display()
    )]
    SkillDestinationConflict {
        path: PathBuf,
        state: ExistingSkillState,
    },

    #[error(
        "failed to create symlink {} -> {}: {source}",
        link.display(),
        target.display()
    )]
    SymlinkFailed {
        link: PathBuf,
        target: PathBuf,
        #[source]
        source: io::Error,
    },

    #[error("invalid MCP JSON at {}: {source}", path.display())]
    InvalidMcpJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error(transparent)]
    Io(#[from] io::Error),
}

pub fn ensure_canonical_skill(
    home_dir: impl AsRef<Path>,
    dry_run: bool,
) -> Result<FileActionStatus, InstallerError> {
    let path = canonical_skill_path(home_dir.as_ref());
    materialize_copy(&path, &path, dry_run)
}

pub fn install_skill_target(
    ctx: &InitContext,
    target: SkillTargetKind,
    method: InstallMethod,
    dry_run: bool,
) -> Result<FileActionStatus, InstallerError> {
    let canonical_path = canonical_skill_path(&ctx.home_dir);

    if !dry_run {
        ensure_canonical_skill(&ctx.home_dir, false)?;
    }

    let destination = target_skill_path(ctx, target);
    match method {
        InstallMethod::Copy => materialize_copy(&destination, &canonical_path, dry_run),
        InstallMethod::Symlink => materialize_symlink(&destination, &canonical_path, dry_run),
    }
}

pub fn register_mcp_target(
    ctx: &InitContext,
    target: McpTargetKind,
    dry_run: bool,
) -> Result<FileActionStatus, InstallerError> {
    let path = target_mcp_config_path(ctx, target);
    let content = match fs::read_to_string(&path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if dry_run {
                return Ok(FileActionStatus::WouldCreate);
            }

            let config = desired_mcp_config();
            write_mcp_config(&path, &config)?;
            return Ok(FileActionStatus::Created);
        }
        Err(error) => return Err(error.into()),
    };

    let config: serde_json::Value =
        serde_json::from_str(&content).map_err(|source| InstallerError::InvalidMcpJson {
            path: path.clone(),
            source,
        })?;
    let (updated_config, changed) = set_desired_mcp_entry(config);

    if !changed {
        return Ok(if dry_run {
            FileActionStatus::WouldLeaveUnchanged
        } else {
            FileActionStatus::Unchanged
        });
    }

    if dry_run {
        return Ok(FileActionStatus::WouldUpdate);
    }

    write_mcp_config(&path, &updated_config)?;
    Ok(FileActionStatus::Updated)
}

pub fn apply_init_actions(
    ctx: &InitContext,
    actions: &[InitAction],
    dry_run: bool,
) -> Result<Vec<(InitAction, FileActionStatus)>, InstallerError> {
    let mut deduped = Vec::new();
    let mut seen = HashSet::new();

    for action in actions {
        if seen.insert(*action) {
            deduped.push(*action);
        }
    }

    let has_install = deduped
        .iter()
        .any(|action| matches!(action, InitAction::InstallSkill { .. }));
    let has_canonical = deduped
        .iter()
        .any(|action| matches!(action, InitAction::MaterializeCanonicalSkill));

    if has_install && !has_canonical {
        deduped.insert(0, InitAction::MaterializeCanonicalSkill);
    }

    let mut results = Vec::with_capacity(deduped.len());
    for action in deduped {
        let status = match action {
            InitAction::MaterializeCanonicalSkill => {
                ensure_canonical_skill(&ctx.home_dir, dry_run)?
            }
            InitAction::InstallSkill { target, method } => {
                install_skill_target(ctx, target, method, dry_run)?
            }
            InitAction::RegisterMcp { target } => register_mcp_target(ctx, target, dry_run)?,
        };
        results.push((action, status));
    }

    Ok(results)
}

fn canonical_skill_path(home_dir: &Path) -> PathBuf {
    home_dir
        .join(".codebones")
        .join("skills")
        .join(CODEBONES_SKILL_NAME)
        .join("SKILL.md")
}

fn target_skill_path(ctx: &InitContext, target: SkillTargetKind) -> PathBuf {
    match target {
        SkillTargetKind::ClaudeGlobal => ctx
            .home_dir
            .join(".claude")
            .join("skills")
            .join(CODEBONES_SKILL_NAME)
            .join("SKILL.md"),
        SkillTargetKind::ClaudeProject => ctx
            .project_dir
            .join(".claude")
            .join("skills")
            .join(CODEBONES_SKILL_NAME)
            .join("SKILL.md"),
        SkillTargetKind::CodexGlobal => ctx
            .home_dir
            .join(".agents")
            .join("skills")
            .join(CODEBONES_SKILL_NAME)
            .join("SKILL.md"),
        SkillTargetKind::UniversalProject => ctx
            .project_dir
            .join(".agents")
            .join("skills")
            .join(CODEBONES_SKILL_NAME)
            .join("SKILL.md"),
        SkillTargetKind::CursorProject => ctx
            .project_dir
            .join(".cursor")
            .join("skills")
            .join(CODEBONES_SKILL_NAME)
            .join("SKILL.md"),
        SkillTargetKind::OpenCodeGlobal => ctx
            .home_dir
            .join(".config")
            .join("opencode")
            .join("skills")
            .join(CODEBONES_SKILL_NAME)
            .join("SKILL.md"),
        SkillTargetKind::GeminiProject => ctx
            .project_dir
            .join(".gemini")
            .join("skills")
            .join(CODEBONES_SKILL_NAME)
            .join("SKILL.md"),
        SkillTargetKind::DroidGlobal => ctx
            .home_dir
            .join(".factory")
            .join("skills")
            .join(CODEBONES_SKILL_NAME)
            .join("SKILL.md"),
        SkillTargetKind::PiGlobal => ctx
            .home_dir
            .join(".pi")
            .join("agent")
            .join("skills")
            .join(CODEBONES_SKILL_NAME)
            .join("SKILL.md"),
    }
}

fn target_mcp_config_path(ctx: &InitContext, target: McpTargetKind) -> PathBuf {
    match target {
        McpTargetKind::ClaudeGlobal => ctx.home_dir.join(".claude").join("settings.json"),
        McpTargetKind::CursorGlobal => ctx.home_dir.join(".cursor").join("mcp.json"),
    }
}

fn desired_mcp_config() -> serde_json::Value {
    let mut servers = serde_json::Map::new();
    servers.insert(CODEBONES_SKILL_NAME.to_string(), desired_mcp_entry());

    let mut config = serde_json::Map::new();
    config.insert("mcpServers".to_string(), serde_json::Value::Object(servers));

    serde_json::Value::Object(config)
}

fn desired_mcp_entry() -> serde_json::Value {
    serde_json::json!({
        "command": "codebones-mcp",
        "args": [],
        "type": "stdio",
    })
}

fn set_desired_mcp_entry(config: serde_json::Value) -> (serde_json::Value, bool) {
    let serde_json::Value::Object(mut root) = config else {
        return (desired_mcp_config(), true);
    };

    let desired_entry = desired_mcp_entry();
    match root.get_mut("mcpServers") {
        Some(serde_json::Value::Object(servers)) => {
            if servers.get(CODEBONES_SKILL_NAME) == Some(&desired_entry) {
                return (serde_json::Value::Object(root), false);
            }

            servers.insert(CODEBONES_SKILL_NAME.to_string(), desired_entry);
        }
        _ => {
            let mut servers = serde_json::Map::new();
            servers.insert(CODEBONES_SKILL_NAME.to_string(), desired_entry);
            root.insert("mcpServers".to_string(), serde_json::Value::Object(servers));
        }
    }

    (serde_json::Value::Object(root), true)
}

fn write_mcp_config(path: &Path, config: &serde_json::Value) -> Result<(), InstallerError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serde_json::to_string_pretty(config)
        .expect("serializing an MCP JSON value should not fail");
    fs::write(path, content)?;
    Ok(())
}

fn materialize_copy(
    path: &Path,
    canonical_path: &Path,
    dry_run: bool,
) -> Result<FileActionStatus, InstallerError> {
    match inspect_existing_skill(path, canonical_path)? {
        None => {
            if dry_run {
                return Ok(FileActionStatus::WouldCreate);
            }

            write_embedded_skill(path)?;
            Ok(FileActionStatus::Created)
        }
        Some(ExistingSkillState::MatchingCodebonesManagedSkill)
        | Some(ExistingSkillState::SymlinkToCanonical) => Ok(if dry_run {
            FileActionStatus::WouldLeaveUnchanged
        } else {
            FileActionStatus::Unchanged
        }),
        Some(ExistingSkillState::StaleCodebonesManagedSkill) => {
            if dry_run {
                return Ok(FileActionStatus::WouldUpdate);
            }

            write_embedded_skill(path)?;
            Ok(FileActionStatus::Updated)
        }
        Some(ExistingSkillState::UserAuthoredCodebonesSkill) => {
            Err(InstallerError::UserAuthoredCodebonesConflict {
                path: path.to_path_buf(),
            })
        }
        Some(state) => Err(InstallerError::SkillDestinationConflict {
            path: path.to_path_buf(),
            state,
        }),
    }
}

fn materialize_symlink(
    link: &Path,
    canonical_path: &Path,
    dry_run: bool,
) -> Result<FileActionStatus, InstallerError> {
    match inspect_existing_skill(link, canonical_path)? {
        None => {
            if dry_run {
                return Ok(FileActionStatus::WouldCreate);
            }

            create_skill_symlink(link, canonical_path)?;
            Ok(FileActionStatus::Created)
        }
        Some(ExistingSkillState::SymlinkToCanonical) => Ok(if dry_run {
            FileActionStatus::WouldLeaveUnchanged
        } else {
            FileActionStatus::Unchanged
        }),
        Some(ExistingSkillState::MatchingCodebonesManagedSkill)
        | Some(ExistingSkillState::StaleCodebonesManagedSkill) => {
            if dry_run {
                return Ok(FileActionStatus::WouldReplaceCodebonesOwned);
            }

            replace_with_skill_symlink(link, canonical_path)?;
            Ok(FileActionStatus::ReplacedCodebonesOwned)
        }
        Some(ExistingSkillState::UserAuthoredCodebonesSkill) => {
            Err(InstallerError::UserAuthoredCodebonesConflict {
                path: link.to_path_buf(),
            })
        }
        Some(state) => Err(InstallerError::SkillDestinationConflict {
            path: link.to_path_buf(),
            state,
        }),
    }
}

fn inspect_existing_skill(
    path: &Path,
    canonical_path: &Path,
) -> Result<Option<ExistingSkillState>, InstallerError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };

    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Ok(Some(if symlink_points_to(path, canonical_path) {
            ExistingSkillState::SymlinkToCanonical
        } else {
            ExistingSkillState::Symlink
        }));
    }

    if file_type.is_dir() {
        return Ok(Some(ExistingSkillState::Directory));
    }

    if !file_type.is_file() {
        return Ok(Some(ExistingSkillState::Other));
    }

    let content = fs::read_to_string(path)?;
    if content == EMBEDDED_CODEBONES_SKILL {
        return Ok(Some(ExistingSkillState::MatchingCodebonesManagedSkill));
    }

    let is_codebones_skill = frontmatter_name_is_codebones(&content);
    let is_managed = content.contains(CODEBONES_MANAGED_MARKER);

    if is_codebones_skill && is_managed {
        Ok(Some(ExistingSkillState::StaleCodebonesManagedSkill))
    } else if is_codebones_skill {
        Ok(Some(ExistingSkillState::UserAuthoredCodebonesSkill))
    } else {
        Ok(Some(ExistingSkillState::RegularFile))
    }
}

fn write_embedded_skill(path: &Path) -> Result<(), InstallerError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, EMBEDDED_CODEBONES_SKILL)?;
    Ok(())
}

fn replace_with_skill_symlink(link: &Path, target: &Path) -> Result<(), InstallerError> {
    let temporary_link = link.with_file_name(format!(
        ".{}.tmp",
        link.file_name()
            .and_then(|file_name| file_name.to_str())
            .unwrap_or("SKILL.md")
    ));

    if temporary_link.exists() || fs::symlink_metadata(&temporary_link).is_ok() {
        fs::remove_file(&temporary_link)?;
    }

    create_skill_symlink(&temporary_link, target)?;
    fs::rename(&temporary_link, link)?;
    Ok(())
}

fn create_skill_symlink(link: &Path, target: &Path) -> Result<(), InstallerError> {
    if let Some(parent) = link.parent() {
        fs::create_dir_all(parent)?;
    }

    platform_symlink(target, link).map_err(|source| InstallerError::SymlinkFailed {
        link: link.to_path_buf(),
        target: target.to_path_buf(),
        source,
    })
}

#[cfg(unix)]
fn platform_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn platform_symlink(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_file(target, link)
}

#[cfg(not(any(unix, windows)))]
fn platform_symlink(_target: &Path, _link: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "skill symlinks are only supported on Unix and Windows",
    ))
}

fn symlink_points_to(link: &Path, target: &Path) -> bool {
    let Ok(raw_target) = fs::read_link(link) else {
        return false;
    };

    let normalized_link = normalize_path(link);
    let normalized_target_path = normalize_path(target);
    let resolved_target = if raw_target.is_absolute() {
        raw_target
    } else {
        link.parent()
            .unwrap_or_else(|| Path::new(""))
            .join(raw_target)
    };

    if normalize_path(&resolved_target) == normalized_target_path {
        return true;
    }

    if normalized_link == normalized_target_path {
        return false;
    }

    match (fs::canonicalize(link), fs::canonicalize(target)) {
        (Ok(link_canonical), Ok(target_canonical)) => link_canonical == target_canonical,
        _ => false,
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }

    normalized
}

fn frontmatter_name_is_codebones(content: &str) -> bool {
    let mut lines = content.lines();

    if lines.next().map(str::trim) != Some("---") {
        return false;
    }

    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            return false;
        }

        if let Some(value) = trimmed.strip_prefix("name:") {
            return unquote_yaml_scalar(value.trim()) == CODEBONES_SKILL_NAME;
        }
    }

    false
}

fn unquote_yaml_scalar(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}
