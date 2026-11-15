# Changelog

What each release changes for the people who use fpeg. Until version 1.0, a minor release may change the grammar language and the APIs, and its section here says what changed.

## Unreleased

The first release: the library for Rust and Python, the `fpeg` command and the documentation.

- **Grammars** are plain text, read at runtime: rules and terminals written much like EBNF, named fields, operator precedence blocks, parameterized rules, keywords, flags, cuts and left recursion.
- **Layout and context**: blocks by indentation, and captures with back references.
- **Modules**: `%extends` builds a grammar on another, with `...` for the alternatives it inherits, and `%import` takes single rules from others. The grammars `common`, `json` and `python` come with fpeg as bases for your own.
- **Errors** say where a parse failed and what was expected there. Labels and messages written in the grammar name a mistake, error alternatives give common mistakes a message of their own, and `%recover` lets a parse go on past mistakes and return a tree with error and missing items.
- **`!.` is the end of the input.** Where text is left, an error lists the end of the input among what was expected, inside terminals too. In a syntactic rule it skips ignored text first, and an error raised while skipping ends the parse.
- **The engine** compiles a grammar to bytecode for a parsing machine, which builds a compact, lossless tree with exact spans. An optimizer leaves out work that a grammar seems to ask for, and limits on steps, depth and memory bound every parse.
- **Python**: `Grammar`, views of the tree that make Python objects only for what you read, the `Transformer`, the `Interpreter`, the `Visitor` and value builders written in the grammar, errors shown with the line they stand in, and `fpeg.python`, which turns Python 3.15 source into the trees of `ast`.
- **The command line**: `fpeg check` reports problems in a grammar with stable codes, `fpeg parse` parses inputs, and `fpeg profile` shows where a parse spends its work.
