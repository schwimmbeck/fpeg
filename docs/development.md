# Development

How to build fastpeg from a checkout and run the checks. The working rules for contributors, people and agents alike, are in `AGENTS.md` in the repository root.

## Requirements

- [rustup](https://rustup.rs). The Rust version is pinned in `rust-toolchain.toml`, and rustup installs it on first use. On Windows Rust needs the Microsoft C++ Build Tools.
- [uv](https://docs.astral.sh/uv/). It installs the Python version named in `.python-version` and every Python tool the project uses.
- Git.

## Setup

```sh
git clone https://github.com/schwimmbeck/fastpeg
cd fastpeg
uv sync
```

This creates `.venv`, builds the native module with maturin, installs the package in editable mode and installs the development tools. `uv run` rebuilds the native module when a Rust source has changed, so there is no separate build step.

## Checks

Every change passes these checks before it is committed. Continuous integration runs the same commands and treats warnings as errors: the Rust and Python tests on Linux with the oldest supported Python and on Windows and macOS with the newest, and all other checks, with the Python tests on the development version, on Linux. Once a month Dependabot proposes updates of the pinned actions, the crates and the Python lock file, one pull request for each.

| Check | Command |
|---|---|
| Rust formatting | `cargo fmt --check` |
| Rust lints | `cargo clippy --all-targets -- -D warnings` |
| Rust lints without tests | `cargo clippy --workspace --lib --bins -- -D warnings` |
| Rust tests | `cargo test` |
| Python formatting | `uv run ruff format --check` |
| Python lints | `uv run ruff check` |
| Python types | `uv run pyright --warnings` |
| Python tests | `uv run pytest` |
| Documentation | `uv run --group docs sphinx-build -W --keep-going -b html docs docs/_build/html` |
| Licence headers | `uv run python scripts/check_licence_headers.py` |
| REUSE compliance | `uv run reuse lint` |

Every file states its copyright and licence, in a header or, for files that cannot carry one, in `REUSE.toml`, and `LICENSES/` holds the text of every licence used, as the [REUSE specification](https://reuse.software/spec/) asks.

`uv run python scripts/check.py` runs all of these in order, together with the checks of the benchmark crate below, and stops at the first failure.

The Rust baselines of the benchmarks live in their own package outside the workspace, with their own checks:

```sh
cargo fmt --check --manifest-path benchmarks/rust/Cargo.toml
cargo clippy --manifest-path benchmarks/rust/Cargo.toml --all-targets -- -D warnings
```

## Tests of the core

The tests of the core crate run on the reference interpreter, and the differential tests compare the parsing machine with it.

| Test | What it checks |
|---|---|
| `tests/semantics/*.txt` | case files: a grammar, inputs, and the expected tree or error for each. Every grammar must also print to text that reads back the same |
| `tests/acceptance.rs` | every grammar in `crates/fastpeg/grammars/` prints back to the same grammar and parses its samples, the samples in `invalid/` fail, and a `.tree` file beside a sample holds its expected tree |
| `tests/generated.rs` | generated grammars and inputs, made by `tests/common/generator.rs`: nothing panics, every parse ends within the limits, printing reads back, and memoization never changes a result |
| `tests/differential.rs` | the parsing machine against the reference interpreter on every case file, every acceptance grammar with its samples, and generated grammars: the same tree, or the same error with the same expected set. Grammars the machine does not run yet are skipped, and each test reports how many |
| `tests/mutated.rs` | the grammars of `crates/fastpeg/grammars/` with one random edit each: reading, resolving and parsing samples never panic, and an edited grammar that reads prints back to the same grammar |
| `tests/docs.rs` | every `fpeg` block of the documentation outside the diagnostics pages |
| `tests/diagnostics.rs` | every diagnostic code has its page in `docs/reference/diagnostics/`: the page's grammar reports its code, its text block holds exactly what `fastpeg check` prints, and its fix compiles |
| `tests/limits.rs` | the limits on parses and on grammars: depth, steps, nesting, template expansion, the number of names, instances and errors, and printing a tree as deep as its input |
| `src/vm/tests.rs` | the code the compiler makes for small grammars, exact character classes, the verifier against broken programs, and what the machine does not run yet |

`FASTPEG_BLESS=1 cargo test` rewrites the expected results of the case files, the `.tree` files and the messages on the diagnostics pages, which are then reviewed before a commit. `FASTPEG_GENERATED_CASES` sets how many generated grammars to try instead of 400, and `FASTPEG_GENERATED_SEED` the first seed, which replays a reported failure. `FASTPEG_MUTANTS` and `FASTPEG_MUTANT_SEED` do the same for the edited grammars. Long runs are faster with `cargo test --release`.

## Documentation

The documentation is built with Sphinx, MyST Markdown and the Furo theme, one Markdown file per page in `docs/`. The build runs every Python code block of a page in order, and a block that raises fails the build. `cargo test` reads every `fpeg` grammar block, and resolves the ones that define a `start` rule. The build output lands in `docs/_build/html`.

The logo lives in `docs/_static/`: `logo-light.svg` and `logo-dark.svg` for light and dark pages, used by the documentation and by `README.md`, and `favicon.svg`, a heavier cut of the mark that follows the browser's colour scheme by itself.

## Benchmarks

The Rust suite measures fastpeg's parsing machine beside other Rust parsers, and the Python suites measure other parsers until the Python binding exists. Python suites run under [pyperf](https://pyperf.readthedocs.io), which measures in several worker processes, and the Rust suite under [criterion](https://docs.rs/criterion).

```sh
uv run --group bench python benchmarks/run.py
```

This prepares the inputs, runs every suite and writes a summary and the raw results to `benchmarks/.cache/last-run`. The first run downloads one input from GitHub. Every input is checked against a pinned hash, and one that is missing or damaged is made again.

| Option | Effect |
|---|---|
| `--suite json` | run one suite: `json`, `python`, `ord` or `rust`. Repeat it for several |
| `--mode fast` | fewer pyperf worker processes and values, and criterion's quick mode, for a rough answer in less time |
| `--mode rigorous` | twice the pyperf worker processes. The Rust suite measures as in the default mode |
| `--mode smoke` | one value per Python benchmark and criterion's quick mode, to check that every suite runs |
| `--record` | move the finished run to `benchmarks/results/<date>-<platform>` to commit it. It needs every suite and the default or rigorous mode |

A full run takes several minutes. Close other programs while it runs, and record only on a quiet machine. `benchmarks/README.md` describes the files a run writes.

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
