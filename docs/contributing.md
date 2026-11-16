---
orphan: true
---

# Contributing

How to build fpeg from a checkout, run the checks and work on its parts. The working rules for contributors, people and agents alike, are in [`AGENTS.md`](https://github.com/schwimmbeck/fpeg/blob/main/AGENTS.md) in the repository root.

## Requirements

- [rustup](https://rustup.rs). The Rust version is pinned in `rust-toolchain.toml`, and rustup installs it on first use. On Windows Rust needs the Microsoft C++ Build Tools.
- [uv](https://docs.astral.sh/uv/). It installs the Python version named in `.python-version` and every Python tool the project uses.
- Git.

## Setup

```sh
git clone https://github.com/schwimmbeck/fpeg
cd fpeg
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
| Rust API documentation | `cargo rustdoc -p fpeg -- -D warnings` |
| Rust tests | `cargo test` |
| Python formatting | `uv run ruff format --check` |
| Python lints | `uv run ruff check` |
| Python types | `uv run pyright --warnings` |
| Python stubs | `uv run python -m mypy.stubtest fpeg._fpeg` |
| Python tests | `uv run --group oracles pytest` |
| Documentation | `uv run --group docs sphinx-build -W --keep-going -b html docs docs/_build/html` |
| Licence headers | `uv run python scripts/check_licence_headers.py` |
| REUSE compliance | `uv run reuse lint` |

Every file states its copyright and licence, in a header or, for files that cannot carry one, in `REUSE.toml`, and `LICENSES/` holds the text of every licence used, as the [REUSE specification](https://reuse.software/spec/) asks.

The dependency group `oracles` holds what a test compares fpeg with beyond the standard library: `llvmlite`, which runs the LLVM IR that a test compiles a small language to, and the parser that the case study's language reads its programs with. A test skips without its oracle, so `uv run pytest` alone works as well.

Tests over a whole corpus carry the marker `corpus` and stay out of the default run: the Python grammar against CPython's `ast` over every file of the running Python's library, the Lua grammar over the test suite of Lua 5.4, and the ORD grammar over the 76 files of its application against the language's own parser, which needs the group `oracles`. `uv run python benchmarks/corpus.py` fetches the corpora that are not part of Python, a test whose corpus is missing skips, and `uv run --group oracles pytest -m corpus` runs them. Continuous integration runs them once a week.

`uv run python scripts/check.py` runs all of these in order, together with the checks of the benchmark and fuzz crates below, and stops at the first failure.

The Rust benchmarks and the fuzz targets live in packages of their own outside the workspace, with their own checks:

```sh
cargo fmt --check --manifest-path benchmarks/rust/Cargo.toml
cargo clippy --manifest-path benchmarks/rust/Cargo.toml --all-targets -- -D warnings
cargo fmt --check --manifest-path fuzz/Cargo.toml
cargo clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings
```

## Fuzzing

The fuzz targets in `fuzz/` run under [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz), which needs a nightly toolchain and works best on Linux. `grammar` and `differential` read their input as a grammar text, then a NUL, then an input to parse. `generated` lets its input's bytes choose a grammar and its inputs through the generator of the tests, a byte for each choice, so that every input is a grammar, and its first eight bytes pick what the machine remembers and the passes it runs.

| Target | What it checks |
|---|---|
| `grammar` | reading, compiling and parsing end without a panic, whatever the grammar and the input |
| `differential` | the parsing machine gives the tree or error of the reference interpreter |
| `generated` | the same for generated grammars, in recovering mode too, at every setting of the machine |

```sh
cargo install cargo-fuzz
cd fuzz
cargo +nightly fuzz run differential corpus/differential ../crates/fpeg/grammars -- -dict=fpeg.dict -max_total_time=600
cargo +nightly fuzz run generated corpus/generated -- -max_total_time=600
```

The corpus folders come before `--` and libFuzzer's options after it. The grammars of the project serve as the first inputs of the targets that read grammar text. libFuzzer keeps what it finds in `fuzz/corpus/` and a failing input in `fuzz/artifacts/`, both ignored by git.

## Tests of the core

The tests of the core crate, in `crates/fpeg/`, run on the reference interpreter, and the differential tests compare the parsing machine with it. The paths below are within that folder, apart from the Python tests in the repository's own `tests/`.

| Test | What it checks |
|---|---|
| `tests/semantics/*.txt`, run by `tests/semantics.rs` | case files: a grammar, inputs, and the expected tree or error for each, in the format `tests/common/cases.rs` describes. Every grammar must also print to text that reads back the same |
| `tests/acceptance.rs` | every grammar in `crates/fpeg/grammars/` prints back to the same grammar and parses its samples, also with CR LF line breaks, the samples in `invalid/` fail, and a `.tree` file beside a sample holds its expected tree. The grammar of grammars also reads itself. A grammar without a rule `start` that extends no other, which others only extend or import from, resolves and has no samples |
| `tests/generated.rs` | generated grammars and inputs, made by `tests/common/generator.rs`: nothing panics, every parse ends within the limits, printing reads back, memoization never changes a result, and every parse keeps to the grammar's item counts: no node holds more items in a field than they allow or lacks a field of exactly one item, and no item is of a kind they say no parse makes |
| `tests/differential.rs` | the parsing machine against the reference interpreter on every case file, every acceptance grammar with its samples, and generated grammars: the same tree, or the same error with the same expected set, with the machine remembering what the grammar marks, nothing, and every rule. Each test reports how many parses it compared |
| `tests/mutated.rs` | the grammars of `crates/fpeg/grammars/` with one random edit each: reading, resolving and parsing samples never panic, and an edited grammar that reads prints back to the same grammar |
| `tests/docs.rs` | every `fpeg` block of the documentation outside the diagnostics pages reads, and one with a `start` rule gives no error and no warning |
| `tests/diagnostics.rs` | every diagnostic code has its page in `docs/reference/diagnostics/`: the page's grammar reports its code, its text block holds exactly what `fpeg check` prints, and its fix gives neither an error nor a warning |
| `tests/check.rs` | the warnings: each is given where it holds and left out where it does not, the acceptance grammars give none, and a very long choice and a very long chain of rules are checked in time |
| `tests/limits.rs` | the limits on parses and on grammars: depth, steps, nesting, template expansion, the number of names, instances and errors, and printing a tree as deep as its input |
| `tests/modules.rs` | grammars that extend and import others: the rules they replace, where `...` puts what they inherit, the names an import gives, and the problems of modules with their codes |
| `tests/recovery.rs` | recovery from single edits: the JSON grammar gives a broken text a tree that keeps most of its tokens, and names each mistake |
| `tests/render.rs` | errors shown with their line, as the command line and the Python package show them |
| `tests/profile.rs` | the counts of a profile: calls, repeats, failures, growths, the row of the ignored text, and that the rows sum to the steps of the parse |
| `src/vm/tests.rs` | the code the compiler makes for small grammars, exact character classes, and the verifier against broken programs |
| `src/grammar/bootstrap.rs` | the bootstrap of the front end, below: `meta.rs` is what the grammar file prints and builds the grammar the file describes, the parser of that grammar reads the file as the bootstrap does, and the reference page shows the file |

`FPEG_BLESS=1 cargo test` rewrites the expected results of the case files, the `.tree` files, the messages on the diagnostics pages and the bootstrap of the front end, which are then reviewed before a commit. `FPEG_GENERATED_CASES` sets how many generated grammars to try instead of 400, and `FPEG_GENERATED_SEED` the first seed, which replays a reported failure. `FPEG_MUTANTS` and `FPEG_MUTANT_SEED` do the same for the edited grammars. Long runs are faster with `cargo test --release`.

## The grammar of grammars

The text front end reads a grammar with a parser of `crates/fpeg/grammars/fpeg.fpeg`, the grammar of grammars in its own notation. That file is the source. The front end cannot read it before it can read grammars, so it starts from a copy as builder code, `crates/fpeg/src/grammar/meta.rs`, which the tests print from the file and which is never edited by hand. To change the language:

1. Change `fpeg.fpeg`, and the same block on the reference page of the grammar language.
2. Run `FPEG_BLESS=1 cargo test -p fpeg --lib bootstrap`, which rewrites `meta.rs`, and review its diff.
3. Run the tests, which now read every grammar with the new front end.

A form that the front end at hand cannot read takes two rounds: add it to the grammar in the notation that exists and bless, then use it. A blessed `meta.rs` that cannot read the grammar file any more leaves no front end to bless with. Restore it with `git checkout` and correct the text.

## Documentation

The documentation is built with Sphinx, MyST Markdown and the Furo theme, one Markdown file per page in `docs/`. The build runs every Python code block of a page in order, and a block that raises fails the build. A block in the language `output` right after a Python block shows what that block printed, and the build fails when it differs. `tests/test_readme.py` holds the Python blocks of `README.md` to the same rules. `cargo test` reads every `fpeg` grammar block, and checks the ones that define a `start` rule, which must give no warning. The build output lands in `docs/_build/html`.

The logo lives in `docs/_static/`: `logo-light.svg` and `logo-dark.svg` for light and dark pages, used by the documentation and by `README.md`, which loads them by absolute URLs since PyPI resolves no relative path, and `favicon.svg`, a heavier cut of the mark that follows the browser's colour scheme by itself.

Flow charts are SVG files in `docs/_diagrams/`, drawn by hand on a grid with the classes of `docs/_static/diagrams.css`: `box` for a step, `box accent` for the one step the figure is about, `arrow` for a path with the arrowhead marker, `title` and `code` for text. They hold no colour of their own, and the directive `diagram` puts one into its page inline, with its caption, so that the theme colours it in light and dark:

````md
```{diagram} pipeline.svg
What the figure shows, in one paragraph.
```
````

## Benchmarks

The Rust suite measures fpeg's parsing machine beside other Rust parsers on JSON, on an expression of twelve operator levels by what the machine remembers, and on ten workloads with every pass of the optimizer and with each one left out, so that each pass shows what it brings, Python, Lua and the outline language among them. The `json` suite measures fpeg's Python package beside other Python parsers, and the `binding` suite measures what the binding adds to a parse: transforms with and without methods, tokens made from views, and many small parses. The `calculator` suite computes lines of arithmetic with fpeg and other Python parsers beside Python's `eval`. The `python` suite measures fpeg's Python grammar beside CPython and other parsers of Python source, fpeg also to `ast` with positions stamped natively and with positions set in Python, and the `ord` suite measures the pipeline of the case study's language beside fpeg's ORD grammar and its transformer, on a file of plain Python and over the application's 76 ORD files. Python suites run under [pyperf](https://pyperf.readthedocs.io), which measures in several worker processes, and the Rust suite under [criterion](https://docs.rs/criterion).

```sh
uv run --group bench python benchmarks/run.py
```

This prepares the inputs, runs every suite and writes a summary and the raw results to `benchmarks/.cache/last-run`. The first run downloads one input from GitHub. Every input is checked against a pinned hash, and one that is missing or damaged is made again.

| Option | Effect |
|---|---|
| `--suite json` | run one suite: `json`, `binding`, `calculator`, `python`, `ord` or `rust`. Repeat it for several |
| `--mode fast` | fewer pyperf worker processes and values, and criterion's quick mode, for a rough answer in less time |
| `--mode rigorous` | twice the pyperf worker processes. The Rust suite measures as in the default mode |
| `--mode smoke` | one value per Python benchmark and criterion's quick mode, to check that every suite runs |
| `--pgo` | run the Rust suite a second time, built with a profile that a short run of its passes suite trained, as the suite `rust-pgo`. It needs rustup's component `llvm-tools` |
| `--record` | move the finished run to `benchmarks/results/<date>-<platform>` to commit it, with a letter after the date for each later run of the same day, and draw the chart of the README from it. It needs every suite, `--pgo` and the default or rigorous mode |

The chart, `docs/_static/benchmarks.svg`, shows three of the suites, and `uv run python benchmarks/chart.py benchmarks/results/<run>` draws it from another record. A test holds the chart to the record it names, so that it cannot drift from its data.

A full run takes several minutes. Close other programs while it runs, and record only on a quiet machine. `benchmarks/README.md` describes the files a run writes.

## Releases

A release starts from a tag that the maintainer pushes, and `.github/workflows/release.yml` does the rest. Nothing is released from a push to `main`.

```{diagram} release.svg
A tag starts the workflow, which builds and tests every package. A pre-release ends on TestPyPI. A release waits for the maintainer's approval, publishes to crates.io and PyPI without a token, and then makes the GitHub release and deploys the docs.
```

1. A release commit sets the version in `Cargo.toml`, in `[workspace.package]` and in the entry of `fpeg` under `[workspace.dependencies]`. Every crate and the Python package take it from there. For a release, the section `Unreleased` of `CHANGELOG.md` becomes `## X.Y.Z (YYYY-MM-DD)`.
2. The tag is the version with a `v` before it, as `v0.1.0`. A pre-release, as `v0.2.0-rc.1` with the version `0.2.0-rc.1`, rehearses a release.
3. The workflow checks the tag against the version and the changelog, builds the wheels for every platform and interpreter and tests each one installed, builds and tests the source distribution, packages and builds the crates, builds the command line for every platform and checks the metadata of the packages.
4. A rehearsal then publishes to TestPyPI and stops. A release waits for the maintainer's approval in the environment `release`, publishes the crates to crates.io, the core first, and the wheels and the source distribution to PyPI, both by trusted publishing, so that no token lives in CI. Then it makes the GitHub release with the section of the changelog, the packages and the binaries, and deploys this documentation to GitHub Pages.

A version is never replaced. A publishing job that failed, as on a network error, runs again and skips what is out already, and a fault in a release is fixed by the next patch version.

The wheels are built with profile-guided optimization, as this builds one:

```sh
uv run maturin build --release --pgo
```

maturin builds the module with counters, installs it in a fresh environment, runs `scripts/pgo_training.py` there, which parses the samples of every grammar and turns larger texts into Python values, and builds the module again with the counts. It needs rustup's component `llvm-tools`, and builds and trains once for each interpreter. `scripts/test_wheels.py` then installs each wheel with its interpreter and runs the Python tests against it.

The maintainer sets up once what the workflow publishes with:

- On PyPI, in the project `fpeg` under Publishing, a trusted publisher for GitHub: the owner `schwimmbeck`, the repository `fpeg`, the workflow `release.yml` and the environment `release`.
- On TestPyPI, the same with the environment `testpypi`, as a pending publisher while the project does not exist there.
- On crates.io, for each of the crates `fpeg` and `fpeg-cli` under Settings, Trusted Publishing: the repository `schwimmbeck/fpeg`, the workflow `release.yml` and the environment `release`.
- In the settings of the repository on GitHub: the environment `release`, with the maintainer as required reviewer and deployments from tags `v*` alone, and the environment `testpypi`, from tags `v*` too. Under Pages, GitHub Actions as the source, and in the environment `github-pages` that this makes, tags `v*` among the allowed deployments. A required reviewer needs the repository to be public on GitHub's free plan.

## Layout

```text
crates/fpeg/          core: grammar front end, analysis, compiler, parsing machine, tree
  grammars/           grammars shipped with the library and the acceptance grammars, samples in samples/
crates/fpeg-cli/      the fpeg command
crates/fpeg-py/       the native module of the Python package, built with maturin
python/fpeg/          the Python package and its type stubs
tests/                Python tests and tests that span crates
benchmarks/           benchmark harness, corpus fetchers, recorded results
benchmarks/rust/      the Rust benchmarks, a package outside the workspace
docs/                 this documentation
fuzz/                 fuzz targets for cargo-fuzz, a package outside the workspace
scripts/              development helpers: the checks, the licence check, the release's helpers
.github/workflows/    CI, and the release from a tag
```
