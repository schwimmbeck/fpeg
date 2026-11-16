<h1>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://github.com/schwimmbeck/fpeg/raw/main/docs/_static/logo-dark.svg">
    <img alt="fpeg" src="https://github.com/schwimmbeck/fpeg/raw/main/docs/_static/logo-light.svg" width="300">
  </picture>
</h1>

[![CI](https://github.com/schwimmbeck/fpeg/actions/workflows/ci.yml/badge.svg)](https://github.com/schwimmbeck/fpeg/actions/workflows/ci.yml)
[![Coverage](https://codecov.io/gh/schwimmbeck/fpeg/graph/badge.svg)](https://codecov.io/gh/schwimmbeck/fpeg)
[![Docs](https://img.shields.io/website?url=https%3A%2F%2Fschwimmbeck.github.io%2Ffpeg%2F&label=docs&up_message=online&down_message=offline)](https://schwimmbeck.github.io/fpeg/)
[![Licence: MIT OR Apache-2.0](https://img.shields.io/badge/licence-MIT%20OR%20Apache--2.0-blue)](#licence)
[![REUSE status](https://api.reuse.software/badge/github.com/schwimmbeck/fpeg)](https://api.reuse.software/info/github.com/schwimmbeck/fpeg)
[![Python 3.11+](https://img.shields.io/badge/python-3.11%2B-blue)](https://github.com/schwimmbeck/fpeg/blob/main/pyproject.toml)
[![Rust 1.98+](https://img.shields.io/badge/rust-1.98%2B-blue)](https://github.com/schwimmbeck/fpeg/blob/main/Cargo.toml)

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

## Benchmarks

![fpeg and other parsers, time per input](https://github.com/schwimmbeck/fpeg/raw/main/docs/_static/benchmarks.svg)

The [benchmarks page](https://schwimmbeck.github.io/fpeg/explanation/benchmarks.html) has the numbers of the newest recorded runs, and the `benchmarks` folder the scripts that take them.

## Documentation

- [Getting started](https://schwimmbeck.github.io/fpeg/start.html): install fpeg, write a first grammar and turn its tree into values, in five minutes.
- [Examples](https://schwimmbeck.github.io/fpeg/index.html#examples): a calculator, a configuration file, CSV, JSON and a filter language, each a grammar and a `Transformer`.
- [Tutorials](https://schwimmbeck.github.io/fpeg/index.html#tutorials) and [guides](https://schwimmbeck.github.io/fpeg/index.html#guides) on operator precedence, syntax trees, indentation, modules, errors and performance.
- The [reference](https://schwimmbeck.github.io/fpeg/index.html#reference) of the grammar language, the Python API and the command line.

## What comes next

Editor support is planned for the next release: a language server, a formatter and highlighting for grammar files. The page of [planned features](https://schwimmbeck.github.io/fpeg/planned.html) lists what comes after.

## Licence

Licensed under either of the [MIT licence](https://github.com/schwimmbeck/fpeg/blob/main/LICENSE-MIT) or the [Apache License, Version 2.0](https://github.com/schwimmbeck/fpeg/blob/main/LICENSE-APACHE), at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in fpeg by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
