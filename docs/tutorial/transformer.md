# From a tree to your own objects

Builders turn a tree into plain Python data. For objects of your own, such as the syntax tree of a language or a running computation, write a `Transformer`: a class with one method for each rule whose nodes you want to turn into something, as [Getting started](../start.md#values) does in a few lines. This tutorial goes further and builds a small language twice, first into a syntax tree of dataclasses, then into the values it computes, and shows how to change the shape of the result, what happens to errors, and how to catch a misspelled method.

## The language

A program is a list of statements: `let` binds names to values, and `show` gives the value of an expression with sums, products and parentheses.

```python
import fpeg

grammar = fpeg.Grammar(r"""
start: (statement ** ";")
?statement: let | show
let: "let" (binding ++ ",")
binding: name=NAME "=" value=expr
show: "show" value=expr
?expr: sum
?sum: product ("+" product)*
?product: atom ("*" atom)*
?atom: NUMBER | name=NAME -> var | "(" expr ")"
NAME: [a-z]+
NUMBER: [0-9]+
%ignore [ \t\r\n]+
""")

program = grammar.parse("let x = 2, y = 3; show x * (y + 1)")
print(program.pretty())
```

```output
start
  let
    binding
      name=x
      value=2
    binding
      name=y
      value=3
  show
    value=product
      var	name=x
      sum
        var	name=y
        1
```

`(binding ++ ",")` matches one or more bindings separated by commas. The rules `?sum` and `?product` are transparent, so a sum of one product is just that product, and `?atom` always gives way to what it holds: a number, a name, or the expression inside the parentheses. `-> var` names the node of a name used as a value, apart from the names that `let` binds.

## Start from a template

`transformer_template` writes a starting point: a method for every rule that makes nodes and for every terminal, each method's parameters named after the fields of its node:

```python
print(grammar.transformer_template("ToAst"))
```

```output
from __future__ import annotations

from typing import Any

import fpeg


class ToAst(fpeg.Transformer, strict=True):
    """Values for the nodes and tokens of the grammar."""

    def start(self, *children: Any) -> Any:
        """Return the value of a node start from its children."""
        ...

    def let(self, *children: Any) -> Any:
        """Return the value of a node let from its children."""
        ...

    def NAME(self, token: fpeg.Token) -> Any:
        """Return the value of a token NAME."""
        ...

    def binding(self, name: Any, value: Any) -> Any:
        """Return the value of a node binding.

        name holds one item.
        value holds one item.
        """
        ...

    def show(self, value: Any) -> Any:
        """Return the value of a node show.

        value holds one item.
        """
        ...

    def sum(self, *children: Any) -> Any:
        """Return the value of a node sum from its children."""
        ...

    def product(self, *children: Any) -> Any:
        """Return the value of a node product from its children."""
        ...

    def NUMBER(self, token: fpeg.Token) -> Any:
        """Return the value of a token NUMBER."""
        ...

    def var(self, name: Any) -> Any:
        """Return the value of a node var.

        name holds one item.
        """
        ...
```

A method you do not need can go: a node without a method stays a tree, and a token stays a token.

## Building a syntax tree

Fill in the methods to return dataclasses. Each method is called after the children of its node, so it gets their values, not their trees:

```python
from dataclasses import dataclass


@dataclass
class Let:
    name: str
    value: object


@dataclass
class Show:
    value: object


@dataclass
class Sum:
    terms: list[object]


@dataclass
class Product:
    factors: list[object]


@dataclass
class Var:
    name: str


class ToAst(fpeg.Transformer, strict=True):
    def start(self, *statements):
        return list(statements)

    def let(self, *bindings):
        yield from bindings

    def binding(self, name, value):
        return Let(str(name), value)

    def show(self, value):
        return Show(value)

    def sum(self, *terms):
        return Sum(list(terms))

    def product(self, *factors):
        return Product(list(factors))

    def var(self, name):
        return Var(str(name))

    def NUMBER(self, token):
        return int(token)


for statement in ToAst().transform(program):
    print(statement)
```

```output
Let(name='x', value=2)
Let(name='y', value=3)
Show(value=Product(factors=[Var(name='x'), Sum(terms=[Var(name='y'), 1])]))
```

What a method gets, its signature says:

- `binding(self, name, value)` names fields, so it gets the fields by name. A field of one item gives the item, and a field the template marks `Any | None` gives `None` when absent.
- `sum(self, *terms)` gets the children as arguments. A single parameter that names no field, such as `children`, would get them as one list.
- `NUMBER(self, token)` gets the token, a `str` with its position.
- Beside these, keyword-only parameters ask for more: `*, meta` for where the node spans, `*, node` for the node as parsed, `*, children` for the list of children next to the fields.

## Changing the shape

A method decides what its node becomes in the parent:

- `return value` replaces the node by the value.
- `return fpeg.Discard` removes the node.
- A method that yields puts each value into the parent in the node's place, in order. That is how `let` above turns one statement with two bindings into two `Let` statements of the program.

The grammar shapes the tree before any of this: `?rule` gives way to a single child, `_rule` puts its children into the parent, `-> name` renames a node, and fields name the parts, so that nothing depends on their position.

## Computing values

A transformer may keep state. Since the children of a node come before the node, and the nodes in the order of the input, the statements of a program run in order:

```python
import math


class Run(fpeg.Transformer):
    def __init__(self):
        super().__init__()
        self.names = {}

    def start(self, *shown):
        return list(shown)

    def let(self):
        return fpeg.Discard

    def binding(self, name, value):
        self.names[name] = value
        return fpeg.Discard

    def show(self, value):
        return value

    def sum(self, *terms):
        return sum(terms)

    def product(self, *factors):
        return math.prod(factors)

    def var(self, name):
        if name not in self.names:
            raise NameError(f"{name} is not defined")
        return self.names[name]

    def NUMBER(self, token):
        return int(token)


print(Run().transform(grammar.parse("let x = 2, y = 3; show x * (y + 1); show y")))
```

```output
[8, 3]
```

Each binding stores its value and removes itself, so `let` is left without children and removes itself as well, and `start` gets only the values of the `show` statements.

## Errors

An exception in a method stops the transform and keeps its type, so `except` catches it as usual. fpeg adds a note that says which method raised it and where the node stands in the input:

```python
try:
    Run().transform(grammar.parse("let x = 1;\nshow x + y"))
except NameError as error:
    print(error, *error.__notes__, sep="\n")
```

```output
y is not defined
fpeg: in Run.var, for the node var at line 2, column 10
```

To report a problem of your own, raise any exception. The traceback shows the note too. A position you want to keep, in an error message or in your syntax tree, comes from `meta` or from the tokens, which carry `line` and `column`.

## Catching misspelled methods

A method whose name matches no rule is never called, which is easy to miss. With `strict=True`, as the template writes it, every public method of the class must name a rule or terminal of the grammar, checked at the first transform:

```python
class Typo(fpeg.Transformer, strict=True):
    def bindng(self, name, value):
        return (name, value)


try:
    Typo().transform(program)
except TypeError as error:
    print(error)
```

```output
Typo.bindng names no rule or terminal of the grammar, did you mean binding?
```

## Builders and methods together

Builders can call functions of your own, given with the grammar, and they take fields as arguments. Where a builder does the job, a method is not needed, and a method still overrides the builder of its rule. Here builders make the syntax tree, and one method splices the bindings:

```python
values = fpeg.Grammar(
    r"""
start: (statement ** ";") => list
?statement: let | show
let: "let" (binding ++ ",")
binding: name=NAME "=" value=expr => Let(name, value)
show: "show" value=expr => Show(value)
?expr: sum
?sum: product ("+" product)* => Sum(*)
?product: atom ("*" atom)* => Product(*)
?atom: NUMBER | name=NAME -> var => Var(name) | "(" expr ")"
NAME: [a-z]+ => text
NUMBER: [0-9]+ => int
%ignore [ \t\r\n]+
""",
    functions={"Let": Let, "Show": Show, "Sum": Sum, "Product": Product, "Var": Var},
)


class Splice(fpeg.Transformer):
    def let(self, *bindings):
        yield from bindings


assert Splice().transform(values.parse("let x = 2, y = 3; show x * (y + 1)")) == [
    Let("x", 2),
    Let("y", 3),
    Show(Product([Var("x"), Sum([Var("y"), 1])])),
]
```

`Sum(*)` passes the list of the children's values. Without `Splice`, `let` has neither a method nor a builder and stays a tree that holds the two `Let` objects.

## Where next

- [A filter language](../examples/filter-language.md) builds a syntax tree of dataclasses and evaluates it with `match`.
- [Building an AST](../guide/building-an-ast.md) builds the syntax tree of a language with `if` and runs its programs, and shows where each node stands in the input.
- The [Python API](../reference/python-api.md#transformer) lists everything a method can ask for.
