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

## Token

A `Token` is a `str` with the name of its kind in `type` and its place in Lark's attributes: `start_pos` and `end_pos` count code points, `line` and `column` count from 1, and `end_column` is the column after the last character. Tokens compare and hash as their text. Their attributes are read-only, and `update()` makes a token with another type or text at the same place.

```python
number = fpeg.Token("NUMBER", "22", 11, 1, 12, end_line=1, end_column=14, end_pos=13)
assert isinstance(number, str) and number == "22"
assert (number.type, number.start_pos, number.end_pos) == ("NUMBER", 11, 13)
assert number.update(type="INT").type == "INT"
assert fpeg.Token("NAME", "x") == "x"
```

```{eval-rst}
.. autoclass:: fpeg.Token
   :members: update, new_borrow_pos
```

## Errors

```{eval-rst}
.. autoexception:: fpeg.GrammarError

.. autoclass:: fpeg.Diagnostic
   :members:
```
