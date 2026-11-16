<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/schwimmbeck/fpeg/raw/main/docs/_static/logo-dark.svg">
    <img alt="fpeg" src="https://github.com/schwimmbeck/fpeg/raw/main/docs/_static/logo-light.svg" width="300">
  </picture>
</h1>

[![CI](https://github.com/schwimmbeck/fpeg/actions/workflows/ci.yml/badge.svg)](https://github.com/schwimmbeck/fpeg/actions/workflows/ci.yml)
[![Coverage](https://codecov.io/gh/schwimmbeck/fpeg/graph/badge.svg)](https://codecov.io/gh/schwimmbeck/fpeg)
[![Licence: MIT OR Apache-2.0](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue)](#licence)
[![REUSE status](https://api.reuse.software/badge/github.com/schwimmbeck/fpeg)](https://api.reuse.software/info/github.com/schwimmbeck/fpeg)
[![Python 3.11+](https://img.shields.io/badge/python-3.11%2B-blue)](pyproject.toml)
[![Rust 1.98+](https://img.shields.io/badge/rust-1.98%2B-blue)](Cargo.toml)

A parsing library for Parsing Expression Grammars (PEG), with a native engine written in Rust and a `Transformer` that turns parse trees into your own objects.

- A grammar is plain text, loaded at runtime. No code generation and no build step.
- The engine compiles a grammar to bytecode for a parsing machine and builds a compact, lossless tree with exact source positions.
- Errors say where a parse failed and what was expected there, a grammar can name its mistakes and go on past them, and limits bound the time and memory of every parse.

```python
import fpeg

grammar = fpeg.Grammar(r"""
start: pair ** ","
pair: key=NAME "=" value=NUMBER
NAME: [a-z]+
NUMBER: [0-9]+
%ignore " "+
""")

tree = grammar.parse("x = 1, y = 22")
print(tree.pretty())
```

```output
start
  pair
    key=x
    value=1
  pair
    key=y
    value=22
```

A `Transformer` turns the tree into values, with one method for each rule:

```python
class ToDict(fpeg.Transformer):
    def start(self, *pairs):
        return dict(pairs)

    def pair(self, key, value):
        return str(key), int(value)


print(ToDict().transform(tree))
```

```output
{'x': 1, 'y': 22}
```

## Install

For Python 3.11 or newer:

```sh
pip install fpeg
```

For Rust 1.98 or newer, the library is the crate [`fpeg`](https://docs.rs/fpeg):

```sh
cargo add fpeg
```

The `fpeg` command checks grammars, parses inputs and profiles a parse. Each [release](https://github.com/schwimmbeck/fpeg/releases) comes with it built for Linux, macOS and Windows, and Cargo builds it from source:

```sh
cargo install fpeg-cli
```

> fpeg is not released yet. Until its first release, build it from a checkout as [Contributing](docs/contributing.md) describes.

## Benchmarks

![fpeg and other parsers, time per input](https://github.com/schwimmbeck/fpeg/raw/main/docs/_static/benchmarks.svg)

The [benchmarks page](docs/explanation/benchmarks.md) has the numbers of the newest recorded runs, and the `benchmarks` folder the scripts that take them.

## Documentation

- [Getting started](docs/start.md): install fpeg, write a first grammar and turn its tree into values, in five minutes.
- [Examples](docs/index.md#examples): a calculator, a configuration file, CSV, JSON and a filter language, each a grammar and a `Transformer`.
- [Tutorials](docs/index.md#tutorials) and [guides](docs/index.md#guides) on operator precedence, syntax trees, indentation, modules, errors and performance.
- The [reference](docs/index.md#reference) of the grammar language, the Python API and the command line.

## What comes next

Editor support is planned for the next release: a language server, a formatter and highlighting for grammar files. The page of [planned features](docs/planned.md) lists what comes after.

## Licence

Licensed under either of the [MIT licence](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in fpeg by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
