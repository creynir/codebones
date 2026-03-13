# Instructions for AI Agents

When asked to implement and register a new context plugin in `codebones`, you must modify `crates/core/src/plugin.rs` to register it correctly. Follow these steps to ensure the plugin is automatically initialized for every file processed:

1. **Import the Plugin:** Add a `use` statement at the top of `crates/core/src/plugin.rs` to bring your new plugin into scope (e.g., `use crate::dbt::DbtPlugin;`).
2. **Modify `Packer::new`:** Update the `Packer::new` function inside `crates/core/src/plugin.rs` to instantiate and register the plugin whenever a new `Packer` is created.

You should change the `Packer::new` implementation from this:
```rust
pub fn new(cache: Cache, parser: Parser, format: OutputFormat, max_tokens: Option<usize>) -> Self {
    Self {
        cache,
        parser,
        plugins: Vec::new(),
        format,
        max_tokens,
    }
}
```

To this:
```rust
pub fn new(cache: Cache, parser: Parser, format: OutputFormat, max_tokens: Option<usize>) -> Self {
    let mut packer = Self {
        cache,
        parser,
        plugins: Vec::new(),
        format,
        max_tokens,
    };
    
    // Register the new plugin here:
    // packer.register_plugin(Box::new(DbtPlugin::new()));
    
    packer
}
```

---

# Codebones Plugin Authoring Guide

## 1. What is a ContextPlugin?

A `ContextPlugin` is a powerful extension mechanism in `codebones`. It exists to enrich the standard tree-sitter AST "bones" (basic structural code elements) with external, domain-specific metadata.

While `codebones` naturally excels at extracting the structural skeleton of code (classes, functions, etc.), it lacks context about external systems. Context plugins bridge this gap by injecting semantic context. For example:
- A **dbt plugin** can read a `manifest.json` file and append database column data and lineage to SQL bones.
- An **OpenAPI plugin** can read a `swagger.yaml` file and annotate API route handlers with expected request and response schemas.
- A **GraphQL plugin** can attach schema definitions to corresponding resolver functions.

## 2. The `ContextPlugin` Trait

To create a plugin, you must implement the `ContextPlugin` trait. Here is the exact trait definition from `crates/core/src/plugin.rs`:

```rust
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

## 3. Step-by-Step Tutorial: Implementing a Dummy `DbtPlugin`

In this tutorial, we will implement a dummy `DbtPlugin` that reads a `manifest.json` file to determine if the workspace uses `dbt`, and then appends dummy column data to the bones of SQL files.

### Step 1: Define the Plugin Struct

First, define your plugin struct. In a real-world scenario, you might parse and cache the `manifest.json` inside this struct during the `detect` or initialization phase.

```rust
use std::path::Path;
use anyhow::Result;
use crate::parser::Bone;
use crate::plugin::ContextPlugin;

pub struct DbtPlugin {
    // In a real plugin, you would load and cache the parsed manifest.json here
    manifest_loaded: bool,
}

impl DbtPlugin {
    pub fn new() -> Self {
        Self {
            manifest_loaded: false,
        }
    }
}
```

### Step 2: Implement the `ContextPlugin` Trait

Next, implement the `ContextPlugin` trait for your struct. We will define logic to detect a dbt workspace and enrich SQL files with dummy column metadata.

```rust
impl ContextPlugin for DbtPlugin {
    fn name(&self) -> &str {
        "dbt"
    }

    fn detect(&self, directory: &Path) -> bool {
        // Detect if this is a dbt project by checking for manifest.json
        // or dbt_project.yml in the target directory.
        let manifest_path = directory.join("target/manifest.json");
        manifest_path.exists()
    }

    fn enrich(&self, file_path: &Path, base_bones: &mut Vec<Bone>) -> Result<()> {
        // We only want to enrich SQL files
        if file_path.extension().and_then(|ext| ext.to_str()) != Some("sql") {
            return Ok(());
        }

        // Iterate through the extracted bones and append metadata
        for bone in base_bones.iter_mut() {
            // In a real implementation, you would match the specific bone (e.g., a SQL model name)
            // with data from the parsed manifest.json.
            // Here, we simply append some dummy column data.
            bone.metadata.insert(
                "dbt_columns".to_string(), 
                "id, user_id, created_at, updated_at".to_string()
            );
        }

        Ok(())
    }
}
```

### Step 3: Register the Plugin

As mentioned in the AI Agent instructions, you must register your plugin with the `Packer` instance so it runs automatically.

Update the `Packer::new` function in `crates/core/src/plugin.rs`:

```rust
use crate::dbt::DbtPlugin;

impl Packer {
    pub fn new(cache: Cache, parser: Parser, format: OutputFormat, max_tokens: Option<usize>) -> Self {
        let mut packer = Self {
            cache,
            parser,
            plugins: Vec::new(),
            format,
            max_tokens,
        };
        
        // Register the new plugin
        packer.register_plugin(Box::new(DbtPlugin::new()));
        
        packer
    }
}
```

And that's it! Your SQL bones will now be enriched with `dbt` metadata whenever `codebones` encounters a `dbt` project.
