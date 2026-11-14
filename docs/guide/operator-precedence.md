# Operator precedence

`1 + 2 * 3` is 7 and `8 - 4 - 2` is 2: multiplication binds tighter than addition, and subtraction groups to the left. A grammar for expressions has to say both for every operator. This guide shows how a precedence block says it, with a calculator that orders its operators as Python does and computes what Python computes for numbers with a fraction.

## One rule per level

Without a precedence block, each level of operators is a rule that repeats the next tighter one:

```python
import fpeg

levels = fpeg.Grammar(r"""
?start: sum
sum: product (op=("+" | "-") product)*
?product: NUMBER (op=("*" | "/") NUMBER)+ -> product | NUMBER
NUMBER: [0-9]+
%ignore " "+
""")

print(levels.parse("8 - 4 - 2 * 3").pretty())
```

```output
sum
  8
  op=-
  4
  op=-
  product
    2
    op=*
    3
```

The levels are right, and the grouping is missing: a chain of operators is one flat node, and the code that reads the tree has to fold it from the left itself. An operator that groups to the right needs another shape of rule, and a sign in front another one again.

## A precedence block

A precedence block lists the levels from loosest to tightest and says the grouping of each operator by where the level calls itself:

```python
calculator = fpeg.Grammar(r"""
?start: expr

expr: %precedence {
    sum: left=sum op=("+" | "-") right=product -> binary
    product: left=product op=("*" | "/") right=unary -> binary
    unary: "-" operand=unary -> negate
    power: left=atom op="**" right=unary -> binary
    atom: NUMBER | "(" expr ")"
}

NUMBER: [0-9]+ ("." [0-9]+)? => float
%ignore " "+
""")

print(calculator.parse("8 - 4 - 2").pretty())
```

```output
binary
  left=binary
    left=8
    op=-
    right=4
  op=-
  right=2
```

Every operation is a node of its own with its operands in fields, grouped as the language means it.

- A level that calls **itself on the left** and the next tighter level on the right groups to the left: `left=sum ... right=product`.
- A level that calls a tighter level on the left and **itself or a looser level on the right** groups to the right. `power` takes `unary` on its right, which is looser than itself, so a power may stand in the exponent of a power, and a sign too.
- A level that fits none of its alternatives is the next tighter level. So `atom` alone is a `sum`, and no node marks the way down.
- An alternative without `->` makes no node, which is what `"(" expr ")"` wants: the parentheses group and leave the inner expression.

A power groups to the right, and a sign in front of a power applies to the whole power, as in Python:

```python
print(calculator.parse("2 ** 3 ** 2").pretty())
print(calculator.parse("-2 ** 2").pretty())
```

```output
binary
  left=2
  op=**
  right=binary
    left=3
    op=**
    right=2

negate
  operand=binary
    left=2
    op=**
    right=2
```

## Computing the value

With every operation in a node of its own, a `Transformer` needs one method for all binary operators and one for the sign. `=> float` in the grammar has made the numbers already.

```python
import operator

OPERATORS = {
    "+": operator.add,
    "-": operator.sub,
    "*": operator.mul,
    "/": operator.truediv,
    "**": operator.pow,
}


class Calculate(fpeg.Transformer):
    def binary(self, left, op, right):
        return OPERATORS[op](left, right)

    def negate(self, operand):
        return -operand


def calculate(text):
    return Calculate().transform(calculator.parse(text))


for text in ["8 - 4 - 2", "2 ** 3 ** 2", "-2 ** 2", "1 + 2 * 3 - 2 ** 3 ** 2 / -4"]:
    print(text, "=", calculate(text))
    assert calculate(text) == eval(text)
```

```output
8 - 4 - 2 = 2.0
2 ** 3 ** 2 = 512.0
-2 ** 2 = -4.0
1 + 2 * 3 - 2 ** 3 ** 2 / -4 = 135.0
```

## Operators that do not chain

A comparison such as `1 < 2 < 3` is an error in many languages. A level that takes a tighter level on both sides does not chain:

```python
comparing = fpeg.Grammar(r"""
?start: expr

expr: %precedence {
    compare: left=sum op=("<" | ">" | "==") right=sum -> compare
    sum: left=sum op=("+" | "-") right=atom -> binary
    atom: NUMBER | "(" expr ")"
}

NUMBER: [0-9]+
%ignore " "+
""")

print(comparing.parse("1 + 2 < 4").pretty())

try:
    comparing.parse("1 < 2 < 3")
except fpeg.ParseError as error:
    print(error)
```

```output
compare
  left=binary
    left=1
    op=+
    right=2
  op=<
  right=4

line 1, column 7: expected "+", "-", end of input
```

## Prefix and postfix operators

A prefix operator is a level that calls itself after its operator, a postfix operator one that calls itself before it. Calls, member access and indexing are postfix operators too, each an alternative of one level:

```python
postfix = fpeg.Grammar(r"""
?start: expr

expr: %precedence {
    sum: left=sum op=("+" | "-") right=unary -> binary
    unary: op=("-" | "!") operand=unary -> prefix
    postfix: func=postfix "(" args=(expr ** ",") ")" -> call
        | object=postfix "." name=NAME -> member
        | operand=postfix "!" -> factorial
    atom: NUMBER | NAME | "(" expr ")"
}

NAME: [a-z]+
NUMBER: [0-9]+
%ignore " "+
""")

print(postfix.parse("-f(1, x).size! + 2").pretty())
```

```output
binary
  left=prefix
    op=-
    operand=factorial
      operand=member
        object=call
          func=f
          args=1
          args=x
        name=size
  op=+
  right=2
```

## Left recursion outside a block

A level that calls itself on the left is a left recursive rule, and any rule may be one. A path of names groups to the left like a chain of operators:

```python
paths = fpeg.Grammar(r"""
?start: path
?path: object=path "." name=NAME -> member | NAME
NAME: [a-z]+
""")

print(paths.parse("a.b.c").pretty())
```

```output
member
  object=member
    object=a
    name=b
  name=c
```

A rule that calls itself at both ends of an alternative, as `sum: sum "-" sum`, groups to the right, which is seldom meant. `check` warns about it, and so does `fpeg check` on the command line:

```python
both = 'start: sum\nsum: left=sum "-" right=sum -> minus | NUMBER\nNUMBER: [0-9]+\n'
for found in fpeg.check(both):
    print(found)
```

```output
<grammar>:2:6: warning[W202]: sum calls itself at both ends of this alternative, which groups to the right
```

## What it costs

A level is tried at every operand, and each try runs down through the levels below it. fpeg remembers the result of a level at a position, so that the levels above reuse it, and does so by default for the levels of every precedence block and for left recursive rules. Nothing has to be set. [Performance](performance.md) says how to see where a parse spends its work.

The syntax in full is in the reference of the [grammar language](../reference/grammar.md#operator-precedence), and what it means in the [semantics](../reference/semantics.md).
