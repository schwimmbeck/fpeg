<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/_static/logo-dark.svg">
    <img alt="fpeg" src="docs/_static/logo-light.svg" width="300">
  </picture>
</h1>

[![CI](https://github.com/schwimmbeck/fpeg/actions/workflows/ci.yml/badge.svg)](https://github.com/schwimmbeck/fpeg/actions/workflows/ci.yml)
[![Licence: MIT OR Apache-2.0](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue)](#license)
[![Python 3.11+](https://img.shields.io/badge/python-3.11%2B-blue)](pyproject.toml)
[![Rust 1.98+](https://img.shields.io/badge/rust-1.98%2B-blue)](Cargo.toml)

A general-purpose PEG parsing library with a native engine and a `Transformer` that turns parse trees into your own objects.

> fpeg is at an early stage and not on PyPI yet. Parts of what this page describes are still being built, see the status below.

## The idea

- Write a grammar as plain text and load it at runtime. No code generation and no build step.
- Parse with a native engine written in Rust.
- Get a parse tree with exact source positions, or turn it into your own objects with a `Transformer`, a class with one method per rule.

JSON, from text to Python objects:

```python
import fpeg

grammar = fpeg.Grammar(r"""
    ?start: value
    ?value: object | array | STRING | NUMBER
          | "true" => True | "false" => False | "null" => None
    object: "{" (member ** ",") "}"
    member: key=STRING ":" value=value
    array: "[" (value ** ",") "]"
    STRING: "\"" ([^"\\] | "\\" .)* "\"" => unquote
    NUMBER: "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)? => float
    %ignore [ \t\r\n]+
""")


class ToPython(fpeg.Transformer):
    def object(self, *members):
        return dict(members)

    def member(self, key, value):
        return key, value

    def array(self, *values):
        return list(values)


tree = grammar.parse('{"name": "fpeg", "tags": ["peg", "rust"], "stable": false}')
print(ToPython().transform(tree))
```

```output
{'name': 'fpeg', 'tags': ['peg', 'rust'], 'stable': False}
```

Each method is named after a rule and gets what its signature asks for: `*members` the children of the node, `key` and `value` its fields. `=> unquote`, `=> float` and the constants are value builders, which make the values of tokens and nodes in the grammar itself, without a Python call.

## Install

fpeg is not on PyPI yet. To build it from a checkout, follow [Development](docs/development.md).

## Documentation

The [documentation](docs/index.md) has two tutorials, one that writes a first grammar and one that builds your own objects with a `Transformer`, and the reference of the grammar language and the Python API.

## What is planned

- **Parsing Expression Grammars**: ordered choice, lookahead and one parse tree for every input, never an ambiguity to resolve.
- **Grammars at runtime**: a grammar is a string or a file. Loading one takes milliseconds.
- **A familiar notation**: rules written much like EBNF, with named fields, operator precedence, grammar modules and indentation-sensitive syntax built in.
- **A native core**: the grammar is compiled to bytecode for a parsing machine. The tree stays in native memory until you ask for a part of it.
- **Transformers and visitors** in Python, called once per node after the parse.
- **Errors that help**: what was expected and where, custom messages in the grammar, and a tree even for broken input.
- **Tooling**: a grammar checker with stable diagnostic codes, a profiler, and later a language server.

The goal for speed is to be the fastest PEG parser that can be used from Python with a grammar loaded at runtime. Benchmarks will be published with the first release, together with the scripts that produce them.

## Status

Early development. The written specification of the grammar language is done, with a reference interpreter that defines what every grammar means, and so are the first versions of the core engine and the Python binding: grammars without left recursion, cuts, indentation and captures parse from Python, Rust and the command line, and a `Transformer` or the builders of a grammar turn the tree into objects. Next comes the rest of the language, with operator precedence, left recursion and cuts. See the [roadmap](docs/roadmap.md).

## License

Licensed under either of the [MIT licence](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in fpeg by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
