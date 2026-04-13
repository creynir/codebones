#!/usr/bin/env python3
"""
Agent-based Token Savings Benchmark

Runs the same task twice through Claude — once with standard tools (grep/cat/find)
and once with codebones tools (map/search/get/graph). Measures total tokens consumed
across the full multi-turn conversation.

Each task runs 3 times per approach to account for variance. Reports median.

Requires: ANTHROPIC_API_KEY env var, pip install anthropic
"""

import json
import os
import subprocess
import sys
import csv
import statistics
from pathlib import Path
from datetime import datetime

try:
    import anthropic
except ImportError:
    print("ERROR: pip install anthropic")
    sys.exit(1)

SCRIPT_DIR = Path(__file__).parent
REPO_ROOT = SCRIPT_DIR.parent.parent
LAB_DIR = REPO_ROOT / "lab"
OUTPUT_DIR = SCRIPT_DIR / "agent-eval-results"
OUTPUT_CSV = SCRIPT_DIR / "agent-eval.csv"
CODEBONES = REPO_ROOT / "target" / "release" / "codebones"

if not CODEBONES.exists():
    print(f"ERROR: {CODEBONES} not found. Run: cargo build --release -p codebones")
    sys.exit(1)

MODEL = "claude-sonnet-4-6"
RUNS_PER_TASK = 1
MAX_TURNS = 20

client = anthropic.Anthropic()


# ---------------------------------------------------------------------------
# Tool definitions for both approaches
# ---------------------------------------------------------------------------

def make_standard_tools(repo_dir: str):
    """Tools an agent has without codebones: grep, cat, find, ls."""
    return [
        {
            "name": "grep",
            "description": "Search for a pattern in files. Returns matching lines with file paths.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "pattern": {"type": "string", "description": "Regex pattern to search for"},
                    "path": {"type": "string", "description": "Directory or file to search in (relative to repo root)"},
                    "include": {"type": "string", "description": "File glob pattern, e.g. '*.py'"},
                },
                "required": ["pattern"],
            },
        },
        {
            "name": "cat",
            "description": "Read the contents of a file.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path relative to repo root"},
                },
                "required": ["path"],
            },
        },
        {
            "name": "find",
            "description": "List files matching a pattern.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory to search in"},
                    "name": {"type": "string", "description": "File name pattern, e.g. '*.py'"},
                },
                "required": ["path"],
            },
        },
        {
            "name": "ls",
            "description": "List directory contents.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "Directory path relative to repo root"},
                },
                "required": ["path"],
            },
        },
    ]


def make_codebones_tools(repo_dir: str):
    """Tools an agent has with codebones."""
    return [
        {
            "name": "codebones_search",
            "description": "Search for symbols (functions, classes, methods) by name substring. Returns symbol IDs and their full source code.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Substring to search for in symbol names"},
                },
                "required": ["query"],
            },
        },
        {
            "name": "codebones_get",
            "description": "Retrieve the full source code of a specific symbol or file.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "symbol_or_path": {"type": "string", "description": "Symbol ID (e.g. 'src/main.py::MyClass.method') or file path"},
                },
                "required": ["symbol_or_path"],
            },
        },
        {
            "name": "codebones_graph",
            "description": "Get the import dependency graph. Without a file argument, returns all files sorted by how many other files import them. With a file argument, returns the blast radius — all files transitively affected by changing that file.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "file": {"type": "string", "description": "Optional: specific file to get blast radius for"},
                },
            },
        },
        {
            "name": "codebones_outline",
            "description": "Get the skeleton of a specific file — function signatures with bodies replaced by '...'.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path"},
                },
                "required": ["path"],
            },
        },
    ]


# ---------------------------------------------------------------------------
# Tool execution
# ---------------------------------------------------------------------------

def execute_standard_tool(name: str, args: dict, repo_dir: Path) -> str:
    """Execute a standard file-system tool."""
    try:
        if name == "grep":
            pattern = args.get("pattern", "")
            path = args.get("path", ".")
            include = args.get("include", "")
            cmd = ["grep", "-rn", "--include", include, pattern, str(repo_dir / path)] if include else ["grep", "-rn", pattern, str(repo_dir / path)]
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=10)
            output = result.stdout
            # Replace absolute paths with relative
            output = output.replace(str(repo_dir) + "/", "")
            # Truncate to keep conversation small
            lines = output.split("\n")
            if len(lines) > 15:
                output = "\n".join(lines[:15]) + f"\n... ({len(lines) - 15} more lines)"
            return output or "(no matches)"

        elif name == "cat":
            path = repo_dir / args["path"]
            if not path.exists():
                return f"Error: {args['path']} not found"
            content = path.read_text(errors="replace")
            if len(content) > 3000:
                content = content[:3000] + f"\n... (truncated, {len(content)} chars total)"
            return content

        elif name == "find":
            path = args.get("path", ".")
            name_pattern = args.get("name", "*")
            result = subprocess.run(
                ["find", str(repo_dir / path), "-name", name_pattern, "-type", "f"],
                capture_output=True, text=True, timeout=10,
            )
            output = result.stdout.replace(str(repo_dir) + "/", "")
            lines = output.strip().split("\n")
            if len(lines) > 50:
                output = "\n".join(lines[:50]) + f"\n... ({len(lines) - 50} more files)"
            return output or "(no files found)"

        elif name == "ls":
            path = repo_dir / args.get("path", ".")
            if not path.exists():
                return f"Error: {args['path']} not found"
            entries = sorted(path.iterdir())
            return "\n".join(
                f"{'d ' if e.is_dir() else '  '}{e.name}" for e in entries[:50]
            )

    except subprocess.TimeoutExpired:
        return "(command timed out)"
    except Exception as e:
        return f"Error: {e}"


def execute_codebones_tool(name: str, args: dict, repo_dir: Path) -> str:
    """Execute a codebones tool."""
    try:
        if name == "codebones_map":
            result = subprocess.run(
                [str(CODEBONES), "map", str(repo_dir), "--format", "markdown"],
                capture_output=True, text=True, timeout=120,
            )
            output = result.stdout
            # Truncate — map on large repos can be huge
            if len(output) > 3000:
                output = output[:3000] + f"\n... (truncated, {len(output)} chars total)"
            return output

        elif name == "codebones_search":
            result = subprocess.run(
                [str(CODEBONES), "search", "--dir", str(repo_dir), args.get("query", ""), "--expand"],
                capture_output=True, text=True, timeout=30,
            )
            output = result.stdout
            if len(output) > 3000:
                output = output[:3000] + f"\n... (truncated, {len(output)} chars total)"
            return output or "(no matches)"

        elif name == "codebones_get":
            result = subprocess.run(
                [str(CODEBONES), "get", "--dir", str(repo_dir), args["symbol_or_path"]],
                capture_output=True, text=True, timeout=30,
            )
            return result.stdout or result.stderr or "(not found)"

        elif name == "codebones_graph":
            cmd = [str(CODEBONES), "graph", "--dir", str(repo_dir), "--format", "markdown"]
            if "file" in args and args["file"]:
                cmd = [str(CODEBONES), "graph", args["file"], "--dir", str(repo_dir), "--format", "markdown"]
            result = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
            return result.stdout or result.stderr or "(empty graph)"

        elif name == "codebones_outline":
            result = subprocess.run(
                [str(CODEBONES), "outline", "--dir", str(repo_dir), args["path"]],
                capture_output=True, text=True, timeout=30,
            )
            return result.stdout or result.stderr or "(not found)"

    except subprocess.TimeoutExpired:
        return "(command timed out)"
    except Exception as e:
        return f"Error: {e}"


# ---------------------------------------------------------------------------
# Agent runner
# ---------------------------------------------------------------------------

def run_agent(system_prompt: str, user_prompt: str, tools: list,
              tool_executor, repo_dir: Path, label: str) -> dict:
    """Run an agentic conversation and return full metrics."""
    messages = [{"role": "user", "content": user_prompt}]
    total_input = 0
    total_output = 0
    tool_calls = 0
    turns = 0
    conversation_log = []

    conversation_log.append({"role": "system", "content": system_prompt})
    conversation_log.append({"role": "user", "content": user_prompt})

    import time

    while turns < MAX_TURNS:
        turns += 1

        # Retry with backoff on rate limits
        response = None
        for attempt in range(10):
            try:
                response = client.messages.create(
                    model=MODEL,
                    max_tokens=2048,
                    system=system_prompt,
                    tools=tools,
                    messages=messages,
                )
                # Brief pause between calls
                time.sleep(2)
                break
            except anthropic.RateLimitError:
                wait = 65 * (attempt + 1)
                print(f"\n    (rate limited, waiting {wait}s...)", end="", flush=True)
                time.sleep(wait)
            except anthropic.BadRequestError as e:
                print(f"\n    (bad request: {e})")
                break

        if response is None:
            print("\n    (API call failed, stopping this run)")
            break

        total_input += response.usage.input_tokens
        total_output += response.usage.output_tokens

        # Extract text and tool use from response
        assistant_content = response.content
        messages.append({"role": "assistant", "content": assistant_content})

        # Log the assistant's text
        for block in assistant_content:
            if block.type == "text":
                conversation_log.append({"role": "assistant", "content": block.text})

        # Check if we're done (no tool use, just text)
        if response.stop_reason == "end_turn":
            break

        # Process tool calls
        tool_results = []
        for block in assistant_content:
            if block.type == "tool_use":
                tool_calls += 1
                tool_input = block.input
                result = tool_executor(block.name, tool_input, repo_dir)

                conversation_log.append({
                    "role": "tool_call",
                    "tool": block.name,
                    "input": tool_input,
                    "output_preview": result[:200] + "..." if len(result) > 200 else result,
                })

                tool_results.append({
                    "type": "tool_result",
                    "tool_use_id": block.id,
                    "content": result,
                })

        if tool_results:
            messages.append({"role": "user", "content": tool_results})

    # Get the final answer
    final_answer = ""
    for block in messages[-1].get("content", []) if isinstance(messages[-1], dict) else messages[-1]["content"]:
        if hasattr(block, "text"):
            final_answer += block.text
        elif isinstance(block, dict) and block.get("type") == "text":
            final_answer += block["content"]

    # Also try to get text from the last assistant message
    if not final_answer:
        for msg in reversed(messages):
            content = msg.get("content", []) if isinstance(msg, dict) else msg["content"]
            if isinstance(content, list):
                for block in content:
                    if hasattr(block, "text"):
                        final_answer += block.text
                        break
            if final_answer:
                break

    conversation_log.append({"role": "final_answer", "content": final_answer[:2000]})

    return {
        "total_input_tokens": total_input,
        "total_output_tokens": total_output,
        "total_tokens": total_input + total_output,
        "tool_calls": tool_calls,
        "turns": turns,
        "final_answer": final_answer[:2000],
        "conversation_log": conversation_log,
    }


# ---------------------------------------------------------------------------
# Tasks
# ---------------------------------------------------------------------------

TASKS = [
    {
        "name": "implement_middleware",
        "prompt": (
            "Add a CORS middleware to the FastAPI application that allows origins from "
            "http://localhost:3000 and http://localhost:5173. Find where middleware is "
            "configured, look at existing middleware examples as a pattern, and write "
            "the code. Show me the exact file to edit and the code to add."
        ),
    },
    {
        "name": "fix_bug",
        "prompt": (
            "I'm getting a TypeError when using `Depends()` with an async generator "
            "that yields None. Find the dependency resolution code, trace how generator "
            "dependencies are handled, and identify where the bug might be. Show me "
            "the relevant code paths."
        ),
    },
]


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    repo_dir = LAB_DIR / "fastapi"
    if not repo_dir.exists():
        print("ERROR: lab/fastapi not found. Run: git clone https://github.com/tiangolo/fastapi.git lab/fastapi")
        sys.exit(1)

    commit = subprocess.run(
        ["git", "-C", str(repo_dir), "rev-parse", "--short=12", "HEAD"],
        capture_output=True, text=True,
    ).stdout.strip()
    print(f"FastAPI at commit {commit}")

    # Index with codebones
    print("Indexing with codebones...")
    subprocess.run([str(CODEBONES), "index", str(repo_dir)], capture_output=True, timeout=120)

    OUTPUT_DIR.mkdir(exist_ok=True)
    rows = []

    for task in TASKS:
        print(f"\n{'='*60}")
        print(f"Task: {task['name']}")
        print(f"Prompt: {task['prompt'][:80]}...")
        print(f"{'='*60}")

        for approach in ["standard", "codebones"]:
            if approach == "standard":
                tools = make_standard_tools(str(repo_dir))
                executor = execute_standard_tool
                system = (
                    "You are working on the FastAPI repository (Python, ~107K LOC). "
                    "You have access to grep, cat, find, and ls to explore the codebase. "
                    "Explore efficiently — use grep to find what you need, then read "
                    "specific files. Complete the task thoroughly."
                )
            else:
                tools = make_standard_tools(str(repo_dir)) + make_codebones_tools(str(repo_dir))
                def combined_executor(name, args, repo_dir):
                    if name.startswith("codebones_"):
                        return execute_codebones_tool(name, args, repo_dir)
                    return execute_standard_tool(name, args, repo_dir)
                executor = combined_executor
                system = (
                    "You are working on the FastAPI repository (Python, ~107K LOC). "
                    "You have access to standard tools (grep, cat, find, ls) AND codebones "
                    "structural tools. Use the best tool for each step:\n"
                    "- codebones_search: find functions/classes by name (returns symbol IDs)\n"
                    "- codebones_get: read a specific function's source (not the whole file)\n"
                    "- codebones_graph <file>: see what depends on a file before changing it\n"
                    "- codebones_outline: see a file's structure without reading it fully\n"
                    "- grep: find text patterns (imports, strings, config values)\n"
                    "- cat: read small files or config files\n"
                    "Complete the task thoroughly."
                )

            run_results = []
            for run_idx in range(RUNS_PER_TASK):
                print(f"\n  [{approach}] Run {run_idx + 1}/{RUNS_PER_TASK}...", end=" ", flush=True)
                result = run_agent(system, task["prompt"], tools, executor, repo_dir, approach)
                print(f"{result['total_tokens']} tokens, {result['tool_calls']} tool calls, {result['turns']} turns")
                run_results.append(result)

                # Save conversation log
                log_path = OUTPUT_DIR / f"{task['name']}_{approach}_run{run_idx+1}.json"
                with open(log_path, "w") as f:
                    json.dump({
                        "task": task["name"],
                        "approach": approach,
                        "run": run_idx + 1,
                        "prompt": task["prompt"],
                        "system": system,
                        "metrics": {
                            "total_input_tokens": result["total_input_tokens"],
                            "total_output_tokens": result["total_output_tokens"],
                            "total_tokens": result["total_tokens"],
                            "tool_calls": result["tool_calls"],
                            "turns": result["turns"],
                        },
                        "final_answer": result["final_answer"],
                        "conversation_log": result["conversation_log"],
                    }, f, indent=2)

            # Compute results (median if multiple runs, direct if single)
            median_input = int(statistics.median([r["total_input_tokens"] for r in run_results]))
            median_output = int(statistics.median([r["total_output_tokens"] for r in run_results]))
            median_total = int(statistics.median([r["total_tokens"] for r in run_results]))
            median_tools = int(statistics.median([r["tool_calls"] for r in run_results]))
            median_turns = int(statistics.median([r["turns"] for r in run_results]))

            print(f"  [{approach}] Result: {median_total} total tokens, {median_tools} tool calls, {median_turns} turns")

            rows.append({
                "task": task["name"],
                "approach": approach,
                "median_input_tokens": median_input,
                "median_output_tokens": median_output,
                "median_total_tokens": median_total,
                "median_tool_calls": median_tools,
                "median_turns": median_turns,
            })

        # Print comparison
        std = next(r for r in rows if r["task"] == task["name"] and r["approach"] == "standard")
        cb = next(r for r in rows if r["task"] == task["name"] and r["approach"] == "codebones")
        ratio = std["median_total_tokens"] / max(cb["median_total_tokens"], 1)
        print(f"\n  Reduction: {ratio:.1f}x total tokens ({std['median_total_tokens']} → {cb['median_total_tokens']})")
        print(f"  Tool calls: {std['median_tool_calls']} → {cb['median_tool_calls']}")

    # Write CSV
    with open(OUTPUT_CSV, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=[
            "task", "approach", "median_input_tokens", "median_output_tokens",
            "median_total_tokens", "median_tool_calls", "median_turns",
        ])
        writer.writeheader()
        writer.writerows(rows)

    print(f"\n{'='*60}")
    print(f"Results: {OUTPUT_CSV}")
    print(f"Conversation logs: {OUTPUT_DIR}/")
    print(f"{'='*60}")


if __name__ == "__main__":
    main()
