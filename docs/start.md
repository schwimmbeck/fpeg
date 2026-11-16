# Getting started

This page installs fpeg, writes a first grammar, parses an input with it and turns the result into Python values. It takes about five minutes.

## Install

fpeg needs Python 3.11 or newer, and installs from PyPI:

```sh
pip install fpeg
```

```{note}
fpeg is not released yet. Until its first release, build it from a checkout as [Contributing](contributing.md) describes.
```

Then check that it imports, which prints its version:

```sh
python -c "import fpeg; print(fpeg.__version__)"
```

## A first grammar

A grammar is a string of rules, loaded at runtime. This one reads pairs such as `x = 1`, separated by commas:

```python
import fpeg

grammar = fpeg.Grammar(r"""
start: pair ** ","
pair: key=NAME "=" value=NUMBER
NAME: [a-z]+
NUMBER: [0-9]+
%ignore " "+
""")
```

- Lowercase names are rules, which give the tree its nodes. Uppercase names are terminals, which match the tokens.
- `"="` is a literal, `[a-z]` a class of characters, and `+` means one or more.
- `pair ** ","` matches pairs separated by commas, none or many.
- `key=` and `value=` name the parts of a pair as fields.
- `%ignore` names the text skipped between tokens, here spaces.

## Parsing

`parse` returns the tree of an input, starting at the rule `start`:

```python
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

Each node is a `Tree` with the name of its rule in `data` and what it matched in `children`. A token is a `str` that knows its terminal and its place in the input:

```python
pair = tree.children[1]
assert pair.data == "pair"
value = pair.field("value")
assert value == "22"
assert (value.type, value.line, value.column) == ("NUMBER", 1, 12)
```

Input that the grammar does not match raises `ParseError`, which says where the parse failed and what could have come there:

```python
try:
    grammar.parse("x = 1, y")
except fpeg.ParseError as error:
    print(error)
```

```output
line 1, column 9: expected "="
```

## Values

A `Transformer` turns a tree into values of your own. Each method is named after a rule and returns the value of its nodes. It is called after the children of its node, so it gets their values, and its signature says how: `*pairs` takes the children, `key` and `value` take the fields.

```python
class ToDict(fpeg.Transformer):
    def start(self, *pairs):
        return dict(pairs)

    def pair(self, key, value):
        return str(key), int(value)


print(ToDict().transform(tree))
```

```output
{'x': 1, 'y': 22}
```

A grammar can also say what its nodes and tokens are worth, with value builders after `=>`, and a plain `Transformer` then needs no methods at all:

```python
grammar = fpeg.Grammar(r"""
start: pair ** "," => dict
pair: key=NAME "=" value=NUMBER => tuple
NAME: [a-z]+ => text
NUMBER: [0-9]+ => int
%ignore " "+
""")

assert fpeg.Transformer().transform(grammar.parse("x = 1, y = 22")) == {"x": 1, "y": 22}
```

## Where next

- The [examples](index.md#examples) are complete programs for common tasks, to copy and change.
- The tutorials go step by step: [your first grammar](tutorial/first-grammar.md) writes a grammar for JSON, and [from a tree to your own objects](tutorial/transformer.md) builds a syntax tree and computes with it.
- The [grammar language](reference/grammar.md) describes every form a grammar can use, and the [Python API](reference/python-api.md) everything the package offers.
