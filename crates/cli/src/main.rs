use anyhow::{bail, Result};
use clap::{Parser, Subcommand, ValueEnum};
use codebones_core::installer::{
    apply_init_actions, FileActionStatus, InitAction, InitContext, InstallMethod, McpTargetKind,
    SkillTargetKind,
};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "codebones", version, about = "Strip codebases down to their structural skeleton", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Builds or updates the SQLite cache for the given directory
    Index {
        /// The directory to index (defaults to current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,
    },
    /// Prints the skeleton of a specific indexed file
    Outline {
        /// The repository directory containing the index (defaults to current directory)
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// The path to a file in the indexed repository
        path: PathBuf,
    },
    /// Retrieves the full source code for a specific symbol or file
    Get {
        /// The repository directory containing the index (defaults to current directory)
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// The symbol name (e.g., `src/main.rs::Database.connect`) or file path
        symbol_or_path: String,
        /// Return only lines matching this pattern (case-insensitive), with 1 line of context
        #[arg(long)]
        filter: Option<String>,
    },
    /// Searches for symbols by name substring. Use an empty string ("") to list all indexed symbols.
    /// Note: % and _ are treated as literals, not wildcards.
    Search {
        /// The repository directory containing the index (defaults to current directory)
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// Substring to match against symbol names. Pass "" to list all symbols.
        query: String,
    },
    /// Outputs the skeleton map only (file paths + symbol signatures) — shorthand for pack --no-files
    Map {
        /// The directory to map (defaults to current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Output format (e.g., xml, markdown)
        #[arg(short, long, default_value = "xml")]
        format: String,
        /// Maximum tokens allowed in the output (0 = unlimited, default = 50000)
        #[arg(short, long, default_value = "50000")]
        max_tokens: usize,
        /// Remove all comments from the code
        #[arg(long)]
        remove_comments: bool,
        /// Remove consecutive empty lines
        #[arg(long)]
        remove_empty_lines: bool,
        /// Truncate long base64/hex strings in the output
        #[arg(long)]
        truncate_base64: bool,
        /// Glob patterns to explicitly include (e.g., "**/*.rs")
        #[arg(long)]
        include: Option<Vec<String>>,
        /// Glob patterns to ignore (e.g., "**/test_*")
        #[arg(long)]
        ignore: Option<Vec<String>>,
    },
    /// Shows the import dependency graph or blast radius for a specific file
    Graph {
        /// File to show blast radius for (omit for full graph)
        file: Option<String>,
        /// The directory containing the index (defaults to current directory)
        #[arg(long, default_value = ".")]
        dir: PathBuf,
        /// Output format (markdown, xml, json)
        #[arg(short, long, default_value = "markdown")]
        format: String,
        /// Show only the top N most-imported files (0 = unlimited, default = 50)
        #[arg(long, default_value = "50")]
        top: usize,
        /// Maximum blast radius depth (default: 3)
        #[arg(long, default_value = "3")]
        depth: usize,
    },
    /// Registers the codebones MCP server with AI tools installed on this machine
    Init {
        /// Override home directory (for testing)
        #[arg(long, hide = true)]
        home: Option<PathBuf>,
        /// Accept the default skill initialization plan without prompting
        #[arg(long)]
        yes: bool,
        /// Print planned changes without writing files
        #[arg(long)]
        dry_run: bool,
        /// Skill target to install; may be repeated
        #[arg(long = "skill-target", value_enum)]
        skill_targets: Vec<InitSkillTargetArg>,
        /// How to install selected skill targets
        #[arg(long = "skill-method", value_enum, default_value = "copy")]
        skill_method: InitSkillMethodArg,
        /// MCP target to register; may be repeated
        #[arg(long = "mcp-target", value_enum)]
        mcp_targets: Vec<InitMcpTargetArg>,
        /// Do not register MCP configuration
        #[arg(long)]
        no_mcp: bool,
    },
    /// Packs the repository's skeleton into a single string for LLM context
    Pack {
        /// The directory to pack (defaults to current directory)
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Output format (e.g., xml, markdown)
        #[arg(short, long, default_value = "xml")]
        format: String,
        /// Maximum tokens allowed in the output
        #[arg(short, long)]
        max_tokens: Option<usize>,
        /// Do not print the file summary/skeleton map at the top
        #[arg(long)]
        no_file_summary: bool,
        /// Only print the summary, do not print file contents
        #[arg(long)]
        no_files: bool,
        /// Remove all comments from the code
        #[arg(long)]
        remove_comments: bool,
        /// Remove consecutive empty lines
        #[arg(long)]
        remove_empty_lines: bool,
        /// Truncate long base64/hex strings in the output
        #[arg(long)]
        truncate_base64: bool,
        /// Glob patterns to explicitly include (e.g., "**/*.rs")
        #[arg(long)]
        include: Option<Vec<String>>,
        /// Glob patterns to ignore (e.g., "**/test_*")
        #[arg(long)]
        ignore: Option<Vec<String>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InitSkillTargetArg {
    ClaudeGlobal,
    ClaudeProject,
    CodexGlobal,
    UniversalProject,
    CursorProject,
    OpenCodeGlobal,
    GeminiProject,
    DroidGlobal,
    PiGlobal,
    All,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InitSkillMethodArg {
    Copy,
    Symlink,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum InitMcpTargetArg {
    ClaudeGlobal,
    CursorGlobal,
    All,
    None,
}

fn format_graph(
    result: &codebones_core::api::GraphResult,
    format: &str,
    unlimited: bool,
) -> String {
    match format {
        "json" => {
            let files_json: Vec<String> = result
                .files
                .iter()
                .map(|f| {
                    format!(
                        r#"{{"path":"{}","import_count":{}}}"#,
                        escape_json(&f.path),
                        f.import_count
                    )
                })
                .collect();
            let edges_json: Vec<String> = result
                .edges
                .iter()
                .map(|e| {
                    format!(
                        r#"{{"from":"{}","to":"{}"}}"#,
                        escape_json(&e.from),
                        escape_json(&e.to)
                    )
                })
                .collect();
            format!(
                r#"{{"files":[{}],"edges":[{}]}}"#,
                files_json.join(","),
                edges_json.join(",")
            )
        }
        "xml" => {
            let mut out = String::from("<graph>\n<files>\n");
            for f in &result.files {
                out.push_str(&format!(
                    "  <file path=\"{}\" import_count=\"{}\"/>\n",
                    escape_xml(&f.path),
                    f.import_count
                ));
            }
            out.push_str("</files>\n<edges>\n");
            for e in &result.edges {
                out.push_str(&format!(
                    "  <edge from=\"{}\" to=\"{}\"/>\n",
                    escape_xml(&e.from),
                    escape_xml(&e.to)
                ));
            }
            out.push_str("</edges>\n</graph>");
            out
        }
        _ => {
            // markdown (default)
            let mut out = String::from("# Import Graph\n\n## Most Imported Files\n");
            for f in &result.files {
                out.push_str(&format!(
                    "- `{}` — imported by **{}** files\n",
                    f.path, f.import_count
                ));
            }
            // Only include the import map if we're showing all files (no top filter)
            if unlimited {
                out.push_str("\n## Import Map\n");
                for e in &result.edges {
                    out.push_str(&format!("- `{}` → {}\n", e.from, e.to));
                }
            }
            out
        }
    }
}

fn format_blast_radius(
    file_path: &str,
    affected: &[codebones_core::api::AffectedFile],
    format: &str,
) -> String {
    match format {
        "json" => {
            let files_json: Vec<String> = affected
                .iter()
                .map(|f| {
                    let imports_json: Vec<String> = f
                        .imports
                        .iter()
                        .map(|i| format!("\"{}\"", escape_json(i)))
                        .collect();
                    format!(
                        r#"{{"path":"{}","imports":[{}]}}"#,
                        escape_json(&f.path),
                        imports_json.join(",")
                    )
                })
                .collect();
            format!(
                r#"{{"file":"{}","affected_files":[{}]}}"#,
                escape_json(file_path),
                files_json.join(",")
            )
        }
        "xml" => {
            let mut out = String::from("<blast_radius>\n");
            out.push_str(&format!("  <file>{}</file>\n", escape_xml(file_path)));
            out.push_str("  <affected>\n");
            for f in affected {
                out.push_str(&format!("    <file>{}</file>\n", escape_xml(&f.path)));
            }
            out.push_str("  </affected>\n</blast_radius>");
            out
        }
        _ => {
            // markdown (default)
            let mut out = format!(
                "# Blast Radius: {}\n\n## Affected Files ({})\n",
                file_path,
                affected.len()
            );
            for f in affected {
                out.push_str(&format!("- {}\n", f.path));
                if !f.imports.is_empty() {
                    out.push_str(&format!("  imports: {}\n", f.imports.join(", ")));
                }
            }
            out
        }
    }
}

fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn default_init_skill_actions(home_dir: &Path) -> Vec<InitAction> {
    let mut actions = vec![InitAction::InstallSkill {
        target: SkillTargetKind::CodexGlobal,
        method: InstallMethod::Copy,
    }];

    if home_dir.join(".claude").exists() {
        actions.push(InitAction::InstallSkill {
            target: SkillTargetKind::ClaudeGlobal,
            method: InstallMethod::Copy,
        });
    }

    actions
}

fn skill_targets_from_args(targets: &[InitSkillTargetArg]) -> Result<Vec<SkillTargetKind>> {
    if targets.contains(&InitSkillTargetArg::None) && targets.len() > 1 {
        bail!("conflict: --skill-target none cannot be combined with other skill targets");
    }

    let mut selected = Vec::new();
    for target in targets {
        match target {
            InitSkillTargetArg::ClaudeGlobal => selected.push(SkillTargetKind::ClaudeGlobal),
            InitSkillTargetArg::ClaudeProject => selected.push(SkillTargetKind::ClaudeProject),
            InitSkillTargetArg::CodexGlobal => selected.push(SkillTargetKind::CodexGlobal),
            InitSkillTargetArg::UniversalProject => {
                selected.push(SkillTargetKind::UniversalProject)
            }
            InitSkillTargetArg::CursorProject => selected.push(SkillTargetKind::CursorProject),
            InitSkillTargetArg::OpenCodeGlobal => selected.push(SkillTargetKind::OpenCodeGlobal),
            InitSkillTargetArg::GeminiProject => selected.push(SkillTargetKind::GeminiProject),
            InitSkillTargetArg::DroidGlobal => selected.push(SkillTargetKind::DroidGlobal),
            InitSkillTargetArg::PiGlobal => selected.push(SkillTargetKind::PiGlobal),
            InitSkillTargetArg::All => {
                selected.extend([
                    SkillTargetKind::ClaudeGlobal,
                    SkillTargetKind::ClaudeProject,
                    SkillTargetKind::CodexGlobal,
                    SkillTargetKind::UniversalProject,
                    SkillTargetKind::CursorProject,
                    SkillTargetKind::OpenCodeGlobal,
                    SkillTargetKind::GeminiProject,
                    SkillTargetKind::DroidGlobal,
                    SkillTargetKind::PiGlobal,
                ]);
            }
            InitSkillTargetArg::None => {}
        }
    }

    Ok(selected)
}

fn mcp_targets_from_args(targets: &[InitMcpTargetArg], no_mcp: bool) -> Result<Vec<McpTargetKind>> {
    if no_mcp
        && targets
            .iter()
            .any(|target| !matches!(target, InitMcpTargetArg::None))
    {
        bail!("conflict: --mcp-target cannot be combined with --no-mcp");
    }

    if targets.contains(&InitMcpTargetArg::None) && targets.len() > 1 {
        bail!("conflict: --mcp-target none cannot be combined with other MCP targets");
    }

    if no_mcp {
        return Ok(Vec::new());
    }

    let mut selected = Vec::new();
    for target in targets {
        match target {
            InitMcpTargetArg::ClaudeGlobal => selected.push(McpTargetKind::ClaudeGlobal),
            InitMcpTargetArg::CursorGlobal => selected.push(McpTargetKind::CursorGlobal),
            InitMcpTargetArg::All => {
                selected.extend([McpTargetKind::ClaudeGlobal, McpTargetKind::CursorGlobal]);
            }
            InitMcpTargetArg::None => {}
        }
    }

    Ok(selected)
}

fn install_method_from_arg(method: InitSkillMethodArg) -> InstallMethod {
    match method {
        InitSkillMethodArg::Copy => InstallMethod::Copy,
        InitSkillMethodArg::Symlink => InstallMethod::Symlink,
    }
}

fn confirm_default_init_plan() -> Result<bool> {
    eprint!("Install default codebones skills? [y/N] ");
    io::stderr().flush()?;

    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes" | "YES" | "Yes"))
}

fn skill_target_label(target: SkillTargetKind) -> &'static str {
    match target {
        SkillTargetKind::ClaudeGlobal => "Claude global skill",
        SkillTargetKind::ClaudeProject => "Claude project skill",
        SkillTargetKind::CodexGlobal => "Codex global skill",
        SkillTargetKind::UniversalProject => "universal project skill",
        SkillTargetKind::CursorProject => "Cursor project skill",
        SkillTargetKind::OpenCodeGlobal => "OpenCode global skill",
        SkillTargetKind::GeminiProject => "Gemini project skill",
        SkillTargetKind::DroidGlobal => "Droid global skill",
        SkillTargetKind::PiGlobal => "Pi global skill",
    }
}

fn mcp_target_label(target: McpTargetKind) -> &'static str {
    match target {
        McpTargetKind::ClaudeGlobal => "Claude MCP config",
        McpTargetKind::CursorGlobal => "Cursor MCP config",
    }
}

fn install_method_label(method: InstallMethod) -> &'static str {
    match method {
        InstallMethod::Copy => "copy",
        InstallMethod::Symlink => "symlink",
    }
}

fn status_label(status: FileActionStatus) -> &'static str {
    match status {
        FileActionStatus::Created => "created",
        FileActionStatus::Updated => "updated",
        FileActionStatus::Unchanged => "unchanged",
        FileActionStatus::ReplacedCodebonesOwned => "replaced codebones-owned file",
        FileActionStatus::WouldCreate => "would create",
        FileActionStatus::WouldUpdate => "would update",
        FileActionStatus::WouldReplaceCodebonesOwned => "would replace codebones-owned file",
        FileActionStatus::WouldLeaveUnchanged => "would leave unchanged",
    }
}

fn init_action_label(action: InitAction) -> String {
    match action {
        InitAction::MaterializeCanonicalSkill => "canonical skill".to_string(),
        InitAction::InstallSkill { target, method } => {
            format!(
                "{} ({})",
                skill_target_label(target),
                install_method_label(method)
            )
        }
        InitAction::RegisterMcp { target } => mcp_target_label(target).to_string(),
    }
}

fn format_init_status(action: InitAction, status: FileActionStatus, dry_run: bool) -> String {
    let message = format!("{}: {}", init_action_label(action), status_label(status));
    if dry_run {
        format!("dry-run: {}", message)
    } else {
        message
    }
}

fn remove_skill_target_actions(actions: &mut Vec<InitAction>, selected_target: SkillTargetKind) {
    actions.retain(|action| {
        !matches!(
            action,
            InitAction::InstallSkill { target, .. } if *target == selected_target
        )
    });
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init {
            home,
            yes,
            dry_run,
            skill_targets,
            skill_method,
            mcp_targets,
            no_mcp,
        } => {
            let home_dir = home
                .unwrap_or_else(|| dirs::home_dir().expect("Could not determine home directory"));
            let has_explicit_skill_targets = !skill_targets.is_empty();
            let has_explicit_mcp_targets = !mcp_targets.is_empty();
            let has_explicit_targets = has_explicit_skill_targets || has_explicit_mcp_targets;

            if !yes && !dry_run && !has_explicit_targets && !io::stdin().is_terminal() {
                bail!(
                    "non-interactive init requires --yes, --dry-run, or explicit --skill-target/--mcp-target targets"
                );
            }

            let explicit_skill_targets = skill_targets_from_args(&skill_targets)?;
            let mut use_default_skills = yes && !has_explicit_skill_targets;
            if !has_explicit_targets && dry_run {
                use_default_skills = true;
            }
            if !yes && !dry_run && !has_explicit_targets {
                use_default_skills = confirm_default_init_plan()?;
                if !use_default_skills {
                    println!("Init cancelled");
                    return Ok(());
                }
            }

            let method = install_method_from_arg(skill_method);
            let mut actions = Vec::new();

            if use_default_skills {
                actions.extend(default_init_skill_actions(&home_dir));
            }
            if has_explicit_skill_targets {
                for target in explicit_skill_targets {
                    remove_skill_target_actions(&mut actions, target);
                    actions.push(InitAction::InstallSkill { target, method });
                }
            }

            actions.extend(
                mcp_targets_from_args(&mcp_targets, no_mcp)?
                    .into_iter()
                    .map(|target| InitAction::RegisterMcp { target }),
            );

            if actions.is_empty() {
                println!("No init actions selected");
                return Ok(());
            }

            let ctx = InitContext {
                home_dir,
                project_dir: std::env::current_dir()?,
            };
            let results = apply_init_actions(&ctx, &actions, dry_run)?;

            for (action, status) in results {
                println!("{}", format_init_status(action, status, dry_run));
            }
        }
        Commands::Map {
            dir,
            format,
            max_tokens,
            remove_comments,
            remove_empty_lines,
            truncate_base64,
            include,
            ignore,
        } => {
            // max_tokens == 0 means unlimited full-content pack; any positive value
            // means skeleton-only output capped at that token budget.
            let (no_files, token_limit) = if max_tokens == 0 {
                (false, None)
            } else {
                (true, Some(max_tokens))
            };
            let result = codebones_core::api::pack(
                &dir,
                &format,
                token_limit,
                codebones_core::api::PackOptions {
                    no_file_summary: false,
                    no_files,
                    remove_comments,
                    remove_empty_lines,
                    truncate_base64,
                    include,
                    ignore,
                },
            )?;
            println!("{}", result);
        }
        Commands::Index { dir } => {
            codebones_core::api::index(&dir)?;
            println!("Indexing complete");
        }
        Commands::Outline { dir, path } => {
            let result = codebones_core::api::outline(&dir, &path.to_string_lossy())?;
            println!("{}", result);
        }
        Commands::Get {
            dir,
            symbol_or_path,
            filter,
        } => {
            let result = codebones_core::api::get(&dir, &symbol_or_path, filter.as_deref())?;
            println!("{}", result);
        }
        Commands::Search { dir, query } => {
            let results = codebones_core::api::search(&dir, &query)?;
            for res in results {
                println!("{}", res);
            }
        }
        Commands::Graph {
            file,
            dir,
            format,
            top,
            depth,
        } => {
            if let Some(file_path) = file {
                // Blast radius mode — top default does not apply
                let result = codebones_core::api::graph_file(&dir, &file_path, depth)?;
                let output = format_blast_radius(&file_path, &result.affected_files, &format);
                println!("{}", output);
            } else {
                // Full graph mode — top=0 means unlimited, otherwise truncate
                let mut graph_result = codebones_core::api::graph(&dir)?;
                let unlimited = top == 0;
                if !unlimited {
                    graph_result.files.truncate(top);
                }
                let output = format_graph(&graph_result, &format, unlimited);
                println!("{}", output);
            }
        }
        Commands::Pack {
            dir,
            format,
            max_tokens,
            no_file_summary,
            no_files,
            remove_comments,
            remove_empty_lines,
            truncate_base64,
            include,
            ignore,
        } => {
            let result = codebones_core::api::pack(
                &dir,
                &format,
                max_tokens,
                codebones_core::api::PackOptions {
                    no_file_summary,
                    no_files,
                    remove_comments,
                    remove_empty_lines,
                    truncate_base64,
                    include,
                    ignore,
                },
            )?;
            println!("{}", result);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use assert_cmd::Command;
    use std::fs;
    use tempfile::TempDir;

    fn setup_e2e_repo() -> TempDir {
        let temp = TempDir::new().unwrap();
        let root = temp.path();
        let rs_content = r#"pub fn greet_user(name: &str) -> String {
    format!("Hello, {}!", name)
}
"#;
        fs::write(root.join("greet.rs"), rs_content).unwrap();
        temp
    }

    #[test]
    fn test_cli_index_and_get_e2e() {
        let temp = setup_e2e_repo();
        let root = temp.path();

        // Index the directory
        Command::cargo_bin("codebones")
            .unwrap()
            .current_dir(root)
            .args(["index", "."])
            .assert()
            .success();

        // Get the file by path — the DB stores relative paths, run from root so "." resolves
        let output = Command::cargo_bin("codebones")
            .unwrap()
            .current_dir(root)
            .args(["get", "greet.rs"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let stdout = String::from_utf8_lossy(&output);
        assert!(
            stdout.contains("greet_user"),
            "Expected function name in get output, got: {}",
            stdout
        );
        assert!(
            stdout.contains("Hello"),
            "Expected function body in get output, got: {}",
            stdout
        );
    }

    #[test]
    fn test_cli_pack_format() {
        let temp = setup_e2e_repo();
        let root = temp.path();

        // Index first (pack also re-indexes, but be explicit)
        Command::cargo_bin("codebones")
            .unwrap()
            .current_dir(root)
            .args(["index", "."])
            .assert()
            .success();

        // XML format
        let xml_output = Command::cargo_bin("codebones")
            .unwrap()
            .current_dir(root)
            .args(["pack", ".", "--format", "xml"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let xml_str = String::from_utf8_lossy(&xml_output);
        assert!(
            xml_str.contains("<repository>"),
            "XML output missing <repository>: {}",
            xml_str
        );
        assert!(
            xml_str.contains("<skeleton_map>"),
            "XML output missing <skeleton_map>: {}",
            xml_str
        );
        assert!(
            xml_str.contains("<content>"),
            "XML output missing <content>: {}",
            xml_str
        );
        assert!(
            xml_str.contains("</repository>"),
            "XML output missing </repository>: {}",
            xml_str
        );

        // Markdown format
        let md_output = Command::cargo_bin("codebones")
            .unwrap()
            .current_dir(root)
            .args(["pack", ".", "--format", "markdown"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let md_str = String::from_utf8_lossy(&md_output);
        assert!(
            md_str.contains("## Skeleton Map"),
            "Markdown output missing '## Skeleton Map': {}",
            md_str
        );
    }

    #[test]
    fn test_cli_search_fts5() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Write a file with a uniquely named function
        let rs_content = r#"pub fn unique_function_xyz(x: i32) -> i32 {
    x * 2
}
"#;
        fs::write(root.join("unique.rs"), rs_content).unwrap();

        // Index
        Command::cargo_bin("codebones")
            .unwrap()
            .current_dir(root)
            .args(["index", "."])
            .assert()
            .success();

        // Search for the unique function — should find it
        let found_output = Command::cargo_bin("codebones")
            .unwrap()
            .current_dir(root)
            .args(["search", "unique_function_xyz"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let found_str = String::from_utf8_lossy(&found_output);
        assert!(
            found_str.contains("unique_function_xyz"),
            "Search should return the unique function, got: {}",
            found_str
        );

        // Search for something that does not exist — should succeed with empty output
        let empty_output = Command::cargo_bin("codebones")
            .unwrap()
            .current_dir(root)
            .args(["search", "this_function_does_not_exist_anywhere"])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let empty_str = String::from_utf8_lossy(&empty_output);
        assert!(
            empty_str.trim().is_empty(),
            "Search with no results should produce empty output, got: {}",
            empty_str
        );
    }
}
