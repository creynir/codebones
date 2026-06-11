#!/usr/bin/env bash
#
# Verify the release tag matches the version in every versioned file, so a
# `vX.Y.Z` tag can never publish artifacts built from a different version.
#
# Usage: check_tag_version.sh <tag>   (tag may include a leading 'v';
#        falls back to $GITHUB_REF_NAME when no argument is given)
set -euo pipefail

tag="${1:-${GITHUB_REF_NAME:-}}"
tag="${tag#v}"
if [ -z "$tag" ]; then
  echo "error: no tag provided (pass as an argument or set GITHUB_REF_NAME)" >&2
  exit 2
fi

root="$(cd "$(dirname "$0")/.." && pwd)"
fail=0

check() {
  local file="$1"
  local got
  got="$(grep -m1 -E '^version = ' "$root/$file" | sed -E 's/^version = "([^"]+)".*/\1/')"
  if [ "$got" != "$tag" ]; then
    echo "::error::$file has version '$got' but the tag is 'v$tag'"
    fail=1
  fi
}

check crates/core/Cargo.toml
check crates/cli/Cargo.toml
check crates/mcp/Cargo.toml
check crates/python-ext/Cargo.toml
check crates/python-ext/pyproject.toml

if [ "$fail" -ne 0 ]; then
  echo "Tag/version mismatch: bump the versioned files to $tag before pushing the tag." >&2
  exit 1
fi

echo "All versioned files match tag v$tag"
