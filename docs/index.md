# fpeg

fpeg is a general-purpose parsing library for Parsing Expression Grammars (PEG).

- A grammar is plain text, loaded at runtime. No code generation and no build step.
- A native engine written in Rust compiles the grammar to bytecode for a parsing machine.
- The parse produces a compact, lossless tree with exact source positions.
- From Python you read that tree or turn it into your own objects with a `Transformer`, a class with one method per rule.

```{note}
fpeg is at an early stage. The [command line](reference/cli.md) and the [Python package](reference/python-api.md) parse with grammars that need no left recursion, cuts, columns or captures. The `Transformer` and the value builders of the grammar come next, and the [roadmap](roadmap.md) says what comes after.
```

## Install

fpeg is not on PyPI yet. To build it from a checkout, follow [Development](development.md). Then check that the native module loads:

```python
import fpeg

print(fpeg.__version__)
```

## Reference

- [Grammar language](reference/grammar.md): the syntax of grammars.
- [Semantics](reference/semantics.md): what a grammar means.
- [Parse tree](reference/tree.md): nodes, tokens, fields and spans.
- [Python API](reference/python-api.md): `Grammar`, `Tree`, `Token` and the errors.
- [Command line](reference/cli.md): `fpeg check` and `fpeg parse`.
- [Diagnostics](reference/diagnostics/index.md): every problem a grammar can have, with its code.

```{toctree}
:hidden:

roadmap
reference/grammar
reference/semantics
reference/tree
reference/python-api
reference/cli
reference/diagnostics/index
development
```
