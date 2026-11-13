# fastpeg

fastpeg is a general-purpose parsing library for Parsing Expression Grammars (PEG).

- A grammar is plain text, loaded at runtime. No code generation and no build step.
- A native engine written in Rust compiles the grammar to bytecode for a parsing machine.
- The parse produces a compact, lossless tree with exact source positions.
- From Python you read that tree or turn it into your own objects with a `Transformer`, a class with one method per rule.

```{note}
fastpeg is at an early stage and does not parse anything yet. The [roadmap](roadmap.md) says what comes next.
```

## Install

fastpeg is not on PyPI yet. To build it from a checkout, follow [Development](development.md). Then check that the native module loads:

```python
import fastpeg

print(fastpeg.__version__)
```

## Reference

- [Grammar language](reference/grammar.md): the syntax of grammars.
- [Semantics](reference/semantics.md): what a grammar means.
- [Diagnostics](reference/diagnostics/index.md): every problem a grammar can have, with its code.

```{toctree}
:hidden:

roadmap
reference/grammar
reference/semantics
reference/diagnostics/index
development
```
