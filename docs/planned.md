# Planned features

What later releases of fpeg are planned to bring, the next release first. Each release moves what it ships from this page to the [changelog](https://github.com/schwimmbeck/fpeg/blob/main/CHANGELOG.md).

## Editor support, for 0.2.0

- **A language server** for grammar files: their diagnostics while you type, the definition of a rule from a reference to it, and completion of names.
- **A formatter** for grammar files that keeps their comments.
- **Highlighting** of grammar files, and an extension for VS Code.

## Later

- **A converter** for grammars written for other parsing tools, with notes on porting them.
- **Python objects built natively**, without a Python call for each node.
- **Recovery for grammars that declare none**, from labels found where a failure can only be a mistake, with `@sync` marking where a parse may resume.
- **Imports under a prefix**, `%import m as p`, which need a syntax for prefixed names, and **`...` in a precedence block**, which needs a rule for where inherited alternatives go among levels. Both are part of the language already, and a grammar that uses them gets [E501](reference/diagnostics/E501.md).
- **Regular expressions as terminals.**
- **A compiled backend**, which turns a grammar into machine code in place of the program of the parsing machine.
- **Incremental parsing**, which parses again only what an edit changed.
- **A WebAssembly binding**, for fpeg in browsers and JavaScript.

## Not planned

- User code during parsing. Semantic actions and predicates stay out of grammars, and your code runs over the finished tree.
- Generalized context-free parsing, such as GLR or Earley.
- An ahead-of-time parser generator. A grammar stays a string that a program loads.
