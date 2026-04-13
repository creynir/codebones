#!/usr/bin/env python3
"""
Tier 2 Token Savings Benchmark — LLM Evaluation

Sends identical tasks to Claude Sonnet and Opus with two context strategies:
1. Raw source files (up to context limit)
2. codebones output (map, graph, search+get)

Measures actual API token usage from the response `usage` field.

Requires: ANTHROPIC_API_KEY environment variable, pip install anthropic
"""

import json
import os
import subprocess
import sys
import csv
from pathlib import Path

try:
    import anthropic
except ImportError:
    print("ERROR: pip install anthropic")
    sys.exit(1)

SCRIPT_DIR = Path(__file__).parent
REPO_ROOT = SCRIPT_DIR.parent.parent
LAB_DIR = REPO_ROOT / "lab"
OUTPUT_CSV = SCRIPT_DIR / "llm-eval.csv"
CODEBONES = REPO_ROOT / "target" / "release" / "codebones"

if not CODEBONES.exists():
    print(f"ERROR: {CODEBONES} not found. Run: cargo build --release -p codebones")
    sys.exit(1)

MODELS = ["claude-sonnet-4-6", "claude-opus-4-6"]

DATASETS = [
    ("small", "agenthelm"),
    # Skip medium/large for LLM eval — too many tokens for raw context
]

client = anthropic.Anthropic()


def run_codebones(*args):
    result = subprocess.run(
        [str(CODEBONES)] + list(args),
        capture_output=True, text=True, timeout=120
    )
    return result.stdout


def count_source_tokens_approx(repo_dir):
    """Rough count of source file content for display."""
    extensions = {'.py', '.rs', '.go', '.ts', '.tsx', '.js', '.jsx', '.java',
                  '.c', '.h', '.cpp', '.hpp', '.cs', '.rb', '.php', '.swift'}
    total = 0
    for f in repo_dir.rglob('*'):
        if f.suffix in extensions and 'node_modules' not in str(f) and '.git' not in str(f):
            try:
                total += len(f.read_text(errors='replace'))
            except Exception:
                pass
    return total // 4  # rough token estimate


def get_raw_context(repo_dir, max_chars=100000):
    """Concatenate source files up to max_chars."""
    extensions = {'.py', '.rs', '.go', '.ts', '.tsx', '.js', '.jsx'}
    files = sorted(repo_dir.rglob('*'))
    context = []
    total = 0
    for f in files:
        if f.suffix in extensions and 'node_modules' not in str(f) and '.git' not in str(f):
            try:
                content = f.read_text(errors='replace')
                if total + len(content) > max_chars:
                    break
                rel = f.relative_to(repo_dir)
                context.append(f"=== {rel} ===\n{content}")
                total += len(content)
            except Exception:
                pass
    return "\n\n".join(context)


def call_model(model, system_msg, user_msg):
    """Call Claude API and return usage stats."""
    try:
        response = client.messages.create(
            model=model,
            max_tokens=1024,
            system=system_msg,
            messages=[{"role": "user", "content": user_msg}]
        )
        return {
            "input_tokens": response.usage.input_tokens,
            "output_tokens": response.usage.output_tokens,
            "total_tokens": response.usage.input_tokens + response.usage.output_tokens,
            "response": response.content[0].text[:200],
        }
    except Exception as e:
        return {"input_tokens": 0, "output_tokens": 0, "total_tokens": 0, "response": f"ERROR: {e}"}


def main():
    rows = []

    for label, repo_name in DATASETS:
        repo_dir = LAB_DIR / repo_name
        if not repo_dir.exists():
            print(f"SKIP: {repo_dir} not found")
            continue

        print(f"\n=== {label} ({repo_name}) ===")

        # Ensure indexed
        run_codebones("index", str(repo_dir))

        # Prepare contexts
        raw_context = get_raw_context(repo_dir)
        map_context = run_codebones("map", str(repo_dir), "--format", "markdown")
        graph_context = run_codebones("graph", "--dir", str(repo_dir), "--format", "markdown")

        # Task 1: Architecture orientation
        task1 = "Describe the architecture of this project in 3-5 bullet points. What are the main modules and how do they relate?"

        # Task 2: Impact analysis (pick a file from the graph)
        graph_json = run_codebones("graph", "--dir", str(repo_dir), "--format", "json")
        try:
            graph_data = json.loads(graph_json)
            hot_file = graph_data["files"][0]["path"] if graph_data["files"] else "main.py"
        except Exception:
            hot_file = "main.py"

        blast_context = run_codebones("graph", hot_file, "--dir", str(repo_dir), "--format", "markdown")
        task2 = f"What files would be affected if I changed `{hot_file}`? List them and explain why."

        # Task 3: Symbol retrieval
        symbols = run_codebones("search", "--dir", str(repo_dir), "").strip().split("\n")[:1]
        if symbols and symbols[0]:
            symbol_id = symbols[0]
            symbol_name = symbol_id.split("::")[-1] if "::" in symbol_id else symbol_id
            get_context = run_codebones("search", "--dir", str(repo_dir), symbol_name)
            get_context += "\n" + run_codebones("get", "--dir", str(repo_dir), symbol_id)
            task3 = f"Explain what the `{symbol_name}` function/class does and how it fits into the codebase."
        else:
            symbol_name = "main"
            get_context = map_context
            task3 = "Explain the main entry point of this project."

        tasks = [
            ("orientation", task1, raw_context, map_context),
            ("impact_analysis", task2, raw_context, blast_context),
            ("symbol_retrieval", task3, raw_context, get_context),
        ]

        for model in MODELS:
            model_short = "sonnet" if "sonnet" in model else "opus"
            print(f"\n  Model: {model_short}")

            for task_name, task_prompt, raw_ctx, cb_ctx in tasks:
                # Raw context
                print(f"    {task_name} (raw)...", end=" ", flush=True)
                raw_result = call_model(
                    model,
                    "You are analyzing a codebase. Here are the source files:\n\n" + raw_ctx,
                    task_prompt
                )
                print(f"{raw_result['total_tokens']} tokens")

                rows.append({
                    "dataset": label,
                    "task": task_name,
                    "model": model_short,
                    "method": "raw_source",
                    "input_tokens": raw_result["input_tokens"],
                    "output_tokens": raw_result["output_tokens"],
                    "total_tokens": raw_result["total_tokens"],
                })

                # Codebones context
                print(f"    {task_name} (codebones)...", end=" ", flush=True)
                cb_result = call_model(
                    model,
                    "You are analyzing a codebase. Here is the codebones structural context:\n\n" + cb_ctx,
                    task_prompt
                )
                print(f"{cb_result['total_tokens']} tokens")

                rows.append({
                    "dataset": label,
                    "task": task_name,
                    "model": model_short,
                    "method": "codebones",
                    "input_tokens": cb_result["input_tokens"],
                    "output_tokens": cb_result["output_tokens"],
                    "total_tokens": cb_result["total_tokens"],
                })

                # Reduction
                if cb_result["input_tokens"] > 0:
                    ratio = raw_result["input_tokens"] / cb_result["input_tokens"]
                    print(f"    → {ratio:.1f}x input token reduction")

    # Write CSV
    with open(OUTPUT_CSV, "w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=["dataset", "task", "model", "method",
                                                "input_tokens", "output_tokens", "total_tokens"])
        writer.writeheader()
        writer.writerows(rows)

    print(f"\nResults written to {OUTPUT_CSV}")


if __name__ == "__main__":
    main()
