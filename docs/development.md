# Development

How to build fastpeg from a checkout and run the checks. The working rules for contributors, people and agents alike, are in `AGENTS.md` in the repository root.

## Requirements

- [rustup](https://rustup.rs). The Rust version is pinned in `rust-toolchain.toml`, and rustup installs it on first use. On Windows Rust needs the Microsoft C++ Build Tools.
- [uv](https://docs.astral.sh/uv/). It installs the Python version named in `.python-version` and every Python tool the project uses.
- Git.

## Setup

```sh
uv sync
```

This creates `.venv`, builds the native module with maturin, installs the package in editable mode and installs the development tools. `uv run` rebuilds the native module when a Rust source has changed, so there is no separate build step.

## Checks

Every change passes these checks before it is committed. Continuous integration runs the same commands on Linux, Windows and macOS and treats warnings as errors.

| Check | Command |
|---|---|
| Rust formatting | `cargo fmt --check` |
| Rust lints | `cargo clippy --all-targets -- -D warnings` |
| Rust tests | `cargo test` |
| Python formatting | `uv run ruff format --check` |
| Python lints | `uv run ruff check` |
| Python types | `uv run pyright` |
| Python tests | `uv run pytest` |
| Documentation | `uv run --group docs sphinx-build -W --keep-going -b html docs docs/_build/html` |
| Licence headers | `uv run python scripts/check_licence_headers.py` |

## Documentation

The documentation is built with Sphinx, MyST Markdown and the Furo theme, one Markdown file per page in `docs/`. The build runs every Python code block of a page in order, and a block that raises fails the build. The build output lands in `docs/_build/html`.

## Layout

```text
crates/fastpeg/       core: grammar front end, analysis, compiler, parsing machine, tree
crates/fastpeg-cli/   the fastpeg command
crates/fastpeg-py/    the native module of the Python package, built with maturin
python/fastpeg/       the Python package and its type stubs
tests/                Python tests and tests that span crates
docs/                 this documentation
scripts/              development helpers
```
