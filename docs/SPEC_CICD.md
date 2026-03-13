# CI/CD Specifications for codebones

This document outlines the GitHub Actions workflows to be added to `.github/workflows/` for the `codebones` project.

## Required Secrets

Before enabling these workflows, ensure the following secrets are configured in your GitHub repository settings:
- `CARGO_REGISTRY_TOKEN`: Required for publishing crates to crates.io.
- `PYPI_API_TOKEN`: Required for publishing native python wheels to PyPI.

---

## 1. `test.yml`

This workflow runs tests, formatting checks, and linting on every push and pull request.

```yaml
name: Test

on:
  push:
    branches: [ "main" ]
  pull_request:
    branches: [ "main" ]

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Check Formatting
        run: cargo fmt --all -- --check

      - name: Run Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: Run Tests
        run: cargo test --all-features
```

---

## 2. `release-rust.yml`

This workflow runs on new tags (e.g., `v*`). It cross-compiles the `codebones-cli` and `codebones-mcp` binaries for Linux, macOS, and Windows, uploads them to GitHub Releases, and publishes the packages to crates.io.

```yaml
name: Release Rust

on:
  push:
    tags:
      - 'v*'

permissions:
  contents: write

jobs:
  build-and-release:
    name: Build and Release Binaries
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            os_name: linux-amd64
          - os: macos-latest
            target: x86_64-apple-darwin
            os_name: macos-amd64
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            os_name: windows-amd64

    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Build CLI and MCP
        run: |
          cargo build --release --bin codebones-cli --target ${{ matrix.target }}
          cargo build --release --bin codebones-mcp --target ${{ matrix.target }}

      - name: Prepare Assets (Unix)
        if: matrix.os != 'windows-latest'
        run: |
          mv target/${{ matrix.target }}/release/codebones-cli codebones-cli-${{ matrix.os_name }}
          mv target/${{ matrix.target }}/release/codebones-mcp codebones-mcp-${{ matrix.os_name }}

      - name: Prepare Assets (Windows)
        if: matrix.os == 'windows-latest'
        run: |
          mv target/${{ matrix.target }}/release/codebones-cli.exe codebones-cli-${{ matrix.os_name }}.exe
          mv target/${{ matrix.target }}/release/codebones-mcp.exe codebones-mcp-${{ matrix.os_name }}.exe

      - name: Upload to GitHub Release
        uses: softprops/action-gh-release@v1
        with:
          files: |
            codebones-cli-*
            codebones-mcp-*

  publish-crates:
    name: Publish to Crates.io
    runs-on: ubuntu-latest
    needs: build-and-release
    steps:
      - uses: actions/checkout@v4
      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
      - name: Publish crates
        env:
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
        run: |
          cargo publish -p codebones-cli
          cargo publish -p codebones-mcp
```

---

## 3. `release-python.yml`

This workflow uses `maturin-action` to build native Python wheels for Linux, macOS, and Windows on new tags, and publishes them to PyPI.

```yaml
name: Release Python

on:
  push:
    tags:
      - 'v*'

jobs:
  build-wheels:
    name: Build wheels on ${{ matrix.os }}
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Setup Python
        uses: actions/setup-python@v5
        with:
          python-version: '3.10'

      - name: Build wheels
        uses: PyO3/maturin-action@v1
        with:
          target: all
          args: --release --out dist

      - name: Upload wheels
        uses: actions/upload-artifact@v4
        with:
          name: wheels-${{ matrix.os }}
          path: dist

  release:
    name: Publish to PyPI
    runs-on: ubuntu-latest
    needs: build-wheels
    steps:
      - uses: actions/download-artifact@v4
        with:
          path: dist
          pattern: wheels-*
          merge-multiple: true

      - name: Publish to PyPI
        uses: pypa/gh-action-pypi-publish@release/v1
        with:
          password: ${{ secrets.PYPI_API_TOKEN }}
```
