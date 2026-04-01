#!/usr/bin/env python3

import re
import sys
from pathlib import Path


def extract_version(path: Path, pattern: str) -> str:
    content = path.read_text(encoding="utf-8")
    match = re.search(pattern, content, re.MULTILINE | re.DOTALL)
    if match is None:
        raise RuntimeError(f"could not find version in {path}")
    return match.group(1)


def main() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    cargo_toml = repo_root / "crates" / "python-ext" / "Cargo.toml"
    pyproject_toml = repo_root / "crates" / "python-ext" / "pyproject.toml"

    cargo_version = extract_version(cargo_toml, r'^version\s*=\s*"([^"]+)"')
    pyproject_version = extract_version(
        pyproject_toml,
        r"^\[project\]\s.*?^version\s*=\s*\"([^\"]+)\"",
    )

    if cargo_version != pyproject_version:
        raise RuntimeError(
            "python package version drift detected: "
            f"Cargo.toml={cargo_version}, pyproject.toml={pyproject_version}"
        )

    print(f"python package versions match: {cargo_version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
