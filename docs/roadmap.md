# Roadmap

fpeg is built in milestones. Each one ends with code that works, tests, benchmarks and documentation. None of them is a throwaway prototype.

M0 to M4 are done. Work continues with M5.

## M0 Foundations

The Rust workspace with the core, the command line and the Python binding as empty shells, the checks that every change has to pass, the benchmark harness with a baseline of other parsers, and this site.

## M1 Language specification and reference interpreter

- The grammar language and its meaning written down as the normative reference.
- A set of acceptance grammars written in that language: JSON, CSV, expressions, a configuration language, an indentation-based language, Lua, Python, a SQL subset and the grammar language itself.
- A slow reference interpreter that implements exactly the written semantics. It is test infrastructure: every later engine and optimization has to agree with it.

## M2 Core engine

- The core language compiled to bytecode for the parsing machine: the classic PEG operators, separators, rules and terminals with automatic skipping of ignored text, and the shaping of the tree.
- The compact, lossless parse tree with exact spans.
- Limits on time and depth, and error reports with position and expected input.
- `fpeg parse` and a first `fpeg check`, with a stable code for every problem a grammar can have.
- Left recursion, cuts, indentation and captures wait for the milestones that bring them to the engine.

## M3 Python binding

- `Tree` and `Token` views, the `Transformer`, value builders written in the grammar, `ParseError` with the place of a failure, and type stubs.
- JSON and CSV from grammar text to Python objects, checked against the `json` and `csv` modules.
- The first tutorials and the reference of the Python API.

## M4 The language, complete for context-free use

- Operator precedence blocks, parameterized rules, keyword handling, flags, cut, left recursion and memoization, all run by the parsing machine.
- Warnings of `fpeg check` and `fpeg.check` about grammars that likely do not do what they seem to, each with its page, and `fpeg profile`, which shows where a parse spends its work.
- `Interpreter` and `Visitor` in Python.
- The grammar language parsed by its own grammar.
- Expressions checked against their generator, a calculator against `eval`, TOML against `tomllib`, and a tiny language run and compiled to LLVM IR.
- Guides to operator precedence, building an AST and performance.

## M5 Optimizer

- Optimization passes, each with a switch and a benchmark that shows its effect.
- Guards that bound the time a parse can take on any input.

## M6 Context-sensitive syntax

- Indentation, captures and back references, lexical rules with structure.
- A Python grammar checked against CPython's own parser over the standard library.

## M7 Modules

- `%import` and `%extends`, with the bundled grammars as a base for your own.
- A large grammar extension as acceptance test, and a converter for grammars written for other parsing tools.

## M8 Errors, complete

- Error messages and labels written in the grammar.
- Recovery that still returns a tree, with error and missing nodes, for broken input.
- Rendering of errors with source snippets.

## M9 Tooling and the first release

- A language server for grammar files, a formatter and editor highlighting.
- Wheels for all platforms and version 0.1.0.

## Later

A compiled backend, incremental parsing, a WebAssembly binding and regular expression terminals.

## Not planned

- User code during parsing. Semantic actions and predicates stay out of grammars, and your code runs over the finished tree.
- Generalized context-free parsing such as GLR or Earley.
- An ahead-of-time parser generator. The workflow stays "load a grammar string".
