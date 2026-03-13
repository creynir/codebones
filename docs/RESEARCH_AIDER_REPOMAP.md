# Research: Aider's Repo Map Implementation

## 1. How does it use tree-sitter?
Aider uses `tree-sitter` (via the `grep_ast` wrapper) to parse source code files and extract "tags" (identifiers). It relies on language-specific `.scm` query files (e.g., `tags.scm`) to identify definitions (`name.definition.*`) and references (`name.reference.*`). If a language's tags query only provides definitions (e.g., C++), it falls back to using Pygments to lex the file and backfill references by looking for tokens of type `Token.Name`.

## 2. How does it rank symbols?
Aider builds a directed multigraph using `networkx` where nodes are files and edges represent a reference in one file pointing to a definition in another file. It then runs the PageRank algorithm (`networkx.pagerank`) on this graph to rank the files and the definitions within them.

Key aspects of the ranking:
- **Personalization**: The PageRank algorithm is "personalized" (biased) towards files that are currently in the chat (`chat_fnames`) or files whose names/paths were explicitly mentioned by the user (`mentioned_fnames`, `mentioned_idents`).
- **Edge Weights**: Edges are weighted based on several heuristics:
  - The number of references (scaled down using `math.sqrt` so high-frequency mentions don't dominate).
  - Multipliers for identifier characteristics:
    - `10x` multiplier if the identifier was explicitly mentioned by the user.
    - `10x` multiplier for identifiers that are at least 8 characters long and follow standard naming conventions (snake_case, camelCase, kebab-case).
    - `0.1x` multiplier for "private" identifiers starting with `_`.
    - `0.1x` multiplier if an identifier has more than 5 definitions (to penalize overly common names).
  - A massive `50x` multiplier if the reference originates from a file currently in the chat.

The final rank of a definition is calculated by distributing the PageRank score of its source file across all its outgoing edges based on these weights.

## 3. How does it format the output string?
Aider uses the `TreeContext` class from the `grep_ast` library to format the output. It takes the top-ranked tags (which include line numbers) and passes them as "lines of interest" (`lois`) to `TreeContext`. This class renders a condensed, tree-like view of the file's AST, showing only the structural context (classes, functions) around those specific lines of interest, while omitting the bodies of uninteresting functions.

## 4. How does it handle token limits?
Aider uses a binary search algorithm to maximize the number of tags included in the repo map without exceeding the `max_map_tokens` limit. 
1. It sorts all tags by their calculated rank.
2. It sets a lower bound (0) and an upper bound (total number of tags).
3. It picks a middle point, takes the top `N` tags, renders the tree string, and counts the tokens using the LLM's tokenizer.
4. If the token count is within an acceptable error margin (15%) of the limit, it stops. Otherwise, it adjusts the bounds and repeats the process until it finds the optimal number of tags to include.

## Recommendation for `codebones`
**Recommendation: Start with a simpler AST-aware tree, but keep PageRank in mind for the future.**

For the `pack` command in `codebones`, implementing a full PageRank-based system with a dependency graph is likely overkill for the initial version. 

Aider's PageRank approach is designed specifically for an interactive chat environment where the AI needs to automatically discover *implicit* dependencies related to the files the user is currently editing. 

If `codebones pack` is primarily a CLI tool for users to manually bundle context, a simpler approach is sufficient:
1. Use `tree-sitter` to parse the files.
2. Extract the high-level structural outline (classes, methods, function signatures).
3. Allow the user to explicitly specify which files or symbols they want to include or expand.
4. Prune the AST to only show the signatures, omitting implementation details unless explicitly requested.

This simpler AST-pruning approach (similar to what `grep_ast` does natively) will provide 80% of the value with 20% of the complexity. If we later find that users struggle to manually identify which files to include in their `pack`, we can then introduce a PageRank-based dependency discovery mechanism to automatically suggest or include related context.