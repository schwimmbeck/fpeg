# fpeg

Working guide for people and AI agents in this repository. Update it when the structure or the rules change.

This file is public. Keep it free of personal or private information. The maintainer's plan, the research behind it and the open decisions live in a local, gitignored `PLAN.md`. The benchmark scripts and raw results behind its numbers are under `research/`, local as well. Read `PLAN.md` first when it exists, and never commit either.

`CLAUDE.md` is a symbolic link to this file. Edit `AGENTS.md` only.

## Product

fpeg is a general-purpose parsing library for Parsing Expression Grammars (PEG).

- A grammar is plain text, loaded at runtime. No code generation and no build step for the user.
- The core is Rust. It compiles a grammar to bytecode for a parsing machine and builds a compact, lossless parse tree with exact source spans.
- Python is the first binding. It exposes the tree lazily and runs a `Transformer`, a class with one method per rule, after the parse.
- Grammar analysis reports diagnostics with stable codes. A command-line checker and later a language server use the same diagnostics.

`CHANGELOG.md` says what each release brought, and `docs/planned.md` what later releases are planned to bring.

## Principles

Each one settles an argument that would otherwise come up again and again.

- **General purpose.** No engine code, API or doc is shaped for one grammar or one user. A need that shows up in one grammar becomes a general primitive or a library grammar, or it stays out. The first real application is an example and a test, never a special case.
- **Semantics first.** What a grammar means is written down independently of how it is parsed, and a slow reference interpreter, compiled only for tests and fuzzing, implements exactly that text. Every engine and every optimization has to agree with it.
- **Measure, then optimize.** An optimization lands with a benchmark that shows its effect and a switch that turns it off. One that does not pay is removed.
- **A good number proves nothing until a control shows it can fail.** Run the case that must break a check before trusting the check.
- **No user code runs during parsing.** Parsing stays deterministic and repeatable, which error reporting, memoization and recovery all rely on. User code runs afterwards, over the tree.
- **The tree is the product.** Every consumer (Transformer, visitor, tooling) is a view over one native tree.
- **The Python boundary is designed, not incidental.** Python objects are created only for what the caller asks for, since creating them costs more than parsing.
- **Untrusted input is the normal case.** Grammars and inputs can come from anywhere, so time and stack use are bounded and nothing panics across the language boundary.
- **Think a design through before building it.** A change to a format, an instruction set, a tree shape or a public API starts with a short written proposal that lists everything reading it and how each reader will be checked.

## Layout

```text
crates/fpeg/          core: grammar front end, analysis, compiler, parsing machine, tree. No Python types
  grammars/           grammars shipped with the library and the acceptance grammars, samples in samples/
crates/fpeg-cli/      the fpeg command: check, parse, profile
crates/fpeg-py/       PyO3 binding, built with maturin
python/fpeg/          Python package: Transformer, Interpreter, Visitor, tree and token views, type stubs
benchmarks/           benchmark harness, corpus fetchers, recorded results
benchmarks/rust/      fpeg's Rust benchmarks and baselines, a package outside the workspace
docs/                 user documentation, one Markdown file per page
tests/                Python tests, and oracle and corpus tests that span crates
fuzz/                 fuzz targets for cargo-fuzz, a package outside the workspace
scripts/              development helpers, among them the licence header check
.github/workflows/    CI
```

## Commands

Setup needs rustup and uv. `uv sync` builds the native module and installs the package and the development tools into `.venv`. `uv run` rebuilds the module when a Rust source has changed. Development uses the Python version in `.python-version`.

These checks are the bar, and CI treats warnings as errors. CI runs the Rust and Python tests on Linux with the oldest supported Python and on Windows and macOS with the newest, all other checks and the Python tests with the development version on Linux, and a benchmark smoke run on Linux. Once a week it runs the smoke run on Windows and the corpus tests.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo clippy --workspace --lib --bins -- -D warnings
cargo rustdoc -p fpeg -- -D warnings
cargo test
uv run ruff format --check
uv run ruff check
uv run pyright --warnings
uv run python -m mypy.stubtest fpeg._fpeg
uv run --group oracles pytest
uv run --group docs sphinx-build -W --keep-going -b html docs docs/_build/html
uv run python scripts/check_licence_headers.py
uv run reuse lint
cargo fmt --check --manifest-path benchmarks/rust/Cargo.toml
cargo clippy --manifest-path benchmarks/rust/Cargo.toml --all-targets -- -D warnings
cargo fmt --check --manifest-path fuzz/Cargo.toml
cargo clippy --manifest-path fuzz/Cargo.toml --all-targets -- -D warnings
```

- `uv run python scripts/check.py` runs all of them in this order, stops at the first failure and prints the time each took. Run it before every commit.
- The docs build runs every Python code block in the docs.
- The second clippy builds the crates as users and the Python package get them. The tests turn on the `reference` feature for the whole workspace, so only that build sees code the tests alone use. `cargo rustdoc` builds the core's documentation as docs.rs does, so it fails on a broken link, one to an item of the tests alone among them.
- The licence check holds project files to the project's licence and holder. `reuse lint` checks that the repository follows the REUSE specification: `REUSE.toml` gives copyright and licence to the files that cannot carry a header, and `LICENSES/` holds the text of every licence used.
- The dependency group `oracles` holds what a test compares fpeg with beyond the standard library, today `llvmlite`. A test skips without its oracle, so `uv run pytest` alone works too, and the test jobs of CI run it that way.
- pyright checks `python/`, `tests/` and `scripts/`, and mypy's stubtest holds the stubs of the native module to the module as built. The benchmarks and the docs import their own dependency groups, and ruff lints them.
- Tests over a whole corpus carry the marker `corpus` and stay out of the default run. `uv run python benchmarks/corpus.py` fetches their corpora, then `uv run --group oracles pytest -m corpus` runs them.
- Benchmarks: `uv run --group bench python benchmarks/run.py`. `--mode smoke` checks that every suite runs, `--record` writes a run to commit. `docs/contributing.md` lists the options.

Run the checks for everything you touched before calling work done. The Rust version is pinned in `rust-toolchain.toml`. Bump it on purpose and fix new lints in the same change.

## Code style

- Concise, direct code. No clever abstraction where an explicit form does the job. A trait, a generic or a macro has to remove real duplication or enable a measured gain.
- Formatting is whatever `rustfmt` and `ruff format` produce. Do not align by hand.
- Fix a lint, do not silence it. An `#[allow(...)]`, `# noqa` or `# type: ignore` needs a one-line reason beside it.
- Comments are short and say why, not what. They state constraints the code cannot express, edge cases, and the paper or spec a routine follows. No comment beats one that repeats the code. A comment never says who asked for something or in which round of feedback. Delete commented-out code.
- In comments, docs, error messages and commit messages: no em or en dashes, no semicolons in sentences, no banner comments, and no filler words such as "robust", "seamless", "leverage" or "comprehensive".
- Public items carry doc comments, `///` in Rust and docstrings in Python: one summary line, then details only when they add information. Types belong in the signature.
- No magic numbers. A value that tunes behaviour (limits, sizes, thresholds, defaults) is a named constant with its unit in the name (`_BYTES`, `_MS`, `_STEPS`) and a one-line doc comment. Exempt are 0, 1, plain arithmetic and values fixed by an outside format.
- Reuse the terms the code and the docs already use (rule, terminal, alternative, field, node, token, span) instead of coining new ones. Names are short, precise and professional.
- Model data explicitly. Typed records and enums instead of positional tuples, parallel arrays in the public API or stringly typed flags.
- Errors are values. The core returns `Result` and never panics on any grammar or input. A panic is a bug and gets a test.
- `unsafe` is allowed only where a benchmark shows the gain. Each block carries a `// SAFETY:` comment, sits behind a safe function and is covered by tests that run under Miri where Miri can run them.
- Every source file starts with two SPDX lines in the comment style of its language: <!-- REUSE-IgnoreStart -->`SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck` and `SPDX-License-Identifier: MIT OR Apache-2.0`<!-- REUSE-IgnoreEnd -->. Outside code needs a licence compatible with both MIT and Apache-2.0 and keeps its own header.
- Keep dependencies few and current. A new dependency of the core needs its reason in the commit message, since it lands in every wheel.

## Tests

- Every feature lands with tests, and every fixed bug with a test that fails without the fix.
- Each test states one rule in its name and asserts that rule. Tests build their own data and share no mutable state.
- Do not overtest. Tests are code that has to be carried. Prefer extending an existing test, and prefer a test through the public API over one for an internal helper. Roughly one test per behaviour.
- The oracles carry most of the weight, so reach for them before writing cases by hand:
  - the reference interpreter, compared with the engine on generated grammars and inputs at every optimization setting,
  - CPython's own `ast` for the Python grammar, over a corpus,
  - `json` and other standard parsers wherever a grammar has one.
- Keep the suite fast. Slow, large-corpus and fuzz runs sit behind `#[ignore]` or a marker and run on a schedule. Each fuzz target runs a round on Linux before each release.
- A performance claim is a benchmark in `benchmarks/`, never a number typed into a doc.

## Docs

- `README.md` is for users only and stays short: what fpeg is, a first example, how to install, the benchmark chart, a link to the documentation. No build steps and no internals.
- User documentation lives in `docs/`, one Markdown file per page. A page changes in the same commit as the behaviour it describes, never afterwards.
- Every code example in the docs runs in CI. The docs build runs the Python blocks, and `cargo test` reads every grammar block and checks those with a `start` rule, which must give no warning. Printed output is shown in an `output` block right after its Python block, which the build compares, and a test runs the README's blocks the same way. Shell commands and plain text are not examples. An example that cannot run is a bug.
- Every diagnostic has a stable code, and its page shows an input that triggers it and the fix.
- `CHANGELOG.md` records user-visible changes per release.
- Design notes and decisions belong in `PLAN.md`, or in the explanation pages of `docs/` once they are worth publishing.

## Git

- Commit subjects use `<module or file>: <message>`, for example `vm: add the span instruction` or `AGENTS.md: add commit rules`. The message is lowercase and imperative, without a trailing period, at most 72 characters. A body is optional and says why, not what.
- Split work into logical commits that each build and pass the checks on their own. Commit after each verified chunk of work.
- Keep a commit small enough to review in one sitting: about 400 changed lines or fewer, tests and docs included. A larger feature lands as a series of steps, each one behaviour with its own tests and docs, such as a type before the code that uses it, one calling convention after another, or the core before the binding. Recorded data and moved code may be larger, and the body says so.
- A step leaves no dead code and no stub or page ahead of the code: the checks treat warnings as errors and hold the stubs and the docs to the module. A helper that only the next step uses belongs to that step.
- A commit that grew too large is split, and a change that belongs to an older commit is amended into it, each new commit checked on its own. Pushed commits may be rewritten so too when the maintainer agrees, and go out with `git push --force-with-lease`.
- Never add `Co-Authored-By`, "Generated with", session links or any other attribution or tool line to commits or pull requests.
- Commit messages name no other project, such as another parsing library, also as a reason for a change. Comparisons with related projects belong on one page of the documentation, `docs/explanation/related-projects.md`, and in the measured results of the benchmarks.
- Pushing waits for the maintainer. Ask before every release tag.
- Stage by explicit path and read the staged list before committing. Never commit build output, caches, generated pages, `PLAN.md`, `research/` or anything private. Recorded benchmark results are data and are committed.
- After each commit, append one line to the Progress section of `PLAN.md` when that file exists.

## Review and security

When reviewing, look first for:

- wrong parses and wrong trees, above all a result that differs from the reference interpreter,
- unbounded time, memory or recursion on some grammar or input,
- panics, and undefined behaviour in `unsafe` code or at the Python boundary (reference counts, the lifetime of borrowed input, the GIL),
- a change in the meaning of a grammar that no version note covers.

One independent review covers a group of related commits, not each commit, since a review costs more than the work around it.

## Interaction

- A why-question wants the reasoning: the requirement, the alternatives that were rejected and why they fail. Not a walkthrough of the mechanism.
- "Evaluate before changing" is a gate. A negative verdict ends the turn, without a compromise variant.
- A question about code gets an answer. Do not change the code unless asked.

## Windows checkouts

`CLAUDE.md` is stored as a symbolic link. Git for Windows writes it as a one-line text file unless symbolic links are enabled (Developer Mode, then `git config core.symlinks true`), and Claude Code then reads that stub in place of this file. Either enable links, or keep the stub out of the working tree so that `AGENTS.md` is read directly:

```sh
git config core.sparseCheckout true
printf '/*\n!/CLAUDE.md\n' > .git/info/sparse-checkout
git read-tree -mu HEAD
```
