# Token Savings Benchmark

Measures how many tokens an AI agent needs to consume for common tasks — with codebones vs reading raw source files.

## Datasets

| Label | Repo | LOC | Language | Commit |
|---|---|---:|---|---|
| small | [tiangolo/fastapi](https://github.com/tiangolo/fastapi) | 107,493 | Python | `eba8942c81db` |
| medium | [temporalio/temporal](https://github.com/temporalio/temporal) | 832,991 | Go | `29a039286526` |
| large | [n8n-io/n8n](https://github.com/n8n-io/n8n) | 2,068,515 | TypeScript | `f7a787aca81c` |

## Static Scenarios

### 1. Project Orientation

> **Prompt:** "Describe the architecture of this project. What are the main modules, their responsibilities, and how they relate to each other? Be specific about file paths."

| Approach | What the AI receives |
|----------|---------------------|
| Without codebones | All source files concatenated — every line of every file |
| With codebones | `codebones map --format markdown` — file paths + function/class signatures |

**Measured:** total tokens (raw source) vs tokens (map output).

### 2. Impact Analysis

> **Prompt:** "I need to refactor `fastapi/routing.py`. What other files in this project would be affected by changes to that file? List them and explain the dependency chain."

| Approach | What the AI receives |
|----------|---------------------|
| Without codebones | All source files — AI must trace imports manually across the entire codebase |
| With codebones | `codebones graph fastapi/routing.py` — affected files with dependency chains |

**Measured:** total tokens (all source) vs tokens (graph output).

### 3. Symbol Retrieval

> **Prompt:** "Find the `APIRouter` class, explain its main methods and how it's used by the rest of the codebase."

| Approach | What the AI receives |
|----------|---------------------|
| Without codebones | Full content of files containing "APIRouter" |
| With codebones | `codebones search APIRouter` + `codebones get <symbol_id>` — search results + targeted source |

**Measured:** tokens (files containing the symbol) vs tokens (search + get output).

### 4. Budget Efficiency

> **Prompt:** "Give me enough context about this project to start contributing. I have a 16K token budget."

| Approach | What the AI receives |
|----------|---------------------|
| Without codebones | First 16K tokens of alphabetically concatenated files — hard cutoff |
| With codebones | `codebones pack --max-tokens 16000` — skeleton map + as many file bodies as fit, prioritized by import count |

**Measured:** number of symbols visible at each budget (8K, 16K, 32K, 64K).

## Agent Eval (Real-World Benchmark)

The static scenarios above count raw tokens but don't capture what actually happens when an AI agent works on a codebase. A real agent doesn't receive one big dump — it explores iteratively, making tool calls.

The agent eval runs real development tasks as **multi-turn agentic conversations** through the Claude Sonnet API on [FastAPI](https://github.com/tiangolo/fastapi) (107K LOC, Python). No turn limit — agents work until done.

### Key finding

codebones doesn't replace grep — it complements it. The most efficient agent uses both:

- `codebones search` to jump directly to symbols (no directory browsing)
- `codebones get` to read one function (not the whole file)
- `codebones graph <file>` to check blast radius before editing
- `grep` for text patterns (imports, strings, config values)
- `cat` for small files

### Setup

Two agents get the same task:

**Standard agent:**
```
Tools: grep, cat, find, ls
```

**Standard + codebones agent:**
```
Tools: grep, cat, find, ls, codebones_search, codebones_get,
       codebones_graph, codebones_outline
```

Both agents choose their own exploration strategy.

### Tasks

**Task 1 — Implement middleware:**
> "Add a CORS middleware to the FastAPI application that allows origins from http://localhost:3000 and http://localhost:5173. Find where middleware is configured, look at existing middleware examples as a pattern, and write the code."

**Task 2 — Trace a bug:**
> "I'm getting a TypeError when using `Depends()` with an async generator that yields None. Find the dependency resolution code, trace how generator dependencies are handled, and identify where the bug might be."

### Results

| Task | Standard only | Standard + codebones | Tokens saved | Turns saved |
|------|---:|---:|---:|---:|
| Implement middleware | 65K tokens, 27 calls, 15 turns | 55K tokens, 23 calls, 9 turns | **1.2x** | **40%** |
| Trace dependency bug | 144K tokens, 34 calls, 20 turns | 114K tokens, 16 calls, 10 turns | **1.3x** | **53%** |

### How the codebones agent traced the bug

The agent used `codebones search` to jump through the call chain:

1. `search "solve_dependencies"` → found the entry point
2. `search "is_async_gen_callable"` → found how generator deps are detected
3. `search "contextmanager_in_threadpool"` → found the context manager wrapper
4. `get "solve_dependencies"` → read the full implementation
5. `search "_solve_generator"` → found the specific generator handling code
6. `outline "dependencies/utils.py"` → saw the full file structure

Then switched to `grep` for text patterns (`asynccontextmanager`, `stack.enter`).

The standard agent needed 34 calls across 20 turns of `ls` → `find` → `grep` → `cat` → `grep again` to build the same understanding.

### What we measure

For each conversation (all turns, all tool calls):
- `total_input_tokens` — total tokens sent to the model across all turns
- `total_output_tokens` — total tokens the model generated
- `total_tokens` — input + output
- `tool_calls` — number of tool invocations
- `turns` — number of conversation turns

### Verifiability

Every conversation is saved as a JSON log in [agent-eval-results/](agent-eval-results/). Each log contains the exact system prompt, every tool call with input and output, the model's reasoning, and the final answer. Token counts come from the API `usage` field.

## Running the benchmarks

```bash
# Prerequisites
cargo build --release -p codebones
pip install tiktoken anthropic

# Static token savings (clones datasets into lab/ on first run)
./docs/benchmarks/run_token_savings.sh

# Agent eval on FastAPI (requires API key)
ANTHROPIC_API_KEY=your-key python3 docs/benchmarks/run_agent_eval.py
```

Both scripts are self-contained — they clone datasets into `lab/` at pinned commits on first run. The `lab/` directory is gitignored.

## Token counting

Tokens are counted using `tiktoken` with the `cl100k_base` encoding (same tokenizer used by codebones internally for `--max-tokens`). For the agent eval, token counts come directly from the API response `usage` field.
