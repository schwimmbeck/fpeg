# fpeg

fpeg is a general-purpose parsing library for Parsing Expression Grammars (PEG).

- A grammar is plain text, loaded at runtime. No code generation and no build step.
- A native engine written in Rust compiles the grammar to bytecode for a parsing machine.
- The parse produces a compact, lossless tree with exact source positions.
- From Python you read that tree or turn it into your own objects with a `Transformer`, a class with one method per rule.

```{note}
fpeg is at an early stage. The [command line](reference/cli.md) and the [Python package](reference/python-api.md) parse with every grammar but those that use a form of a later milestone ([E501](reference/diagnostics/E501.md)), and warn about grammars that likely do not do what they seem to. The Python package turns trees into Python values through a `Transformer`, an `Interpreter` or the value builders written in the grammar. The [roadmap](roadmap.md) says what comes next.
```

## Install

fpeg is not on PyPI yet. To build it from a checkout, follow [Development](development.md). Then check that the native module loads:

```python
import fpeg

print(fpeg.__version__)
```

## A first parse

A grammar is a string, and `parse` returns the tree of an input:

```python
grammar = fpeg.Grammar(r"""
start: (pair ** ",")
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

Lowercase names are rules and uppercase names terminals, `**` separates repetitions, `key=` and `value=` name the parts of a pair, and `%ignore` skips the spaces. The tutorials go on from here.

## Tutorials

- [Your first grammar](tutorial/first-grammar.md): JSON from nothing to Python objects.
- [From a tree to your own objects](tutorial/transformer.md): a `Transformer` that builds a syntax tree and computes values.

## Guides

- [Operator precedence](guide/operator-precedence.md): expressions with a precedence block, and a calculator in Python's order of operators.
- [Building an AST](guide/building-an-ast.md): your own classes from a parse, by methods or by builders in the grammar, and running the result.
- [Indentation](guide/indentation.md): blocks by columns, brackets that turn layout off, continuation lines and tabs.
- [Interpolated strings](guide/interpolated-strings.md): expressions inside strings, quotes as parameters, and delimiters that the input chooses.
- [Composing grammars](guide/composing-grammars.md): a grammar that extends another, rules taken from others, modules of your own, and Python extended with its transformer.
- [A language over Python](guide/a-language-over-python.md): ORD, a grammar over Python with its transformer to `ast`, checked against the language's own parser.
- [Performance](guide/performance.md): seeing where a parse spends its work, and what to change in a grammar.

## Reference

- [Grammar language](reference/grammar.md): the syntax of grammars.
- [Semantics](reference/semantics.md): what a grammar means.
- [Parse tree](reference/tree.md): nodes, tokens, fields and spans.
- [Python API](reference/python-api.md): `Grammar`, `Tree`, `Token`, `Transformer`, `Interpreter`, `Visitor` and the errors.
- [Command line](reference/cli.md): `fpeg check`, `fpeg parse` and `fpeg profile`.
- [Diagnostics](reference/diagnostics/index.md): every problem a grammar can have, with its code.
- [Benchmarks](benchmarks.md): what fpeg and other parsers take on the same inputs.

```{toctree}
:hidden:

roadmap
tutorial/first-grammar
tutorial/transformer
guide/operator-precedence
guide/building-an-ast
guide/indentation
guide/interpolated-strings
guide/composing-grammars
guide/a-language-over-python
guide/performance
reference/grammar
reference/semantics
reference/tree
reference/python-api
reference/cli
reference/diagnostics/index
benchmarks
development
```
