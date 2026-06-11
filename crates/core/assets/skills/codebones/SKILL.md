---
name: codebones
description: Use the codebones CLI to inspect repository structure before editing.
---
<!-- codebones-managed-skill:v1 -->

# Codebones

This project may be indexed by codebones. Prefer codebones tools over broad file
crawling when you need structural context.

Use:

- `codebones search <name>` to find functions, classes, and symbols by name.
- `codebones get <symbol> --filter <keyword>` to read matching lines with light context.
- `codebones get <symbol>` to read full source when you need the complete implementation.
- `codebones outline <file>` to inspect file structure with signatures and bodies elided.
- `codebones graph <file>` to check blast radius: what depends on this file and what imports it.
  Treat the result as a lower bound — imports that resolve through aliases (e.g. tsconfig
  paths), re-exports, or dynamic loading are not followed. If the affected-file count looks
  thin for a widely used file, cross-check with `rg` on the file's basename before concluding
  nothing depends on it.
- `codebones map` to get a repository-level skeleton before planning larger changes.

Use normal file reads when you already know the exact small file, need non-code
files, docs, config, prose, or need context that codebones does not index.
