# Python API

The `fpeg` package compiles a grammar from text and parses inputs into a tree that stays in native memory. Python sees the tree through views: a `Tree` for each node and a `Token` for each token, made when they are first asked for.

```python
import fpeg

grammar = fpeg.Grammar(r"""
start: pair ("," pair)*
pair: key=NAME "=" value=NUMBER
NAME: [a-z]+
NUMBER: [0-9]+
%ignore " "+
""")

tree = grammar.parse("x = 1, y = 22")
assert tree.data == "start"
pair = tree.children[1]
assert pair.children == ["y", "22"]
assert pair.field("value") == "22"
assert pair.field("value").type == "NUMBER"
assert (pair.meta.line, pair.meta.column, pair.meta.end_column) == (1, 8, 14)
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

`parse` returns the one visible item at the top of the tree: a `Tree` for a node, or a `Token` for a token, which a transparent start rule gives when its one child is a token. A start rule that may leave no visible item or several, such as a rule `_name`, is refused before parsing, as an unknown one is.

```python
numbers = fpeg.Grammar('?start: NUMBER | list\nlist: "[" (start ** ",") "]"\nNUMBER: [0-9]+')
assert isinstance(numbers.parse("7"), fpeg.Token)
assert numbers.parse("[7,8]").data == "list"
```

A grammar is immutable, so one grammar serves any number of parses from any thread. A parse of a longer input lets other Python threads run while it works.

```{eval-rst}
.. autoclass:: fpeg.Grammar
   :members:
```

## Tree

A `Tree` is a node: `data` is the name of its rule or alias, `children` its visible items, and `meta` where it spans. Hidden tokens, such as the `=` above, stay in the native tree but not among the children. `field(name)` gives a field of the node by how many items the grammar lets it hold: the item, or `None` when it is absent, for a field of at most one item, and a list for a field that can hold several.

```python
calls = fpeg.Grammar(r"""
start: name=NAME "(" args=(NAME ** ",") ")"
NAME: [a-z]+
%ignore " "+
""")
call = calls.parse("f(a, b)")
assert call.field("name") == "f"
assert call.field("args") == ["a", "b"]
assert call.text == "f(a, b)"
```

`Tree(data, children, meta=None)` builds a tree in Python, as in Lark.

```{eval-rst}
.. autoclass:: fpeg.Tree
   :members:

.. autoclass:: fpeg.Meta
   :members:
```

## Token

A `Token` is a `str` with the name of its kind in `type` and its place in Lark's attributes: `start_pos` and `end_pos` count code points, `line` and `column` count from 1, and `end_column` is the column after the last character. Tokens compare and hash as their text. Their attributes are read-only, and `update()` makes a token with another type or text at the same place.

```python
number = pair.field("value")
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

A parse that does not match raises a `ValueError` that says what the grammar expected there, and so does a parse that reaches a limit. `max_steps` and `max_depth` lower the limits for a parse, whose defaults suit untrusted input.

```{eval-rst}
.. autoexception:: fpeg.GrammarError

.. autoclass:: fpeg.Diagnostic
   :members:
```
