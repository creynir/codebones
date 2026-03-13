# Research: Repository Maps for LLM Context Packing

When providing codebase context to Large Language Models, the structure and density of the information are critical. This document compares two primary approaches to "Repository Maps" and proposes a superior architecture for the `codebones` project.

## 1. The Standard File-Tree Approach
**Tools:** `repomix`, `gpt-repository-loader`

This approach generates a simple hierarchical text representation of the directory and file structure, similar to the output of the Unix `tree` command.

**Pros:**
*   Extremely fast to generate (requires only filesystem metadata).
*   Provides a high-level overview of project organization.
*   Low token overhead for small to medium projects.

**Cons:**
*   Opaque contents: The LLM only sees file names, not what is inside them.
*   Poor signal-to-noise ratio: A file named `utils.ts` could contain anything from a simple date formatter to a complex state machine.

## 2. The AST-Aware Symbol Map Approach
**Pioneered by:** `aider`

Instead of just showing file names, this approach parses the source code into an Abstract Syntax Tree (AST) using tools like `tree-sitter`. It extracts the "bones" of the codebase—classes, methods, and function signatures—and presents a semantic map of the repository.

### How `aider` Formats its Repo Map
`aider` builds a highly condensed, tree-like text representation of the codebase's symbols. 
*   **Structure:** It uses indentation to represent scope (e.g., File -> Class -> Method).
*   **Content:** It strips out all implementation details (function bodies, internal variables) and only keeps the signatures.
*   **Ranking:** To fit within token limits, `aider` often builds a graph of dependencies (imports/calls) and uses algorithms like PageRank to determine which symbols are most important to the current task, pruning the rest.

Example of an `aider`-style map:
```python
src/server.py:
class Server:
    def start(self, port: int): ...
    def handle_request(self, req: Request): ...

src/models/user.py:
class User:
    def __init__(self, name: str): ...
    def save(self): ...
```

## Proposal: A Superior `Packer` for `codebones`

`codebones` already possesses a significant advantage: an SQLite cache of tree-sitter "bones" (signatures). We can leverage this to build a blazing-fast, highly contextual `Packer` that outperforms existing tools.

### Architecture of the `codebones` Packer

The final packed prompt should be structured in three distinct layers:

#### 1. The AST-Aware Repo Map (Top Section)
Query the SQLite cache to instantly retrieve the signatures for the relevant subset of the codebase. Format this as a condensed, indented tree (similar to `aider`). Because the data is pre-cached in SQLite, generation will be instantaneous, bypassing the need to re-parse files on the fly.

#### 2. The File Contents (Middle/Bottom Section)
Below the repo map, inject the full contents of the specifically requested or highly relevant files. To ensure the LLM can easily parse boundaries, wrap these contents in XML tags (which LLMs are highly trained to recognize) or standard Markdown code blocks.

```xml
<file path="src/server.py">
# Full implementation here...
</file>
```

#### 3. Dynamic Token Counting (`tiktoken-rs`)
To ensure the packed context never exceeds the LLM's context window (and to optimize costs), integrate a fast Rust crate like `tiktoken-rs`. 
*   **Process:** As the `Packer` builds the prompt, it continuously counts tokens.
*   **Strategy:** First, allocate tokens for the requested full-file contents. Then, use the remaining token budget to expand the AST-Aware Repo Map. If the map exceeds the budget, prune the least relevant symbols (based on a simple heuristic or import graph) until it fits perfectly.

### Summary of the `codebones` Advantage
By combining an **SQLite-backed AST cache**, **XML/Markdown content formatting**, and **strict `tiktoken-rs` token budgeting**, `codebones` will provide LLMs with maximum semantic context at minimal latency and cost.