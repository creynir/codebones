use crate::cache::{CacheStore, SqliteCache};
use crate::parser::Bone;
use crate::parser::Parser;
use anyhow::Result;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// Regex::new is called inside OnceLock::get_or_init, which guarantees compilation at most once.
// Clippy cannot see through the OnceLock abstraction and fires regex_creation_in_loops.
#[allow(clippy::regex_creation_in_loops)]
static RE_EMPTY_LINES: OnceLock<regex::Regex> = OnceLock::new();
#[allow(clippy::regex_creation_in_loops)]
static RE_BASE64: OnceLock<regex::Regex> = OnceLock::new();
#[allow(clippy::regex_creation_in_loops)]
static RE_LINE_COMMENT: OnceLock<regex::Regex> = OnceLock::new();
#[allow(clippy::regex_creation_in_loops)]
static RE_BLOCK_COMMENT: OnceLock<regex::Regex> = OnceLock::new();

/// A plugin that can enrich extracted code bones with domain-specific metadata.
pub trait ContextPlugin: Send + Sync {
    /// The unique name of the plugin (e.g., "dbt", "openapi").
    fn name(&self) -> &str;

    /// Returns true if this plugin should be active for the given directory/workspace.
    fn detect(&self, directory: &Path) -> bool;

    /// Enriches the extracted bones for a specific file with additional metadata.
    /// The plugin can modify the `base_bones` in place (e.g., adding JSON metadata).
    fn enrich(&self, file_path: &Path, base_bones: &mut Vec<Bone>) -> Result<()>;
}

/// Supported output formats for the packed context.
pub enum OutputFormat {
    Xml,
    Markdown,
}

impl OutputFormat {
    pub fn parse(format: &str) -> Result<Self> {
        match format.to_lowercase().as_str() {
            "xml" => Ok(Self::Xml),
            "markdown" => Ok(Self::Markdown),
            other => anyhow::bail!("Invalid output format: {other}. Expected 'xml' or 'markdown'"),
        }
    }
}

/// Bundles files and their enriched bones into an AI-friendly output format.
pub struct Packer {
    cache: SqliteCache,
    parser: Parser,
    workspace_root: PathBuf,
    plugins: Vec<Box<dyn ContextPlugin>>,
    format: OutputFormat,
    max_tokens: Option<usize>,
    no_file_summary: bool,
    no_files: bool,
    remove_comments: bool,
    remove_empty_lines: bool,
    truncate_base64: bool,
}

impl Packer {
    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }

    fn xml_escape_cdata(s: &str) -> String {
        // Split ]]> into ]]]]><![CDATA[> to keep it inside CDATA
        s.replace("]]>", "]]]]><![CDATA[>")
    }

    /// Markdown structure (headings, list items, fences) can only be forged at
    /// a line start, so replacing control characters with spaces is sufficient
    /// to keep repo-controlled strings (paths, symbol names, plugin metadata)
    /// from injecting structure into the packed output. File content is
    /// fence-protected separately.
    fn markdown_sanitize(s: &str) -> String {
        s.chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect()
    }

    /// Renders a path with `/` separators on every platform so packed output is
    /// portable and deterministic (Windows `path.display()` would otherwise emit
    /// `.\dummy.rs`). On Unix `\` is a legal filename character and is left alone.
    fn display_path(path: &Path) -> String {
        let s = path.display().to_string();
        if cfg!(windows) {
            s.replace('\\', "/")
        } else {
            s
        }
    }

    /// Creates a new Packer instance.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cache: SqliteCache,
        parser: Parser,
        format: OutputFormat,
        max_tokens: Option<usize>,
        no_file_summary: bool,
        no_files: bool,
        remove_comments: bool,
        remove_empty_lines: bool,
        truncate_base64: bool,
    ) -> Self {
        Self::with_workspace_root(
            cache,
            parser,
            PathBuf::from("."),
            format,
            max_tokens,
            no_file_summary,
            no_files,
            remove_comments,
            remove_empty_lines,
            truncate_base64,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_workspace_root(
        cache: SqliteCache,
        parser: Parser,
        workspace_root: PathBuf,
        format: OutputFormat,
        max_tokens: Option<usize>,
        no_file_summary: bool,
        no_files: bool,
        remove_comments: bool,
        remove_empty_lines: bool,
        truncate_base64: bool,
    ) -> Self {
        let _ = cache.init();
        Self {
            cache,
            parser,
            workspace_root,
            plugins: Vec::new(),
            format,
            max_tokens,
            no_file_summary,
            no_files,
            remove_comments,
            remove_empty_lines,
            truncate_base64,
        }
    }

    /// Registers a context plugin.
    pub fn register_plugin(&mut self, plugin: Box<dyn ContextPlugin>) {
        self.plugins.push(plugin);
    }

    /// Packs the specified files into a single formatted string.
    // OnceLock::get_or_init guarantees each regex is compiled at most once.
    // Clippy fires regex_creation_in_loops because it cannot see through the OnceLock
    // abstraction — the allow is intentional and correct.
    #[allow(clippy::regex_creation_in_loops)]
    pub fn pack(&self, file_paths: &[PathBuf]) -> Result<String> {
        let _ = &self.parser;

        let mut output = String::new();
        let active_plugins: Vec<&dyn ContextPlugin> = self
            .plugins
            .iter()
            .filter(|plugin| plugin.detect(&self.workspace_root))
            .map(|plugin| plugin.as_ref())
            .collect();

        match self.format {
            OutputFormat::Xml => output.push_str("<repository>\n"),
            OutputFormat::Markdown => {}
        }

        let lookup_symbols = |path: &PathBuf| -> Result<Vec<(String, String)>> {
            let relative_path = path
                .strip_prefix(&self.workspace_root)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            self.cache
                .get_file_symbols(&relative_path)
                .map_err(Into::into)
        };

        // Generate Skeleton Map
        // Track which files made it into the skeleton so the body loop can skip the rest.
        let mut included_files: HashSet<PathBuf> = HashSet::new();
        if !self.no_file_summary {
            match self.format {
                OutputFormat::Xml => {
                    let bpe = if self.max_tokens.is_some() {
                        Some(tiktoken_rs::cl100k_base().map_err(|e| {
                            anyhow::anyhow!("Failed to initialize tokenizer: {}", e)
                        })?)
                    } else {
                        None
                    };
                    // Running token count: track only the entry tokens accumulated so far.
                    // Structural framing tokens (<repository>, <skeleton_map>, etc.) are
                    // small overhead that fits within the allowed tolerance, so we don't
                    // count them here — this lets the budget check focus on file entries.
                    let skeleton_open = "  <skeleton_map>\n";
                    let skeleton_close = "  </skeleton_map>\n";
                    let mut running_tokens: usize = 0;
                    output.push_str(skeleton_open);
                    for path in file_paths {
                        let mut entry = format!(
                            "    <file path=\"{}\">\n",
                            Self::xml_escape(&Self::display_path(path))
                        );
                        for (kind, name) in lookup_symbols(path)? {
                            entry.push_str(&format!(
                                "      <signature>{} {}</signature>\n",
                                Self::xml_escape(&kind),
                                Self::xml_escape(&name)
                            ));
                        }
                        entry.push_str("    </file>\n");

                        if let (Some(ref bpe), Some(max)) = (&bpe, self.max_tokens) {
                            let entry_tokens = bpe.encode_with_special_tokens(&entry).len();
                            if running_tokens + entry_tokens > max {
                                break; // budget exhausted — drop remaining files
                            }
                            running_tokens += entry_tokens;
                        }

                        output.push_str(&entry);
                        included_files.insert(path.clone());
                    }
                    output.push_str(skeleton_close);
                }
                OutputFormat::Markdown => {
                    let bpe = if self.max_tokens.is_some() {
                        Some(tiktoken_rs::cl100k_base().map_err(|e| {
                            anyhow::anyhow!("Failed to initialize tokenizer: {}", e)
                        })?)
                    } else {
                        None
                    };
                    let header = "## Skeleton Map\n\n";
                    // Same approach: track only entry tokens, not structural framing.
                    let mut running_tokens: usize = 0;
                    output.push_str(header);
                    for path in file_paths {
                        let mut entry =
                            format!("- {}\n", Self::markdown_sanitize(&Self::display_path(path)));
                        for (kind, name) in lookup_symbols(path)? {
                            entry.push_str(&format!(
                                "  - {} {}\n",
                                Self::markdown_sanitize(&kind),
                                Self::markdown_sanitize(&name)
                            ));
                        }

                        if let (Some(ref bpe), Some(max)) = (&bpe, self.max_tokens) {
                            let entry_tokens = bpe.encode_with_special_tokens(&entry).len();
                            if running_tokens + entry_tokens > max {
                                break;
                            }
                            running_tokens += entry_tokens;
                        }

                        output.push_str(&entry);
                        included_files.insert(path.clone());
                    }
                    output.push('\n');
                }
            }
        }

        if self.no_files {
            if let OutputFormat::Xml = self.format {
                output.push_str("</repository>\n");
            }
            return Ok(output);
        }

        let bpe = tiktoken_rs::cl100k_base()
            .map_err(|e| anyhow::anyhow!("Failed to initialize tokenizer: {}", e))?;
        let mut degrade_to_bones = false;

        for path in file_paths {
            // If the skeleton map was truncated, skip files that didn't make the cut.
            if !included_files.is_empty() && !included_files.contains(path) {
                continue;
            }

            let mut raw_content = match std::fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!(
                        "Warning: skipping unreadable file {}: {}",
                        path.display(),
                        e
                    );
                    continue;
                }
            };

            if self.remove_empty_lines {
                raw_content = RE_EMPTY_LINES
                    .get_or_init(|| {
                        regex::Regex::new(r"\n\s*\n").expect("valid static regex: empty lines")
                    })
                    .replace_all(&raw_content, "\n")
                    .to_string();
            }

            if self.truncate_base64 {
                // Truncate long hex or base64 looking strings (length > 100)
                raw_content = RE_BASE64
                    .get_or_init(|| {
                        regex::Regex::new(r"[A-Za-z0-9+/=]{100,}")
                            .expect("valid static regex: base64")
                    })
                    .replace_all(&raw_content, "[TRUNCATED_BASE64]")
                    .to_string();
            }

            // Generate the skeleton by eliding function/class bodies
            let content = {
                let ext = path.extension().unwrap_or_default().to_string_lossy();
                if let Some(spec) = crate::parser::get_spec_for_extension(&ext) {
                    let doc = crate::parser::parse_file(&raw_content, &spec);
                    let mut result = String::new();
                    let mut last_end = 0;

                    let mut indices: Vec<usize> = (0..doc.symbols.len()).collect();
                    indices.sort_by_key(|&i| doc.symbols[i].full_range.start);

                    for i in &indices {
                        let sym = &doc.symbols[*i];
                        if let Some(body_range) = &sym.body_range {
                            if body_range.start >= last_end {
                                result.push_str(&raw_content[last_end..body_range.start]);
                                result.push_str("...");
                                last_end = body_range.end;
                            }
                        }
                    }
                    result.push_str(&raw_content[last_end..]);

                    if self.remove_comments {
                        // Simple regex fallback for comments (C-style, Python, HTML)
                        result = RE_BLOCK_COMMENT
                            .get_or_init(|| {
                                regex::Regex::new(r"(?s)/\*.*?\*/|<!--.*?-->")
                                    .expect("valid static regex: block comment")
                            })
                            .replace_all(&result, "")
                            .to_string();
                        result = RE_LINE_COMMENT
                            .get_or_init(|| {
                                regex::Regex::new(r"(?m)(//|#).*\n")
                                    .expect("valid static regex: line comment")
                            })
                            .replace_all(&result, "\n")
                            .to_string();
                    }

                    result
                } else {
                    if self.remove_comments {
                        let no_blocks = RE_BLOCK_COMMENT
                            .get_or_init(|| {
                                regex::Regex::new(r"(?s)/\*.*?\*/|<!--.*?-->")
                                    .expect("valid static regex: block comment")
                            })
                            .replace_all(&raw_content, "")
                            .to_string();
                        RE_LINE_COMMENT
                            .get_or_init(|| {
                                regex::Regex::new(r"(?m)(//|#).*\n")
                                    .expect("valid static regex: line comment")
                            })
                            .replace_all(&no_blocks, "\n")
                            .to_string()
                    } else {
                        raw_content.clone() // Fallback to raw content if language isn't supported
                    }
                }
            };

            let mut bones = vec![Bone::default()];

            for plugin in &active_plugins {
                plugin.enrich(path, &mut bones)?;
            }

            if !degrade_to_bones {
                if let Some(max) = self.max_tokens {
                    let current_tokens = bpe.encode_with_special_tokens(&output).len();
                    let content_tokens = bpe.encode_with_special_tokens(&content).len();
                    // Also account for the file wrapper and closing repo tag that will
                    // be added around the content, so the budget check is conservative.
                    let wrapper = match self.format {
                        OutputFormat::Xml => {
                            format!(
                                "  <file path=\"{}\">\n    <content><![CDATA[\n\n]]></content>\n  </file>\n</repository>\n",
                                Self::xml_escape(&Self::display_path(path))
                            )
                        }
                        OutputFormat::Markdown => {
                            format!("## {}\n\n```\n\n```\n\n", Self::display_path(path))
                        }
                    };
                    let wrapper_tokens = bpe.encode_with_special_tokens(&wrapper).len();
                    if current_tokens + content_tokens + wrapper_tokens > max {
                        degrade_to_bones = true;
                    }
                }
            }

            match self.format {
                OutputFormat::Xml => {
                    // When budget is exceeded, skip the file entry entirely.
                    // The skeleton map already surfaces these files — empty wrappers add no value
                    // and push the output over the token budget.
                    if degrade_to_bones {
                        continue;
                    }
                    output.push_str(&format!(
                        "  <file path=\"{}\">\n",
                        Self::xml_escape(&Self::display_path(path))
                    ));
                    {
                        let safe_content = Self::xml_escape_cdata(&content);
                        if safe_content == content {
                            output.push_str(&format!(
                                "    <content><![CDATA[\n{}\n]]></content>\n",
                                safe_content
                            ));
                        } else {
                            // Content contains ]]> which cannot be safely embedded in CDATA;
                            // fall back to XML entity escaping so the document stays well-formed.
                            output.push_str(&format!(
                                "    <content>{}</content>\n",
                                Self::xml_escape(&content)
                            ));
                        }
                    }
                    // Only print bones block if plugins added metadata
                    let has_metadata = bones.iter().any(|b| !b.metadata.is_empty());
                    if has_metadata {
                        output.push_str("    <bones>\n");
                        for bone in &bones {
                            for (k, v) in &bone.metadata {
                                output.push_str(&format!(
                                    "      <metadata key=\"{}\">{}</metadata>\n",
                                    Self::xml_escape(k),
                                    Self::xml_escape(v)
                                ));
                            }
                        }
                        output.push_str("    </bones>\n");
                    }
                    output.push_str("  </file>\n");
                }
                OutputFormat::Markdown => {
                    // When budget is exceeded, skip the file entry entirely.
                    if degrade_to_bones {
                        continue;
                    }
                    output.push_str(&format!(
                        "## {}\n\n",
                        Self::markdown_sanitize(&Self::display_path(path))
                    ));
                    {
                        // Find longest run of backticks in content and use one more as the fence
                        // delimiter (CommonMark spec approach) to prevent fence injection.
                        let max_backticks = {
                            let mut max = 0usize;
                            let mut cur = 0usize;
                            for c in content.chars() {
                                if c == '`' {
                                    cur += 1;
                                    max = max.max(cur);
                                } else {
                                    cur = 0;
                                }
                            }
                            max
                        };
                        let fence_len = max_backticks.max(2) + 1;
                        let fence = "`".repeat(fence_len);
                        // Break up any backtick run of length >= (fence_len - 1) within the
                        // content to prevent a closing-fence sequence from appearing verbatim.
                        // A zero-width space (U+200B) is inserted after the (fence_len-1)-th
                        // consecutive backtick so the run is interrupted while the characters
                        // remain visible to the reader.
                        let safe_content = if max_backticks >= fence_len - 1 {
                            let threshold = fence_len - 1;
                            let mut result = String::with_capacity(content.len());
                            let mut run = 0usize;
                            for c in content.chars() {
                                result.push(c);
                                if c == '`' {
                                    run += 1;
                                    if run == threshold {
                                        result.push('\u{200B}'); // zero-width space
                                        run = 0;
                                    }
                                } else {
                                    run = 0;
                                }
                            }
                            result
                        } else {
                            content.clone()
                        };
                        output.push_str(&format!("{}\n{}\n{}\n\n", fence, safe_content, fence));
                    }
                    // Only print Bones section if plugins added metadata
                    let has_metadata = bones.iter().any(|b| !b.metadata.is_empty());
                    if has_metadata {
                        output.push_str("Bones:\n");
                        for bone in &bones {
                            for (k, v) in &bone.metadata {
                                output.push_str(&format!(
                                    "- {}: {}\n",
                                    Self::markdown_sanitize(k),
                                    Self::markdown_sanitize(v)
                                ));
                            }
                        }
                        output.push('\n');
                    }
                }
            }
        }

        if let OutputFormat::Xml = self.format {
            output.push_str("</repository>\n");
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct MockPlugin;

    impl ContextPlugin for MockPlugin {
        fn name(&self) -> &str {
            "mock"
        }

        fn detect(&self, _directory: &Path) -> bool {
            true
        }

        fn enrich(&self, _file_path: &Path, base_bones: &mut Vec<Bone>) -> Result<()> {
            for bone in base_bones.iter_mut() {
                bone.metadata
                    .insert("injected".to_string(), "true".to_string());
            }
            Ok(())
        }
    }

    fn make_temp_rs_file(content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let file_path = dir.path().join("sample.rs");
        let mut f = std::fs::File::create(&file_path).expect("failed to create temp file");
        f.write_all(content.as_bytes())
            .expect("failed to write file content");
        (dir, file_path)
    }

    #[test]
    fn test_plugin_detect_and_enrich() {
        let plugin = MockPlugin;
        assert!(plugin.detect(Path::new(".")));
        let mut bones = vec![Bone::default()];
        plugin
            .enrich(Path::new("any_file.rs"), &mut bones)
            .expect("enrich should succeed");
        assert_eq!(
            bones[0]
                .metadata
                .get("injected")
                .expect("injected key must be present"),
            "true"
        );
    }

    #[test]
    fn test_packer_xml_format() {
        let (_dir, file_path) = make_temp_rs_file("fn main() {}\n");
        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        let result = packer.pack(&[file_path]);
        assert!(result.is_ok());
        let output = result.expect("pack should succeed");
        assert!(output.contains("<repository>"));
    }

    #[test]
    fn test_packer_markdown_format() {
        let (_dir, file_path) = make_temp_rs_file("fn main() {}\n");
        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Markdown,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        let result = packer.pack(std::slice::from_ref(&file_path));
        assert!(result.is_ok());
        let output = result.expect("pack should succeed");
        assert!(output.contains(&format!("## {}", Packer::display_path(&file_path))));
    }

    #[test]
    fn test_packer_with_plugins() {
        let (_dir, file_path) = make_temp_rs_file("fn main() {}\n");
        let mut packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        packer.register_plugin(Box::new(MockPlugin));
        let result = packer.pack(&[file_path]);
        assert!(result.is_ok());
        let output = result.expect("pack should succeed");
        assert!(output.contains("injected"));
    }

    #[test]
    fn test_packer_empty_file_list() {
        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        let result = packer.pack(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_packer_missing_file() {
        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        let result = packer.pack(&[PathBuf::from("this_file_does_not_exist_xyz.rs")]);
        // Missing files are skipped gracefully
        assert!(result.is_ok());
    }

    #[test]
    fn test_packer_generates_skeleton_map_at_top() {
        let (_dir, file_path) = make_temp_rs_file("fn main() {}\n");
        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        let result = packer.pack(&[file_path]);
        assert!(result.is_ok());
        let output = result.expect("pack should succeed");
        // The skeleton map should be at the top of the output
        assert!(output.starts_with("<repository>\n  <skeleton_map>"));
    }

    #[test]
    fn test_packer_token_governor_degrades_to_bones() {
        // Set a very low max_tokens to force degradation to bones-only output
        let (_dir, file_path) = make_temp_rs_file("fn main() { let x = 1; }\n");
        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            Some(10),
            false,
            false,
            false,
            false,
            false,
        );
        let result = packer.pack(&[file_path]);
        assert!(result.is_ok());
        let output = result.expect("pack should succeed");
        // When degraded to bones, full file content should not appear in output
        assert!(!output.contains("<content>"));
    }

    // -------------------------------------------------------------------------
    // Helper: create a temp file with a given extension
    // -------------------------------------------------------------------------
    fn make_temp_file(dir: &tempfile::TempDir, filename: &str, content: &str) -> PathBuf {
        let file_path = dir.path().join(filename);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).expect("failed to create parent directories");
        }
        let mut f = std::fs::File::create(&file_path).expect("failed to create temp file");
        f.write_all(content.as_bytes())
            .expect("failed to write file content");
        file_path
    }

    // =========================================================================
    // XML output correctness
    // =========================================================================

    /// Symbol names with XML special characters should be escaped in XML output.
    /// This test describes CORRECT behavior. The current implementation does NOT
    /// escape these characters in <signature> tags — so this test is expected to
    /// FAIL until the implementation is fixed.
    #[test]
    fn test_xml_signature_special_chars_are_escaped() {
        use crate::cache::CacheStore;

        let cache = SqliteCache::new_in_memory().expect("failed to create test cache");
        cache.init().expect("failed to init cache schema");

        // Insert a file + symbol with XML-dangerous characters in the name.
        let file_id = cache
            .upsert_file("bad.rs", "h1", b"fn bad() {}")
            .expect("upsert_file should succeed");
        cache
            .insert_symbol(&crate::cache::Symbol {
                id: "s1".to_string(),
                file_id,
                name: "<script>&\"test\"</script>".to_string(),
                kind: "function".to_string(),
                byte_offset: 0,
                byte_length: 11,
            })
            .expect("symbol insert should succeed");

        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let file_path = make_temp_file(&dir, "bad.rs", "fn bad() {}\n");

        let packer = Packer::with_workspace_root(
            cache,
            Parser {},
            dir.path().to_path_buf(),
            OutputFormat::Xml,
            None,
            false, // no_file_summary
            false, // no_files
            false,
            false,
            false,
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        // The raw unescaped characters must NOT appear outside of CDATA in XML attributes/tags.
        // Correct output would use &lt; &gt; &amp; &quot; instead.
        assert!(
            !output.contains("<script>"),
            "Bare <script> tag should not appear in XML output; expected escaped form"
        );
        assert!(
            output.contains("&lt;script&gt;") || output.contains("&amp;"),
            "XML special characters in symbol names must be escaped"
        );
    }

    /// File paths with XML special characters should be escaped in path attributes.
    /// This test describes CORRECT behavior and is expected to FAIL until fixed.
    #[test]
    fn test_xml_path_attribute_special_chars_are_escaped() {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        // Use a filename that contains an ampersand (legal on most filesystems).
        let file_path = make_temp_file(&dir, "a&b.txt", "hello world\n");

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        // The bare & must be escaped as &amp; in XML attributes.
        assert!(
            !output.contains("path=\"") || !output.contains("a&b.txt\""),
            "Bare & in path attribute must be escaped as &amp;"
        );
    }

    /// File content containing `]]>` inside a CDATA section must be escaped so
    /// the XML document stays well-formed.
    #[test]
    fn test_xml_cdata_cdata_end_sequence_is_escaped() {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        // Content that would prematurely close a CDATA section.
        let tricky = "let s = \"]]>\";\n";
        let file_path = make_temp_file(&dir, "tricky.txt", tricky);

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        // The raw ]]> sequence must not appear verbatim inside a CDATA section.
        // The implementation splits it as ]]]]><![CDATA[>.
        // After the transformation there should be no bare ]]> that closes CDATA prematurely.
        // A simple check: every ]]> in the output must be followed immediately by </content>
        // (i.e., it is the legitimate CDATA close).
        let positions: Vec<_> = output.match_indices("]]>").collect();
        for (idx, _) in &positions {
            let after = &output[idx + 3..];
            assert!(
                after.starts_with("</content>"),
                "Found ]]> at position {} that is not the CDATA closing sequence; \
                 raw content may break XML well-formedness",
                idx
            );
        }
    }

    /// A basic well-formedness check: the XML output should have balanced
    /// `<repository>` / `</repository>` tags and no bare `<` or `>` outside CDATA.
    #[test]
    fn test_xml_output_basic_well_formedness() {
        let (_dir, file_path) = make_temp_rs_file("fn main() {}\n");

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        assert!(
            output.starts_with("<repository>"),
            "XML output must start with <repository>"
        );
        assert!(
            output.trim_end().ends_with("</repository>"),
            "XML output must end with </repository>"
        );

        // Strip all CDATA sections before checking for bare angle brackets.
        let cdata_re =
            regex::Regex::new(r"(?s)<!\[CDATA\[.*?]]>").expect("failed to compile cdata regex");
        let stripped = cdata_re.replace_all(&output, "");

        // Any remaining < must be the start of a tag (followed by [/a-zA-Z!?])
        for (i, ch) in stripped.char_indices() {
            if ch == '<' {
                let next = stripped[i + 1..].chars().next();
                assert!(
                    matches!(next, Some('/' | '!' | '?' | 'a'..='z' | 'A'..='Z')),
                    "Bare < found at position {} outside of CDATA: ...{}...",
                    i,
                    &stripped[i.saturating_sub(10)..std::cmp::min(i + 20, stripped.len())]
                );
            }
        }
    }

    // =========================================================================
    // Markdown output correctness
    // =========================================================================

    /// Markdown skeleton map must indent symbol entries with two spaces under
    /// their parent file bullet.
    #[test]
    fn test_markdown_skeleton_map_indentation() {
        use crate::cache::CacheStore;

        let cache = SqliteCache::new_in_memory().expect("failed to create test cache");
        cache.init().expect("failed to init cache schema");

        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let file_path = make_temp_file(&dir, "lib.rs", "fn alpha() {}\n");

        let file_id = cache
            .upsert_file("lib.rs", "h2", b"fn alpha() {}")
            .expect("upsert_file should succeed");
        cache
            .insert_symbol(&crate::cache::Symbol {
                id: "s_alpha".to_string(),
                file_id,
                name: "alpha".to_string(),
                kind: "function".to_string(),
                byte_offset: 0,
                byte_length: 13,
            })
            .expect("symbol insert should succeed");

        let packer = Packer::with_workspace_root(
            cache,
            Parser {},
            dir.path().to_path_buf(),
            OutputFormat::Markdown,
            None,
            false,
            true, // no_files — only generate skeleton map
            false,
            false,
            false,
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        // The file should appear as a bullet: "- <path>"
        assert!(
            output.contains("- "),
            "File bullet not found in Markdown output"
        );

        // Each symbol under the file should be indented with two spaces: "  - kind name"
        assert!(
            output.contains("  - function alpha"),
            "Symbol entries in skeleton map must be indented with two spaces; got:\n{}",
            output
        );
    }

    /// Markdown symbol names containing *, _, [, ], ` should appear verbatim and
    /// must not break the overall Markdown skeleton structure (file bullet is still present).
    #[test]
    fn test_markdown_symbol_names_with_special_chars() {
        use crate::cache::CacheStore;

        let cache = SqliteCache::new_in_memory().expect("failed to create test cache");
        cache.init().expect("failed to init cache schema");

        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let file_path = make_temp_file(&dir, "weird.rs", "fn weird() {}\n");

        let file_id = cache
            .upsert_file("weird.rs", "h3", b"fn weird() {}")
            .expect("upsert_file should succeed");
        // Symbol name with markdown special characters
        cache
            .insert_symbol(&crate::cache::Symbol {
                id: "s_weird".to_string(),
                file_id,
                name: "*_[weird`_]*".to_string(),
                kind: "function".to_string(),
                byte_offset: 0,
                byte_length: 13,
            })
            .expect("symbol insert should succeed");

        let packer = Packer::with_workspace_root(
            cache,
            Parser {},
            dir.path().to_path_buf(),
            OutputFormat::Markdown,
            None,
            false,
            true, // no_files
            false,
            false,
            false,
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        // The file bullet must still be present — structure is intact.
        assert!(output.contains("- "), "File bullet disappeared");

        // The weird symbol name should appear verbatim in the output.
        assert!(
            output.contains("*_[weird`_]*"),
            "Symbol name with Markdown special chars should appear verbatim"
        );
    }

    #[test]
    fn test_markdown_skeleton_map_uses_exact_relative_path_for_duplicate_basenames() {
        use crate::cache::CacheStore;

        let cache = SqliteCache::new_in_memory().expect("failed to create test cache");
        cache.init().expect("failed to init cache schema");

        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        std::fs::create_dir_all(dir.path().join("src")).expect("create src dir");
        std::fs::create_dir_all(dir.path().join("tests")).expect("create tests dir");

        let src_path = make_temp_file(&dir, "src/lib.rs", "fn alpha() {}\n");
        let tests_path = make_temp_file(&dir, "tests/lib.rs", "fn beta() {}\n");

        let src_file_id = cache
            .upsert_file("src/lib.rs", "h-src", b"fn alpha() {}")
            .expect("upsert_file should succeed");
        cache
            .insert_symbol(&crate::cache::Symbol {
                id: "src_alpha".to_string(),
                file_id: src_file_id,
                name: "alpha".to_string(),
                kind: "function".to_string(),
                byte_offset: 0,
                byte_length: 13,
            })
            .expect("insert alpha symbol");

        let tests_file_id = cache
            .upsert_file("tests/lib.rs", "h-tests", b"fn beta() {}")
            .expect("upsert_file should succeed");
        cache
            .insert_symbol(&crate::cache::Symbol {
                id: "tests_beta".to_string(),
                file_id: tests_file_id,
                name: "beta".to_string(),
                kind: "function".to_string(),
                byte_offset: 0,
                byte_length: 12,
            })
            .expect("insert beta symbol");

        let packer = Packer::with_workspace_root(
            cache,
            Parser {},
            dir.path().to_path_buf(),
            OutputFormat::Markdown,
            None,
            false,
            true,
            false,
            false,
            false,
        );
        let output = packer
            .pack(&[src_path.clone(), tests_path.clone()])
            .expect("pack should succeed");

        let expected_src = format!("- {}\n  - function alpha", Packer::display_path(&src_path));
        let expected_tests = format!("- {}\n  - function beta", Packer::display_path(&tests_path));
        assert!(
            output.contains(&expected_src),
            "src/lib.rs should retain its own symbols; got:\n{output}"
        );
        assert!(
            output.contains(&expected_tests),
            "tests/lib.rs should retain its own symbols; got:\n{output}"
        );
    }

    #[test]
    fn test_plugin_detection_uses_workspace_root_for_nested_files() {
        struct RootMarkerPlugin;

        impl ContextPlugin for RootMarkerPlugin {
            fn name(&self) -> &str {
                "root-marker"
            }

            fn detect(&self, workspace_root: &Path) -> bool {
                workspace_root.join("manifest.json").exists()
            }

            fn enrich(&self, _file_path: &Path, base_bones: &mut Vec<Bone>) -> Result<()> {
                for bone in base_bones.iter_mut() {
                    bone.metadata
                        .insert("root_detected".to_string(), "true".to_string());
                }
                Ok(())
            }
        }

        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        std::fs::write(dir.path().join("manifest.json"), "{}").expect("write root marker");
        let nested = make_temp_file(&dir, "src/lib.rs", "fn nested() {}\n");

        let mut packer = Packer::with_workspace_root(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            dir.path().to_path_buf(),
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        packer.register_plugin(Box::new(RootMarkerPlugin));

        let output = packer.pack(&[nested]).expect("pack should succeed");
        assert!(
            output.contains("root_detected"),
            "plugin detect() should run against workspace root and enrich nested files"
        );
    }

    // =========================================================================
    // Token governor
    // =========================================================================

    /// With a generous budget, all file content should be included.
    #[test]
    fn test_token_governor_generous_budget_includes_content() {
        let (_dir, file_path) = make_temp_rs_file("fn main() { let x = 42; }\n");

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            Some(100_000), // very large budget
            false,
            false,
            false,
            false,
            false,
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        // Content block should be present.
        assert!(
            output.contains("<content><![CDATA["),
            "Expected <content> block when budget is generous; got:\n{}",
            output
        );
    }

    /// With a budget of 1 token, content must be omitted (only skeleton map output).
    #[test]
    fn test_token_governor_one_token_budget_omits_content() {
        let (_dir, file_path) = make_temp_rs_file("fn main() { let x = 42; }\n");

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            Some(1), // impossibly tight budget
            false,
            false,
            false,
            false,
            false,
        );
        let result = packer.pack(&[file_path]);

        // Must not panic or error.
        assert!(result.is_ok(), "pack() must not error under tight budget");
        let output = result.expect("pack should succeed");

        // No file content should be present.
        assert!(
            !output.contains("<content>"),
            "No <content> block expected when budget is 1 token"
        );
    }

    /// Degradation due to token exhaustion must be graceful — no panic, no Err.
    #[test]
    fn test_token_governor_graceful_degradation_no_panic() {
        let (_dir, file_path) =
            make_temp_rs_file("fn a() { 1 }\nfn b() { 2 }\nfn c() { 3 }\nfn d() { 4 }\n");

        for budget in [0usize, 1, 5, 50] {
            let packer = Packer::new(
                SqliteCache::new_in_memory().expect("failed to create test cache"),
                Parser {},
                OutputFormat::Xml,
                Some(budget),
                false,
                false,
                false,
                false,
                false,
            );
            let result = packer.pack(std::slice::from_ref(&file_path));
            assert!(
                result.is_ok(),
                "pack() panicked or errored at max_tokens={}",
                budget
            );
        }
    }

    // =========================================================================
    // Flag combinations
    // =========================================================================

    /// no_files=true AND no_file_summary=true together — the output should be
    /// minimal: just the opening/closing repository tags and nothing else.
    #[test]
    fn test_no_files_and_no_file_summary_together() {
        let (_dir, file_path) = make_temp_rs_file("fn main() {}\n");

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            true, // no_file_summary
            true, // no_files
            false,
            false,
            false,
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        // Only the repository wrapper should be present.
        let trimmed = output.trim();
        assert_eq!(
            trimmed, "<repository>\n</repository>",
            "With both no_files and no_file_summary, output should be just the repository tags; got:\n{}",
            trimmed
        );
    }

    /// remove_comments=true should strip `//` line comments from Rust source.
    #[test]
    fn test_remove_line_comments_from_rust() {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        // Use .txt so the parser falls back to raw content (no body elision complicates things).
        let file_path = make_temp_file(
            &dir,
            "comments.txt",
            "let x = 1; // this is a comment\nlet y = 2;\n",
        );

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            true, // remove_comments
            false,
            false,
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        assert!(
            !output.contains("// this is a comment"),
            "Line comment should be stripped; got:\n{}",
            output
        );
        assert!(
            output.contains("let x = 1;"),
            "Non-comment code should remain after stripping line comments"
        );
    }

    /// remove_comments=true should strip `/* */` block comments.
    #[test]
    fn test_remove_block_comments() {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let file_path = make_temp_file(
            &dir,
            "block_comments.txt",
            "int x = /* inline block */ 42;\n/* multi\nline\ncomment */\nint y = 1;\n",
        );

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            true, // remove_comments
            false,
            false,
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        assert!(
            !output.contains("inline block"),
            "Inline block comment should be stripped"
        );
        assert!(
            !output.contains("multi\nline\ncomment"),
            "Multi-line block comment should be stripped"
        );
        assert!(
            output.contains("int x ="),
            "Code outside block comment should be preserved"
        );
    }

    /// remove_empty_lines=true should collapse multiple consecutive blank lines
    /// into a single newline.
    #[test]
    fn test_remove_empty_lines_collapses_blanks() {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let file_path = make_temp_file(
            &dir,
            "blanks.txt",
            "line one\n\n\n\nline two\n\n\nline three\n",
        );

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            true, // remove_empty_lines
            false,
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        // There must be no run of more than one blank line in the content.
        assert!(
            !output.contains("\n\n\n"),
            "Multiple consecutive blank lines should be collapsed to a single newline; got:\n{}",
            output
        );
        assert!(
            output.contains("line one"),
            "Non-blank lines must be preserved"
        );
        assert!(
            output.contains("line two"),
            "Non-blank lines must be preserved"
        );
    }

    /// truncate_base64=true should replace strings of 100+ alphanumeric chars
    /// with the placeholder `[TRUNCATED_BASE64]`.
    #[test]
    fn test_truncate_base64_replaces_long_strings() {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        // Exactly 100 alphanumeric chars — the boundary that SHOULD be truncated.
        let long_token = "A".repeat(100);
        let content = format!("key = {}\n", long_token);
        let file_path = make_temp_file(&dir, "tokens.txt", &content);

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            true, // truncate_base64
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        assert!(
            output.contains("[TRUNCATED_BASE64]"),
            "A 100-char alphanumeric string should be replaced with [TRUNCATED_BASE64]"
        );
        assert!(
            !output.contains(&long_token),
            "The original long token must not appear in output after truncation"
        );
    }

    /// truncate_base64=true must NOT truncate strings of 99 characters or fewer.
    #[test]
    fn test_truncate_base64_preserves_short_strings() {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        // 99 alphanumeric chars — one below the truncation threshold.
        let short_token = "B".repeat(99);
        let content = format!("key = {}\n", short_token);
        let file_path = make_temp_file(&dir, "short_tokens.txt", &content);

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            true, // truncate_base64
        );
        let output = packer.pack(&[file_path]).expect("pack should succeed");

        assert!(
            output.contains(&short_token),
            "A 99-char string must NOT be truncated"
        );
        assert!(
            !output.contains("[TRUNCATED_BASE64]"),
            "No truncation should occur for strings under 100 chars"
        );
    }

    // =========================================================================
    // Multiple files
    // =========================================================================

    /// Packer with 3 files: all three must appear in the skeleton map.
    #[test]
    fn test_three_files_all_appear_in_skeleton_map() {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let f1 = make_temp_file(&dir, "one.txt", "content one\n");
        let f2 = make_temp_file(&dir, "two.txt", "content two\n");
        let f3 = make_temp_file(&dir, "three.txt", "content three\n");

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        let output = packer.pack(&[f1, f2, f3]).expect("pack should succeed");

        assert!(output.contains("one.txt"), "one.txt missing from output");
        assert!(output.contains("two.txt"), "two.txt missing from output");
        assert!(
            output.contains("three.txt"),
            "three.txt missing from output"
        );
    }

    /// Files must appear in the skeleton map in the same order they were supplied
    /// to pack() — i.e., the ordering is deterministic.
    #[test]
    fn test_skeleton_map_preserves_input_order() {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let f1 = make_temp_file(&dir, "alpha.txt", "alpha\n");
        let f2 = make_temp_file(&dir, "beta.txt", "beta\n");
        let f3 = make_temp_file(&dir, "gamma.txt", "gamma\n");

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        let output = packer.pack(&[f1, f2, f3]).expect("pack should succeed");

        let pos_alpha = output.find("alpha.txt").expect("alpha.txt not found");
        let pos_beta = output.find("beta.txt").expect("beta.txt not found");
        let pos_gamma = output.find("gamma.txt").expect("gamma.txt not found");

        assert!(
            pos_alpha < pos_beta && pos_beta < pos_gamma,
            "Files must appear in the skeleton map in the order they were supplied"
        );
    }

    // =========================================================================
    // Binary / missing files
    // =========================================================================

    /// A file that exists when pack() starts being called but has been deleted
    /// before its content is read should be gracefully skipped — no panic, no Err,
    /// just a warning on stderr.
    #[test]
    fn test_deleted_file_is_gracefully_skipped() {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let file_path = make_temp_file(&dir, "ephemeral.txt", "will be deleted\n");

        // Delete the file before calling pack().
        std::fs::remove_file(&file_path).expect("failed to delete ephemeral file");

        let packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        let result = packer.pack(&[file_path]);

        assert!(
            result.is_ok(),
            "pack() must not return Err when a file has been deleted; got: {:?}",
            result.err()
        );

        let output = result.expect("pack should succeed even when file is deleted");
        // The output should still be a well-formed XML document.
        assert!(
            output.contains("<repository>"),
            "Output must start with <repository>"
        );
        assert!(
            output.trim_end().ends_with("</repository>"),
            "Output must end with </repository>"
        );
        // No content should be emitted for the missing file.
        assert!(
            !output.contains("will be deleted"),
            "Content of deleted file must not appear in output"
        );
    }

    // =========================================================================
    // Metadata XML injection (Amber team gap #1)
    // =========================================================================

    /// Plugin metadata keys and values containing XML-dangerous characters must be
    /// escaped before being written into the <metadata> element.
    ///
    /// This test describes CORRECT behavior. The current implementation does NOT
    /// escape metadata key/value strings — so this test is expected to FAIL until
    /// the implementation is fixed.
    #[test]
    fn test_plugin_metadata_xml_escaping() {
        struct XmlDangerousPlugin;

        impl ContextPlugin for XmlDangerousPlugin {
            fn name(&self) -> &str {
                "xml_dangerous"
            }

            fn detect(&self, _directory: &Path) -> bool {
                true
            }

            fn enrich(&self, _file_path: &Path, base_bones: &mut Vec<Bone>) -> Result<()> {
                for bone in base_bones.iter_mut() {
                    // Key with XML-dangerous characters
                    bone.metadata.insert(
                        "key<with>&\"special".to_string(),
                        // Value that attempts XML injection: inject a sibling element
                        "</metadata><malicious>payload</malicious><metadata key=\"x\">".to_string(),
                    );
                }
                Ok(())
            }
        }

        let (_dir, file_path) = make_temp_rs_file("fn main() {}\n");
        let mut packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Xml,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        packer.register_plugin(Box::new(XmlDangerousPlugin));

        let output = packer.pack(&[file_path]).expect("pack should succeed");

        // The raw injection string must NOT appear verbatim in the output.
        assert!(
            !output.contains("<malicious>"),
            "Bare <malicious> tag found in output — metadata value was not XML-escaped; got:\n{}",
            output
        );
        assert!(
            !output.contains("</malicious>"),
            "Bare </malicious> tag found in output — metadata value was not XML-escaped; got:\n{}",
            output
        );

        // Escaped forms must be present instead.
        // The value contains '<' and '>' so at minimum &lt; and/or &gt; must appear.
        assert!(
            output.contains("&lt;") || output.contains("&gt;") || output.contains("&amp;"),
            "Expected XML-escaped entities (&lt;, &gt;, or &amp;) in metadata output; got:\n{}",
            output
        );

        // The document must still be well-formed (closing tag present).
        assert!(
            output.contains("</repository>"),
            "Output must still contain </repository> after metadata injection; got:\n{}",
            output
        );
    }

    /// Plugin metadata containing newlines must not be able to forge Markdown
    /// structure (headings, list items) in the packed output — Markdown
    /// injection requires a line start, so control characters are replaced.
    #[test]
    fn test_plugin_metadata_markdown_injection_neutralized() {
        struct MarkdownDangerousPlugin;

        impl ContextPlugin for MarkdownDangerousPlugin {
            fn name(&self) -> &str {
                "markdown_dangerous"
            }

            fn detect(&self, _directory: &Path) -> bool {
                true
            }

            fn enrich(&self, _file_path: &Path, base_bones: &mut Vec<Bone>) -> Result<()> {
                for bone in base_bones.iter_mut() {
                    bone.metadata.insert(
                        "key".to_string(),
                        "x\n## INJECTED HEADING\n- forged list item".to_string(),
                    );
                }
                Ok(())
            }
        }

        let (_dir, file_path) = make_temp_rs_file("fn main() {}\n");
        let mut packer = Packer::new(
            SqliteCache::new_in_memory().expect("failed to create test cache"),
            Parser {},
            OutputFormat::Markdown,
            None,
            false,
            false,
            false,
            false,
            false,
        );
        packer.register_plugin(Box::new(MarkdownDangerousPlugin));

        let output = packer.pack(&[file_path]).expect("pack should succeed");

        // No line in the output may start with the injected heading/list markers.
        assert!(
            !output.lines().any(|l| l.starts_with("## INJECTED HEADING")),
            "Metadata newline forged a Markdown heading; got:\n{}",
            output
        );
        assert!(
            !output.lines().any(|l| l.starts_with("- forged list item")),
            "Metadata newline forged a Markdown list item; got:\n{}",
            output
        );
        // The metadata text itself must still be present (flattened onto one line).
        assert!(
            output.contains("INJECTED HEADING"),
            "Sanitized metadata value must still appear in output; got:\n{}",
            output
        );
    }
}
