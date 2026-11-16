# Building an AST

A parse tree holds every node the grammar makes, in the terms of the grammar. A program that works on the input mostly wants its own objects: a class for each construct, with typed attributes and nothing left of the syntax. This guide builds such an abstract syntax tree for a small language, first with methods and then with builders in the grammar, and runs the programs.

## The language and its tree

A program is a list of statements: assignments with `let`, `if` with an optional `else`, and `print`.

```python
import fpeg

RULES = r"""
start: statement*
?statement: let | if | show
let: "let" name=NAME "=" value=expr ";"
if: "if" test=expr "{" then=statement* "}" ("else" "{" otherwise=statement* "}")?
show: "print" value=expr ";"

expr: %precedence {
    compare: left=sum op=COMPARE right=sum -> binary
    sum: left=sum op=SUM right=product -> binary
    product: left=product op=PRODUCT right=atom -> binary
    atom: NUMBER | name=NAME -> load | "(" expr ")"
}

COMPARE: "<" | ">" | "=="
SUM: "+" | "-"
PRODUCT: "*" | "/"
@reserved
NAME: [a-z]+
NUMBER: [0-9]+
%ignore [ \n]+
%word [a-z]
"""
grammar = fpeg.Grammar(RULES)

PROGRAM = """
let width = 6;
let area = width * (width + 1);
if area > 40 { print area; } else { print 0; }
"""
tree = grammar.parse(PROGRAM)
print(tree.children[0].pretty())
```

```output
let
  name=width
  value=6
```

Three choices in the grammar prepare the tree for what follows:

- **Fields name the parts.** `name=`, `value=`, `test=` say what each child is, so that code asks for a part by name and not by position.
- **Aliases name the constructs.** The three operator levels all make `binary` nodes, since a program treats them alike. `-> load` makes a node for a name that is read, which tells it from the name that `let` writes.
- **`?statement` and the levels make no nodes.** A statement is a `let`, an `if` or a `show`, with nothing around it.

The method for the rule `if` is called `if_`. Python reserves `if`, so fpeg looks for a rule named like a keyword under the name with an underscore added.

## Classes for the constructs

One class for each construct, with the attributes a program needs:

```python
from dataclasses import dataclass


@dataclass
class Let:
    name: str
    value: object


@dataclass
class If:
    test: object
    then: list
    otherwise: list


@dataclass
class Show:
    value: object


@dataclass
class Binary:
    left: object
    op: str
    right: object


@dataclass
class Load:
    name: str
```

## With a Transformer

A `Transformer` has a method for each rule, named like it, and calls it after the children of the node. A method that names the fields as its parameters gets their values, so each method is the call of a constructor:

```python
class ToAst(fpeg.Transformer):
    def start(self, *statements):
        return list(statements)

    def let(self, name, value):
        return Let(str(name), value)

    def if_(self, test, then, otherwise):
        return If(test, then, otherwise)

    def show(self, value):
        return Show(value)

    def binary(self, left, op, right):
        return Binary(left, str(op), right)

    def load(self, name):
        return Load(str(name))

    def NUMBER(self, token):
        return int(token)


program = ToAst().transform(tree)
for statement in program:
    print(statement)
```

```output
Let(name='width', value=6)
Let(name='area', value=Binary(left=Load(name='width'), op='*', right=Binary(left=Load(name='width'), op='+', right=1)))
If(test=Binary(left=Load(name='area'), op='>', right=40), then=[Show(value=Load(name='area'))], otherwise=[Show(value=0)])
```

A field that holds several items, as `then` does, arrives as a list, and as an empty one where the input has none. A token is a `str` with its place in the source, and `str(name)` keeps the text alone.

## With builders in the grammar

Every method above does the same: it hands the fields to a class. A builder after `=>` says that in the grammar, and no method is left to write. The classes are given with the grammar:

```python
BUILT = r"""
start: statement* => list
?statement: let | if | show
let: "let" name=NAME "=" value=expr ";" => Let(name, value)
if: "if" test=expr "{" then=statement* "}" ("else" "{" otherwise=statement* "}")? => If(test, then, otherwise)
show: "print" value=expr ";" => Show(value)

expr: %precedence {
    compare: left=sum op=COMPARE right=sum -> binary => Binary(left, op, right)
    sum: left=sum op=SUM right=product -> binary => Binary(left, op, right)
    product: left=product op=PRODUCT right=atom -> binary => Binary(left, op, right)
    atom: NUMBER | name=NAME -> load => Load(name) | "(" expr ")"
}

COMPARE: ("<" | ">" | "==") => text
SUM: ("+" | "-") => text
PRODUCT: ("*" | "/") => text
@reserved
NAME: [a-z]+ => text
NUMBER: [0-9]+ => int
%ignore [ \n]+
%word [a-z]
"""
classes = {"Let": Let, "If": If, "Show": Show, "Binary": Binary, "Load": Load}
built = fpeg.Grammar(BUILT, functions=classes)

assert fpeg.Transformer().transform(built.parse(PROGRAM)) == program
```

`=> text` makes a token a plain string and `=> int` a number, `=> list` the list of a node's children, and `=> Let(name, value)` calls the class with the values of the two fields. A plain `Transformer` applies the builders. A method of a subclass still replaces the builder of its rule, for the one construct that needs more than a constructor.

A builder belongs to one alternative, so the alternatives of the operator terminals stand in parentheses: `("+" | "-") => text` is one alternative with one builder.

## Where a node stands

An error message of your own wants the line and the column of a construct. A method asks for them with a keyword-only parameter `meta`:

```python
class Placed(ToAst):
    def load(self, name, *, meta):
        return (str(name), meta.line, meta.column)


print(Placed().transform(grammar.parse("print a + b;")))
```

```output
[Show(value=Binary(left=('a', 1, 7), op='+', right=('b', 1, 11)))]
```

A class of your tree would keep such a place in an attribute.

## Running the tree

The AST is plain Python, and pattern matching reads it well:

```python
import operator

OPERATORS = {
    "<": operator.lt,
    ">": operator.gt,
    "==": operator.eq,
    "+": operator.add,
    "-": operator.sub,
    "*": operator.mul,
    "/": operator.floordiv,
}


def evaluate(expression, variables):
    match expression:
        case Binary(left, op, right):
            return OPERATORS[op](evaluate(left, variables), evaluate(right, variables))
        case Load(name):
            return variables[name]
        case _:
            return expression


def run(statements, variables):
    for statement in statements:
        match statement:
            case Let(name, value):
                variables[name] = evaluate(value, variables)
            case If(test, then, otherwise):
                run(then if evaluate(test, variables) else otherwise, variables)
            case Show(value):
                print(evaluate(value, variables))


run(program, {})
```

```output
42
```

## Without an AST

A program that only wants to run the input can walk the parse tree itself. An `Interpreter` has a method for each rule too, and gets the fields as parsed: it visits the parts it needs, when it needs them, so the branch of an `if` that is not taken never runs. An error keeps its type and says where its node stands.

```python
class Run(fpeg.Interpreter):
    def __init__(self):
        super().__init__()
        self.variables = {}

    def let(self, name, value):
        self.variables[str(name)] = self.visit(value)

    def if_(self, test, then, otherwise):
        for statement in then if self.visit(test) else otherwise:
            self.visit(statement)

    def show(self, value):
        print(self.visit(value))

    def binary(self, left, op, right):
        return OPERATORS[op](self.visit(left), self.visit(right))

    def load(self, name):
        return self.variables[str(name)]

    def NUMBER(self, token):
        return int(token)


Run().visit(tree)

try:
    Run().visit(grammar.parse("print 1;\nprint missing + 1;"))
except KeyError as error:
    print(error.__notes__[0])
```

```output
42
1
fpeg: in Run.load, for the node load at line 2, column 7
```

An AST pays off when a tree is read more than once: checked, changed, compiled. A walk over the parse tree is the shorter way to a single result.

The [Python API](../reference/python-api.md) lists what a method's signature can ask for, and the [grammar language](../reference/grammar.md#values) every builder.
