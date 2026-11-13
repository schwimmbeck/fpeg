# Python API

The `fpeg` package compiles a grammar from text at runtime.

```python
import fpeg
```

## Grammar

`Grammar(text)` compiles a grammar given as text, and `Grammar.open(path)` reads one from a file. A grammar with problems raises `GrammarError`, which holds each problem as a `Diagnostic` with its [code](diagnostics/index.md) and its place.

```python
try:
    fpeg.Grammar("start: pair")
except fpeg.GrammarError as error:
    (problem,) = error.diagnostics
    assert (problem.code, problem.line, problem.column) == ("E201", 1, 8)
    assert str(problem) == "<grammar>:1:8: error[E201]: unknown rule pair"
```

```{eval-rst}
.. autoclass:: fpeg.Grammar
   :members:
```

## Errors

```{eval-rst}
.. autoexception:: fpeg.GrammarError

.. autoclass:: fpeg.Diagnostic
   :members:
```
