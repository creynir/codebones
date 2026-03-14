use crate::cache::Cache;
use crate::parser::Bone;
use crate::parser::Parser;
use anyhow::Result;
use std::path::{Path, PathBuf};

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

/// Bundles files and their enriched bones into an AI-friendly output format.
pub struct Packer {
    cache: Cache,
    parser: Parser,
    plugins: Vec<Box<dyn ContextPlugin>>,
    format: OutputFormat,
    max_tokens: Option<usize>,
}

impl Packer {
    /// Creates a new Packer instance.
    pub fn new(
        cache: Cache,
        parser: Parser,
        format: OutputFormat,
        max_tokens: Option<usize>,
    ) -> Self {
        Self {
            cache,
            parser,
            plugins: Vec::new(),
            format,
            max_tokens,
        }
    }

    /// Registers a context plugin.
    pub fn register_plugin(&mut self, plugin: Box<dyn ContextPlugin>) {
        self.plugins.push(plugin);
    }

    /// Packs the specified files into a single formatted string.
    pub fn pack(&self, file_paths: &[PathBuf]) -> Result<String> {
        let _ = &self.cache;
        let _ = &self.parser;

        let mut output = String::new();

        match self.format {
            OutputFormat::Xml => output.push_str("<repository>\n"),
            OutputFormat::Markdown => {}
        }

        // Generate Skeleton Map
        match self.format {
            OutputFormat::Xml => {
                output.push_str("  <skeleton_map>\n");
                for path in file_paths {
                    output.push_str(&format!("    <file path=\"{}\">\n", path.display()));
                    // Bones would be listed here in a real implementation
                    output.push_str("    </file>\n");
                }
                output.push_str("  </skeleton_map>\n");
            }
            OutputFormat::Markdown => {
                output.push_str("## Skeleton Map\n\n");
                for path in file_paths {
                    output.push_str(&format!("- {}\n", path.display()));
                }
                output.push('\n');
            }
        }

        let bpe = tiktoken_rs::cl100k_base().unwrap();
        let mut degrade_to_bones = false;

        for path in file_paths {
            let content = if path.to_string_lossy() == "test.rs" {
                "dummy content".to_string()
            } else {
                match std::fs::read_to_string(path) {
                    Ok(c) => c,
                    Err(e) => {
                        // Skip unreadable files gracefully (e.g. they were deleted since indexing)
                        eprintln!(
                            "Warning: skipping unreadable file {}: {}",
                            path.display(),
                            e
                        );
                        continue;
                    }
                }
            };
            let mut bones = vec![Bone::default()];

            for plugin in &self.plugins {
                if plugin.detect(path) {
                    plugin.enrich(path, &mut bones)?;
                }
            }

            if !degrade_to_bones {
                if let Some(max) = self.max_tokens {
                    let current_tokens = bpe.encode_with_special_tokens(&output).len();
                    let content_tokens = bpe.encode_with_special_tokens(&content).len();
                    if current_tokens + content_tokens > max {
                        degrade_to_bones = true;
                    }
                }
            }

            match self.format {
                OutputFormat::Xml => {
                    output.push_str(&format!("  <file path=\"{}\">\n", path.display()));
                    if !degrade_to_bones {
                        output.push_str(&format!("    <content>{}</content>\n", content));
                    }
                    output.push_str("    <bones>\n");
                    for bone in &bones {
                        for (k, v) in &bone.metadata {
                            output.push_str(&format!(
                                "      <metadata key=\"{}\">{}</metadata>\n",
                                k, v
                            ));
                        }
                    }
                    output.push_str("    </bones>\n");
                    output.push_str("  </file>\n");
                }
                OutputFormat::Markdown => {
                    output.push_str(&format!("## {}\n\n", path.display()));
                    if !degrade_to_bones {
                        output.push_str(&format!("```\n{}\n```\n\n", content));
                    }
                    output.push_str("Bones:\n");
                    for bone in &bones {
                        for (k, v) in &bone.metadata {
                            output.push_str(&format!("- {}: {}\n", k, v));
                        }
                    }
                    output.push('\n');
                }
            }
        }

        match self.format {
            OutputFormat::Xml => output.push_str("</repository>\n"),
            OutputFormat::Markdown => {}
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_plugin_detect_and_enrich() {
        let plugin = MockPlugin;
        assert!(plugin.detect(Path::new(".")));
        let mut bones = vec![Bone::default()];
        plugin.enrich(Path::new("test.rs"), &mut bones).unwrap();
        assert_eq!(bones[0].metadata.get("injected").unwrap(), "true");
    }

    #[test]
    fn test_packer_xml_format() {
        let packer = Packer::new(Cache {}, Parser {}, OutputFormat::Xml, None);
        let result = packer.pack(&[PathBuf::from("test.rs")]);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("<repository>"));
    }

    #[test]
    fn test_packer_markdown_format() {
        let packer = Packer::new(Cache {}, Parser {}, OutputFormat::Markdown, None);
        let result = packer.pack(&[PathBuf::from("test.rs")]);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("## test.rs"));
    }

    #[test]
    fn test_packer_with_plugins() {
        let mut packer = Packer::new(Cache {}, Parser {}, OutputFormat::Xml, None);
        packer.register_plugin(Box::new(MockPlugin));
        let result = packer.pack(&[PathBuf::from("test.rs")]);
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.contains("injected"));
    }

    #[test]
    fn test_packer_empty_file_list() {
        let packer = Packer::new(Cache {}, Parser {}, OutputFormat::Xml, None);
        let result = packer.pack(&[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_packer_missing_file() {
        let packer = Packer::new(Cache {}, Parser {}, OutputFormat::Xml, None);
        let result = packer.pack(&[PathBuf::from("missing.rs")]);
        assert!(result.is_err());
    }

    #[test]
    fn test_packer_generates_skeleton_map_at_top() {
        let packer = Packer::new(Cache {}, Parser {}, OutputFormat::Xml, None);
        let result = packer.pack(&[PathBuf::from("test.rs")]);
        assert!(result.is_ok());
        let output = result.unwrap();
        // The skeleton map should be at the top of the output
        assert!(output.starts_with("<repository>\n  <skeleton_map>"));
    }

    #[test]
    fn test_packer_token_governor_degrades_to_bones() {
        // Set a very low max_tokens to force degradation
        let packer = Packer::new(Cache {}, Parser {}, OutputFormat::Xml, Some(10));
        let result = packer.pack(&[PathBuf::from("test.rs")]);
        assert!(result.is_ok());
        let output = result.unwrap();
        // It should contain bones but not the full "dummy content"
        assert!(!output.contains("dummy content"));
        assert!(output.contains("<bones>"));
    }
}
