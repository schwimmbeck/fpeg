# A calculator

A calculator for lines of arithmetic, with Python's operators and their order, variables and a few functions. The grammar reads one line, and a `Transformer` computes its value as it goes and keeps the variables from one line to the next.

```python
import math
import operator

import fpeg

grammar = fpeg.Grammar(r"""
?start: assignment | expr
assignment: name=NAME "=" value=expr

expr: %precedence {
    sum: left=sum op=("+" | "-") right=product -> binary
    product: left=product op=("*" | "/") right=unary -> binary
    unary: "-" operand=unary -> negate
    power: left=atom op="**" right=unary -> binary
    atom: NUMBER
        | function=NAME "(" arguments=(expr ** ",") ")" -> call
        | name=NAME -> variable
        | "(" expr ")"
}

NAME: [a-z_]+
NUMBER: [0-9]+ ("." [0-9]+)? => float
%ignore " "+
""")

OPERATORS = {
    "+": operator.add,
    "-": operator.sub,
    "*": operator.mul,
    "/": operator.truediv,
    "**": operator.pow,
}
FUNCTIONS = {"sqrt": math.sqrt, "max": max, "min": min}


class Calculator(fpeg.Transformer):
    def __init__(self):
        super().__init__()
        self.variables = {"pi": math.pi}

    def assignment(self, name, value):
        self.variables[name] = value
        return value

    def binary(self, left, op, right):
        return OPERATORS[op](left, right)

    def negate(self, operand):
        return -operand

    def call(self, function, arguments):
        if function not in FUNCTIONS:
            raise NameError(f"no function {function}")
        return FUNCTIONS[function](*arguments)

    def variable(self, name):
        if name not in self.variables:
            raise NameError(f"{name} is not defined")
        return self.variables[name]


LINES = """\
width = 3
height = width + 1
sqrt(width ** 2 + height ** 2)
max(1, -2 ** 2, width / 2)
"""

calculator = Calculator()
for line in LINES.splitlines():
    print(line, "->", calculator.transform(grammar.parse(line)))
```

```output
width = 3 -> 3.0
height = width + 1 -> 4.0
sqrt(width ** 2 + height ** 2) -> 5.0
max(1, -2 ** 2, width / 2) -> 1.5
```

## How it works

- The [precedence block](../guide/operator-precedence.md) lists the levels of operators from loosest to tightest. A level that calls itself on the left groups to the left, as `8 - 4 - 2` is `(8 - 4) - 2`, and `power`, which calls a looser level on its right, groups to the right, as `2 ** 3 ** 2` is `2 ** (3 ** 2)`.
- `-> binary` makes one kind of node for every binary operator, with its operands and its operator in the fields `left`, `op` and `right`, so one method computes them all.
- `=> float` on `NUMBER` is a value builder: the tokens of numbers become floats without a method.
- Each method is called after the children of its node, so `binary` gets the values of its operands, not their trees.
- `(expr ** ",")` matches arguments separated by commas, none or many. The field `arguments` can hold several, so `call` gets them as a list.
- `?start` gives way to what it holds, so the value of a line is the value of its assignment or its expression.

## Errors

A line that does not parse raises `ParseError`, which says where and what could have come there. An exception raised by a method keeps its type, and fpeg adds a note that names the method and the place of its node:

```python
for line in ["2 * (3 + 4", "depth * 2"]:
    try:
        calculator.transform(grammar.parse(line))
    except fpeg.ParseError as error:
        print(error)
    except NameError as error:
        print(error, *error.__notes__, sep="\n")
```

```output
line 1, column 11: expected ")", "*", "**", "+", "-", "/"
depth is not defined
fpeg: in Calculator.variable, for the node variable at line 1, column 1
```
