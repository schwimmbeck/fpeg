# Related projects

What other parsing tools are for, where fpeg differs, and what moving a grammar or a transformer from Lark to fpeg takes. The [benchmarks](benchmarks.md) measure several of these tools beside fpeg on the same inputs.

## For Python

- **[Lark](https://github.com/lark-parser/lark)** reads a grammar in EBNF at runtime and parses with an Earley parser, which takes any context-free grammar, ambiguous ones included, or with an LALR(1) parser for grammars that fit it. Both are written in Python. For a grammar that is ambiguous by nature, or where every reading of an input is wanted, Earley parsing is the tool, and a parsing expression grammar is not: it gives every input at most one tree. fpeg's notation and its `Transformer` follow Lark's, as the section below shows.
- **[pe](https://github.com/goodmami/pe)** reads parsing expression grammars at runtime too, and parses with a packrat parser or a parsing machine.
- **[tree-sitter](https://tree-sitter.github.io/tree-sitter/)** generates a parser in C from a grammar written in JavaScript, ahead of time, and parses again only what an edit changed. Editors use it to highlight and fold code as it is typed, and its Python binding loads the generated parsers. fpeg loads a grammar from text at runtime instead, and does not parse incrementally yet.
- **[LibCST](https://github.com/Instagram/LibCST)** parses Python source, in native code, into a concrete syntax tree that keeps every space and comment, so that a program can change source code and write it back. It reads Python only, where fpeg reads any language its grammar describes, Python among them.
- **Python's own parser**, through the `ast` module, is the reference for Python source. fpeg's Python grammar gives the same `ast`, which the tests check, for languages that add to Python.

## For Rust

- **[pest](https://pest.rs)** generates a parser for a parsing expression grammar at build time, from a grammar file that a derive macro reads, and its crate `pest_vm` runs a grammar given at runtime. Its rules return pairs of tokens that the program walks.
- **[serde_json](https://github.com/serde-rs/json)** is a JSON parser for Rust written by hand for JSON alone. The benchmarks show it beside fpeg's JSON grammar as the speed that a parser built for one format reaches.

## Coming from Lark

A grammar for Lark reads much the same in fpeg:

| Lark | fpeg |
|---|---|
| `?rule`, `_rule`, `-> alias`, `_TERMINAL` | the same |
| `"select"i` | the same |
| `[item]`, `item?` | `item?` |
| `item ("," item)*` | the same, or `(item ++ ",")` |
| `/[a-z]+/`, a regular expression | `[a-z]+`, classes and repetition, since terminals are written in the grammar's own notation |
| `%import common.WS` | `%import common (WS)` |
| `%ignore WS` | the same |
| `_separated{x, sep}: x (sep x)*` | `separated<x, sep>: x (sep x)*` |
| an `Indenter` after the lexer | `%indent` and `%same` in the grammar, as the [indentation guide](../guide/indentation.md) shows |
| `TERMINAL.2`, priorities | the order of alternatives, since a choice takes the first that matches |

The main difference is the choice: Lark's `|` lets the parser find the alternative that fits, and fpeg's tries the alternatives in order and takes the first that matches, so a longer alternative goes before a shorter one that starts it. `fpeg check` warns about an alternative that an earlier one hides.

Code written for Lark's `Transformer` mostly runs: a method named after a rule gets the list of the children, and `fpeg.v_args` reads Lark's decorator. `Tree(data, children, meta)`, `pretty()` and the token attributes `type`, `start_pos`, `end_pos`, `line`, `column` and `end_column` follow Lark's.

```python
import fpeg
from fpeg import Transformer, v_args

grammar = fpeg.Grammar(r"""
start: pair ("," pair)*
pair: NAME "=" NUMBER
NAME: [a-z]+
NUMBER: [0-9]+
%ignore " "+
""")


class ToDict(Transformer):
    def start(self, children):
        return dict(children)

    @v_args(inline=True)
    def pair(self, name, number):
        return str(name), int(number)


print(ToDict().transform(grammar.parse("x = 1, y = 22")))
```

```output
{'x': 1, 'y': 22}
```

Two things differ on purpose. Tokens compare as plain strings, without Lark's comparison of their types, and an exception in a method keeps its type instead of becoming Lark's `VisitError`, with a note that says which method raised it. fpeg's grammars also name fields, which Lark's do not, and a method that names fields as its parameters gets them by name. The [Python API](../reference/python-api.md#transformer) lists every form a method can take.
