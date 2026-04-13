#!/usr/bin/env bash
set -euo pipefail

# Token Savings Benchmark
# Measures codebones token reduction vs reading raw source files.
#
# Prerequisites:
#   - codebones binary in PATH (cargo install --path crates/cli)
#   - Python 3 with tiktoken: pip install tiktoken
#   - git
#
# Datasets are automatically cloned into lab/ on first run.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LAB_DIR="$REPO_ROOT/lab"
OUTPUT_CSV="$SCRIPT_DIR/token-savings.csv"

# Use locally built binary if available, otherwise expect codebones in PATH
if [ -x "$REPO_ROOT/target/release/codebones" ]; then
    CODEBONES="$REPO_ROOT/target/release/codebones"
elif command -v codebones &>/dev/null; then
    CODEBONES="codebones"
else
    echo "ERROR: codebones not found. Run: cargo build --release -p codebones"
    exit 1
fi

# Dataset definitions: label,git_url,pinned_commit
DATASETS=(
    "small,https://github.com/tiangolo/fastapi.git,eba8942c81db"
    "medium,https://github.com/temporalio/temporal.git,29a039286526"
    "large,https://github.com/n8n-io/n8n.git,f7a787aca81c"
)

# ---------------------------------------------------------------------------
# Setup: clone datasets into lab/ at pinned commits
# ---------------------------------------------------------------------------

setup_datasets() {
    mkdir -p "$LAB_DIR"

    for entry in "${DATASETS[@]}"; do
        IFS=',' read -r label git_url pinned_commit <<< "$entry"
        repo_name=$(basename "$git_url" .git)
        dir="$LAB_DIR/$repo_name"

        if [ -d "$dir" ]; then
            # Verify correct commit
            current=$(git -C "$dir" rev-parse --short=12 HEAD 2>/dev/null || echo "none")
            if [[ "$current" == "$pinned_commit"* ]]; then
                echo "[$label] $repo_name already at $pinned_commit"
                continue
            fi
            echo "[$label] $repo_name at $current, checking out $pinned_commit..."
            git -C "$dir" fetch --quiet 2>/dev/null || true
            git -C "$dir" checkout "$pinned_commit" --quiet 2>/dev/null
        else
            echo "[$label] Cloning $git_url..."
            git clone --quiet "$git_url" "$dir"
            git -C "$dir" checkout "$pinned_commit" --quiet 2>/dev/null
            echo "[$label] Checked out $pinned_commit"
        fi
    done

    echo ""
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

# Token counter using tiktoken (cl100k_base)
count_tokens() {
    python3 -c "
import sys, tiktoken
enc = tiktoken.get_encoding('cl100k_base')
data = sys.stdin.read()
print(len(enc.encode(data)))
"
}

# Count tokens in all source files (excluding binaries, secrets, vendor, node_modules)
count_raw_tokens() {
    local dir="$1"
    find "$dir" -type f \
        \( -name '*.rs' -o -name '*.py' -o -name '*.go' -o -name '*.ts' -o -name '*.tsx' \
        -o -name '*.js' -o -name '*.jsx' -o -name '*.java' -o -name '*.c' -o -name '*.h' \
        -o -name '*.cpp' -o -name '*.hpp' -o -name '*.cs' -o -name '*.rb' -o -name '*.php' \
        -o -name '*.swift' \) \
        ! -path '*/node_modules/*' \
        ! -path '*/vendor/*' \
        ! -path '*/.git/*' \
        ! -path '*/target/*' \
        ! -path '*/.codebones/*' \
        -exec cat {} + | count_tokens
}

# Count tokens in a string/command output
tokens_of() {
    echo "$1" | count_tokens
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

echo "Setting up datasets..."
setup_datasets

echo "dataset,scenario,method,tokens,details" > "$OUTPUT_CSV"

# Disable exit-on-error for the measurement loop — individual command
# failures should be reported, not abort the entire benchmark.
set +e

for entry in "${DATASETS[@]}"; do
    IFS=',' read -r label git_url pinned_commit <<< "$entry"
    repo_name=$(basename "$git_url" .git)
    dir="$LAB_DIR/$repo_name"

    if [ ! -d "$dir" ]; then
        echo "SKIP: $dir not found"
        continue
    fi

    echo "=== $label ($repo_name) ==="

    # Index the repo
    echo "  Indexing..."
    $CODEBONES index "$dir" 2>/dev/null || { echo "  ERROR: index failed"; continue; }

    # --- Scenario 1: Project Orientation ---
    echo "  Scenario 1: Project Orientation"

    raw_tokens=$(count_raw_tokens "$dir")
    echo "    Raw source tokens: $raw_tokens"
    echo "$label,orientation,raw_source,$raw_tokens,all source files" >> "$OUTPUT_CSV"

    map_output=$($CODEBONES map "$dir" --format markdown 2>/dev/null)
    map_tokens=$(tokens_of "$map_output")
    echo "    Map tokens: $map_tokens"
    echo "$label,orientation,codebones_map,$map_tokens,codebones map --format markdown" >> "$OUTPUT_CSV"

    ratio=$(python3 -c "print(f'{$raw_tokens / max($map_tokens,1):.0f}x')")
    echo "    Reduction: $ratio"

    # --- Scenario 2: Impact Analysis ---
    echo "  Scenario 2: Impact Analysis"

    # Get top 3 hot files
    graph_output=$($CODEBONES graph --dir "$dir" --format json 2>/dev/null || echo '{"files":[]}')
    hot_files=$(echo "$graph_output" | python3 -c "
import sys, json
data = json.loads(sys.stdin.read())
files = data.get('files', [])[:3]
for f in files:
    print(f['path'])
" 2>/dev/null || echo "")

    if [ -n "$hot_files" ]; then
        echo "    Hot files: $(echo "$hot_files" | tr '\n' ', ')"
        while IFS= read -r hot_file; do
            [ -z "$hot_file" ] && continue
            blast_output=$($CODEBONES graph "$hot_file" --dir "$dir" --format markdown 2>/dev/null || echo "")
            blast_tokens=$(tokens_of "$blast_output")
            echo "    Blast radius ($hot_file): $blast_tokens tokens"
            echo "$label,impact_analysis,codebones_graph,$blast_tokens,$hot_file" >> "$OUTPUT_CSV"
        done <<< "$hot_files"
    fi

    echo "$label,impact_analysis,raw_source,$raw_tokens,must read all files to trace imports" >> "$OUTPUT_CSV"

    # --- Scenario 3: Symbol Retrieval ---
    echo "  Scenario 3: Symbol Retrieval"

    symbols=$($CODEBONES search --dir "$dir" "" 2>/dev/null | head -3)
    while IFS= read -r symbol_id; do
        [ -z "$symbol_id" ] && continue
        file_path=$(echo "$symbol_id" | sed 's/::.*//')
        symbol_name=$(echo "$symbol_id" | sed 's/.*:://')

        file_content=$($CODEBONES get --dir "$dir" "$file_path" 2>/dev/null || echo "")
        file_tokens=$(tokens_of "$file_content")

        search_output=$($CODEBONES search --dir "$dir" "$symbol_name" 2>/dev/null || echo "")
        get_output=$($CODEBONES get --dir "$dir" "$symbol_id" 2>/dev/null || echo "")
        combined="$search_output\n$get_output"
        cb_tokens=$(tokens_of "$combined")

        echo "    $symbol_name: file=$file_tokens tokens, codebones=$cb_tokens tokens"
        echo "$label,symbol_retrieval,raw_file,$file_tokens,$file_path" >> "$OUTPUT_CSV"
        echo "$label,symbol_retrieval,codebones_get,$cb_tokens,$symbol_id" >> "$OUTPUT_CSV"
    done <<< "$symbols"

    # --- Scenario 4: Budget Efficiency ---
    echo "  Scenario 4: Budget Efficiency"

    for budget in 8000 16000 32000 64000; do
        pack_output=$($CODEBONES pack "$dir" --format markdown --max-tokens "$budget" 2>/dev/null || echo "")
        pack_tokens=$(tokens_of "$pack_output")

        symbol_count=$(echo "$pack_output" | grep -cE '^\s*(- |  - |Function |Class |Struct |Impl |Method |Interface )' 2>/dev/null || echo "0")

        echo "    Budget $budget: $pack_tokens tokens, $symbol_count symbols visible"
        echo "$label,budget_efficiency,codebones_pack,$pack_tokens,max_tokens=$budget symbols_visible=$symbol_count" >> "$OUTPUT_CSV"
    done

    echo ""
done

echo "Results written to $OUTPUT_CSV"
