# fastpeg

fastpeg is a general-purpose parsing library for Parsing Expression Grammars (PEG).

- A grammar is plain text, loaded at runtime. No code generation and no build step.
- A native engine written in Rust compiles the grammar to bytecode for a parsing machine.
- The parse produces a compact, lossless tree with exact source positions.
- From Python you read that tree or turn it into your own objects with a `Transformer`, a class with one method per rule.

```{note}
fastpeg is at its first milestone. The build, the checks and the benchmark harness are in place, the parser is not. The [roadmap](roadmap.md) says what comes next.
```

## Install

fastpeg is not on PyPI yet. To build it from a checkout, follow [Development](development.md). Then check that the native module loads:

```python
import fastpeg

print(fastpeg.__version__)
```

```{toctree}
:hidden:

roadmap
development
```
