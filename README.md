# fastpeg

A general-purpose PEG parsing library with a native engine and a Lark-style `Transformer`.

> fastpeg is in the planning stage. Nothing can be installed yet. This page describes what is being built.

## The idea

- Write a grammar as plain text and load it at runtime. No code generation and no build step.
- Parse with a native engine written in Rust.
- Get a parse tree with exact source positions, or turn it into your own objects with a `Transformer`, a class with one method per rule.

A sketch of where this is going. The grammar syntax and the API will still change:

```python
import operator
import fastpeg

grammar = fastpeg.Grammar(r"""
    ?start: expr

    expr: precedence {
        | left=(@) op=("+" | "-") right=@   -> binary
        ---
        | left=(@) op=("*" | "/") right=@   -> binary
        ---
        | NUMBER
        | "(" expr ")"
    }

    NUMBER: [0-9]+ ("." [0-9]+)?
    %ignore [ \t]+
""")

OPERATORS = {"+": operator.add, "-": operator.sub, "*": operator.mul, "/": operator.truediv}


class Calculate(fastpeg.Transformer):
    def binary(self, left, op, right):
        return OPERATORS[op](left, right)

    def NUMBER(self, token):
        return float(token)


print(Calculate().transform(grammar.parse("1 + 2 * 3")))  # 7.0
```

## What is planned

- **Parsing Expression Grammars**: ordered choice, lookahead and one parse tree for every input, never an ambiguity to resolve.
- **Grammars at runtime**: a grammar is a string or a file. Loading one takes milliseconds.
- **A familiar notation**: close to Lark's, with named fields, operator precedence, grammar modules and indentation-sensitive syntax built in.
- **A native core**: the grammar is compiled to bytecode for a parsing machine. The tree stays in native memory until you ask for a part of it.
- **Transformers and visitors** in Python, called once per node after the parse.
- **Errors that help**: what was expected and where, custom messages in the grammar, and a tree even for broken input.
- **Tooling**: a grammar checker with stable diagnostic codes, a profiler, and later a language server.

The goal for speed is to be the fastest PEG parser that can be used from Python with a grammar loaded at runtime. Benchmarks will be published with the first release, together with the scripts that produce them.

## Status

Design and planning. The first milestones build the core engine, the Python binding and the documentation side by side.

## License

MIT. See [LICENSE](LICENSE).
