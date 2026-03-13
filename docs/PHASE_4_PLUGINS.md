# Phase 4: Plugin System & Pack Command

## 1. Overview
This specification outlines the implementation of the `ContextPlugin` trait and the `Packer` struct for `codebones`. The goal is to allow extensible metadata enrichment via plugins (e.g., dbt, OpenAPI) and to provide a `pack` command that bundles multiple files and their extracted symbols ("bones") into an AI-friendly format (XML or Markdown), similar to tools like `repomix`.

## 2. The `ContextPlugin` Trait

The `ContextPlugin` trait defines the interface for all plugins. It lives in `crates/core/src/plugin.rs`.

```rust
use std::path::Path;
use anyhow::Result;
use crate::parser::Bone;

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
```

## 3. The `Packer` Struct

The `Packer` struct is responsible for taking a list of files, retrieving their contents and parsed bones, applying any active plugins, and formatting the result into a single string.

```rust
use std::path::{Path, PathBuf};
use anyhow::Result;
use crate::cache::Cache;
use crate::parser::Parser;
use crate::plugin::ContextPlugin;

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
}

impl Packer {
    /// Creates a new Packer instance.
    pub fn new(cache: Cache, parser: Parser, format: OutputFormat) -> Self {
        Self {
            cache,
            parser,
            plugins: Vec::new(),
            format,
        }
    }

    /// Registers a context plugin.
    pub fn register_plugin(&mut self, plugin: Box<dyn ContextPlugin>) {
        self.plugins.push(plugin);
    }

    /// Packs the specified files into a single formatted string.
    pub fn pack(&self, file_paths: &[PathBuf]) -> Result<String> {
        // Implementation steps:
        // 1. Initialize the output buffer based on self.format.
        // 2. Iterate over each file path in `file_paths`.
        // 3. Retrieve the file content and its extracted `Bone`s from `self.cache` / `self.parser`.
        // 4. For each active plugin (where `detect` returned true), call `enrich` on the bones.
        // 5. Format the file content and the enriched bones into the output buffer (XML or Markdown).
        // 6. Return the final packed string.
        todo!("Implement the packing logic")
    }
}
```

## 4. TDD Unit Tests

The test-writing agent must implement the following strict TDD unit tests for the plugin system and packer:

1. **`test_plugin_detect_and_enrich`**: Create a mock `ContextPlugin`. Verify that `detect` correctly identifies the target environment and that `enrich` successfully mutates the `metadata` field of a provided `Bone`.
2. **`test_packer_xml_format`**: Pass a list of mock files to the `Packer` with `OutputFormat::Xml`. Verify the output strictly follows the expected XML structure (e.g., `<repository><file path="..."><content>...</content><bones>...</bones></file></repository>`).
3. **`test_packer_markdown_format`**: Pass a list of mock files to the `Packer` with `OutputFormat::Markdown`. Verify the output strictly follows the expected Markdown structure (e.g., headers for file paths, code blocks for content).
4. **`test_packer_with_plugins`**: Register a mock plugin that injects specific metadata. Call `pack` and verify that the resulting XML/Markdown output includes the injected metadata for the relevant files.
5. **`test_packer_empty_file_list`**: Call `pack` with an empty `file_paths` slice. Verify it returns a valid, empty representation of the chosen format without erroring.
6. **`test_packer_missing_file`**: Call `pack` with a file path that does not exist in the cache. Verify that the `Packer` either returns an appropriate `anyhow::Error` or gracefully skips the file and logs a warning, depending on the desired error-handling policy.
