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

The Rust baselines of the benchmarks live in their own package outside the workspace, with their own checks:

```sh
cargo fmt --check --manifest-path benchmarks/rust/Cargo.toml
cargo clippy --manifest-path benchmarks/rust/Cargo.toml --all-targets -- -D warnings
```

## Documentation

The documentation is built with Sphinx, MyST Markdown and the Furo theme, one Markdown file per page in `docs/`. The build runs every Python code block of a page in order, and a block that raises fails the build. The build output lands in `docs/_build/html`.

## Benchmarks

The benchmark suites measure other parsers today, and fastpeg as soon as it parses. Python suites run under [pyperf](https://pyperf.readthedocs.io), which measures in several worker processes, and the Rust suite under [criterion](https://docs.rs/criterion).

```sh
uv run --group bench python benchmarks/run.py
```

This prepares the inputs, runs every suite and writes a summary and the raw results to `benchmarks/.cache/last-run`. The first run downloads one input from GitHub and checks it against a pinned hash.

| Option | Effect |
|---|---|
| `--suite json` | run one suite: `json`, `python`, `ord` or `rust`. Repeat it for several |
| `--mode fast` | fewer worker processes and values, for a rough answer in less time |
| `--mode rigorous` | twice the worker processes |
| `--mode smoke` | one value per benchmark, to check that every suite runs |
| `--record` | write to `benchmarks/results/<date>-<platform>` instead, to commit the run |

A full run takes about ten minutes. Close other programs while it runs, and record only on a quiet machine. `benchmarks/README.md` describes the files a run writes.

## Layout

```text
crates/fastpeg/       core: grammar front end, analysis, compiler, parsing machine, tree
crates/fastpeg-cli/   the fastpeg command
crates/fastpeg-py/    the native module of the Python package, built with maturin
python/fastpeg/       the Python package and its type stubs
tests/                Python tests and tests that span crates
benchmarks/           benchmark harness, baseline suites, recorded results
docs/                 this documentation
scripts/              development helpers
```
