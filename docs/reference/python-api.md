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
print(call.pretty())
```

`Tree(data, children, meta=None)` builds a tree in Python, as in Lark. Trees compare equal when their data and children are, hash and pickle. Every walk over a tree runs without recursion, so trees of any depth print, compare and pickle.

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

## Transformer

A `Transformer` turns a tree into values. A method named like a rule gives the value of its nodes, a method named like a terminal the value of its tokens, and each is called after the children of its node, with their values. What a method gets, its signature says:

| Method | Gets |
|---|---|
| `def pair(self, key, value)`, every parameter a field | the fields by name: the value, or `None` when absent, for a field of at most one item, a list for a field that can hold several |
| `def pair(self, *children)` | the visible children as arguments |
| `def pair(self, children)`, a parameter that is no field | the list of the visible children, as in Lark |
| `def pair(self)` | nothing |
| `def NAME(self, token)` | the token |
| `meta`, `node` or `children` as keyword-only parameters | beside the rest: where the node spans, the node as parsed, the list of children |

```python
class Pairs(fpeg.Transformer):
    def pair(self, key, value):
        return (str(key), int(value))

    def start(self, *pairs):
        return dict(pairs)


assert Pairs().transform(grammar.parse("x = 1, y = 22")) == {"x": 1, "y": 22}
```

A node without a method stays a tree, and a tree no method changes comes back as it is, so a transformer pays only for the nodes it handles. `return fpeg.Discard` removes a node from its parent, and a method that yields puts each value into the parent in the node's place, so the parent's list stays flat:

```python
lists = fpeg.Grammar(r"""
start: (item ** ",")
?item: NAME | "(" (item ** ",") ")" -> group
NAME: [a-z]+
%ignore " "+
""")


class Flatten(fpeg.Transformer):
    def group(self, *items):
        yield from items

    def start(self, *items):
        return list(items)


assert Flatten().transform(lists.parse("a, (b, (c, d)), e")) == ["a", "b", "c", "d", "e"]
```

An exception in a method keeps its type and gets a note with the method and where its node stands, which the traceback shows. A wrong signature fails at the first transform with the fields of the node. `class T(fpeg.Transformer, strict=True)` also makes every public method of `T` name a rule or terminal, so a misspelled method fails instead of never being called.

```python
class Strict(fpeg.Transformer, strict=True):
    def pairs(self, children):
        return children


try:
    Strict().transform(grammar.parse("x = 1"))
except TypeError as error:
    assert "did you mean pair" in str(error)


class Failing(fpeg.Transformer):
    def pair(self, key, value):
        raise ValueError(f"no {key}")


try:
    Failing().transform(grammar.parse("x = 1, y = 2"))
except ValueError as error:
    assert error.__notes__ == ["fpeg: in Failing.pair, for the node pair at line 1, column 1"]
```

`grammar.transformer_template(name)` writes a starting transformer for a grammar: a method for every rule and terminal, the parameters of a node's method named after its fields with type hints from how many items each holds, the builders a method replaces, and `strict=True`.

```python
template = grammar.transformer_template("Pairs")
assert "    def pair(self, key: Any, value: Any) -> Any:" in template
assert "        key holds one item or None." in template
```

`fpeg.v_args` reads Lark's decorator, so ported code runs: `inline=True` passes the children as arguments, `meta=True` the position first, `tree=True` a tree of the transformed children. A tree built in Python transforms too, by its `data`, without fields.

```{eval-rst}
.. autoclass:: fpeg.Transformer
   :members: transform, __default__, __default_token__

.. autofunction:: fpeg.v_args
```

## Value builders

A grammar can say itself what its nodes and tokens are worth: `=> builder` after an alternative, as the [grammar reference](grammar.md#values) describes. A transformer without a method for a node uses its builder, so a bare `fpeg.Transformer()` turns a tree into values with no Python method called, and a method still overrides a builder.

```python
json = fpeg.Grammar(r"""
?start: value
?value: object | array | STRING | NUMBER
      | "true" => True | "false" => False | "null" => None
object: "{" (member ** ",") "}" => dict
member: STRING ":" value => tuple
array: "[" (value ** ",") "]" => list
STRING: "\"" ([^"\\] | "\\" .)* "\"" => unquote
NUMBER: "-"? [0-9]+ ("." [0-9]+)? => float
%ignore [ \t\r\n]+
""")

value = fpeg.Transformer().transform(json.parse('{"a": [1.5, true, null, "\\u00e9"]}'))
assert value == {"a": [1.5, True, None, "\u00e9"]}
```

A builder may call a function of your own, given with the grammar in `functions`. Every function the builders call must be given, which loading the grammar checks ([E208](diagnostics/E208.md)).

```python
from dataclasses import dataclass


@dataclass
class Assign:
    name: str
    value: int


assignments = fpeg.Grammar(
    r"""
start: (assign ** ";") => list
assign: name=NAME "=" value=NUMBER => Assign(name, value)
NAME: [a-z]+ => text
NUMBER: [0-9]+ => int
%ignore " "+
""",
    functions={"Assign": Assign},
)
result = fpeg.Transformer().transform(assignments.parse("x = 1; y = 2"))
assert result == [Assign("x", 1), Assign("y", 2)]
```

## Errors

A parse that does not match raises `ParseError`, a `ValueError`, with the place of the failure and what the grammar expected there. A parse that reaches a limit raises its subclass `LimitError`. `max_steps` and `max_depth` lower the limits for a parse, whose defaults suit untrusted input.

```python
try:
    grammar.parse("x = 1, y = ")
except fpeg.ParseError as error:
    assert (error.line, error.column, error.pos) == (1, 12, 11)
    assert error.expected == ("NUMBER",)
    assert str(error) == "line 1, column 12: expected NUMBER"

try:
    grammar.parse("x = 1, y = 2", max_steps=10)
except fpeg.LimitError as error:
    assert error.limit == "steps"
```

```{eval-rst}
.. autoexception:: fpeg.ParseError

.. autoexception:: fpeg.LimitError

.. autoexception:: fpeg.GrammarError

.. autoclass:: fpeg.Diagnostic
   :members:
```
