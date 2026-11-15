# Roadmap

fpeg is built in milestones. Each one ends with code that works, tests, benchmarks and documentation. None of them is a throwaway prototype.

M0 to M9 are done. Work continues with M10.

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

- Optimization passes, each with a switch and a benchmark that shows its effect: runs of a class in one step, one skip for the alternatives of a choice at a position, a look at the next byte before an alternative, small rules inside their callers, and left recursion as a loop.
- A default of what a parse remembers, the results that a choice can try twice, and a restart with every rule remembered for a parse that repeats work all the same.
- Guards that bound the time and the memory a parse can take on any input, the memory also from Python with `max_memory`.
- A page of benchmarks made from the recorded runs, with the Rust suite also built with profile-guided optimization.

## M6 Context-sensitive syntax

- Indentation by columns, captures and back references, run by the parsing machine, with what a parse remembers kept apart by them.
- Lexical rules with structure, for strings that hold expressions.
- The Python grammar checked against CPython's own parser over its standard library, tree and positions, Lua's own test suite as a corpus that must parse, and a small language with the offside rule.
- Positions stamped natively on the values a transform makes, as Python's `ast` counts them or in the units of other tools.
- Guides to indentation and to interpolated strings.

## M7 Modules

- `%extends` with `...` for the inherited alternatives, and `%import` of single rules, with what they need under private names.
- The grammars `common`, `json` and `python` as bases for your own, and modules of your own given by name or read from the files beside a grammar.
- The Python grammar's transformer to `ast` in the package as `fpeg.python`, to extend together with the grammar.
- Two grammar extensions as acceptance tests: JSON with comments, and the first real application over the Python grammar, which gives its own parser's `ast` on all its 76 files, with its benchmark from source text to `ast`.
- A guide to composing grammars and a case study of a language over Python.

## M8 Errors

- Messages written in the grammar: labels that name an error and say what was expected where it stands, labels with messages, and error alternatives that give common mistakes a message of their own.
- Recovery that still returns a tree for broken input, with error and missing items, from `%recover` in the grammar, in Python and on the command line, and recoveries in the JSON and Python grammars, measured on texts with single edits.
- Errors shown with the line of the text they stand in, in Python and on the command line, and grammar files read with recovery, so that a check reports each syntax error.
- A guide to errors and recovery.

## M9 Known limits, revisited

- Every known limit of the engine, the Python binding, the checks and the bundled grammars, collected since M1 and decided one by one before the first release: fixed where a fix paid, otherwise kept and stated on a page of known limits.
- Fixed: a default depth that takes the deepest nesting CPython takes, `MemoryError` for a text too large for memory, a display name for every terminal that an error of a bundled or example grammar can name, messages a grammar wrote shown so that a terminal acts on none of their characters, a profile whose advice counts what repeated calls spend, and the text of a parse error listing at most ten errors.
- A fuzz target that lets the fuzzer's bytes choose the grammars it tests.

## M10 The first release

- Wheels for all platforms, built with profile-guided optimization, and version 0.1.0 on PyPI and crates.io, released when it is ready rather than from every change.
- Documentation organized for users, with simple examples and a short tutorial, and a benchmark chart in the README.
- This roadmap gives way to a page of the features planned for later releases, editor support first, and no page refers to milestones any more.

## M11 Editor support

- A language server for grammar files, a formatter and editor highlighting, released as 0.2.0.
- A converter for grammars written for other parsing tools.

## Later

A compiled backend, incremental parsing, a WebAssembly binding and regular expression terminals. Imports under a prefix, `%import m as p`, which need a syntax for prefixed names, and `...` in a precedence block, which needs a rule for where inherited alternatives go among levels. Recovery for grammars that declare none, from labels found where a failure can only be a mistake, with `@sync` marking where it may resume. A way to build Python objects natively, without a Python call for each node.

## Not planned

- User code during parsing. Semantic actions and predicates stay out of grammars, and your code runs over the finished tree.
- Generalized context-free parsing such as GLR or Earley.
- An ahead-of-time parser generator. The workflow stays "load a grammar string".
