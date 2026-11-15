# A filter language

A small language of conditions, such as `age >= 18 and (city == "Berlin" or admin)`, the kind a search box or a rule in a configuration reads. The grammar parses a condition, a `Transformer` turns its tree into a syntax tree of dataclasses, and a function evaluates that tree against records.

## The grammar

```python
import fpeg

grammar = fpeg.Grammar(r"""
?start: condition

condition: %precedence {
    disjunction: left=disjunction op="or" right=conjunction -> binary
    conjunction: left=conjunction op="and" right=negation -> binary
    negation: "not" operand=negation -> negate
    comparison: left=operand op=COMPARE right=operand -> binary
    operand: NUMBER | STRING | name=NAME -> field | "(" condition ")"
}

COMPARE: "==" | "!=" | "<=" | ">=" | "<" | ">"
@reserved
NAME: [a-z_]+
NUMBER: [0-9]+ => int
STRING: "\"" [^"]* "\"" => unquote
%ignore " "+
%word [a-z_]
""")
```

- The [precedence block](../guide/operator-precedence.md) lists the levels from loosest to tightest, so `and` binds tighter than `or`, `not` tighter than `and`, and a comparison tighter than `not`. A comparison calls `operand` on both sides and not itself, so comparisons do not chain: `a < b < c` is an error.
- `-> binary` makes one kind of node for `or`, `and` and the comparisons, with the fields `left`, `op` and `right`. A literal that is a whole field, as in `op="or"`, gives the field its token, which would be hidden otherwise.
- `@reserved` keeps the keywords out of `NAME`, so `and` is never a field. `%word` makes a keyword a whole word: `not` does not match the start of `nothing`.

## The syntax tree

Three dataclasses make the syntax tree, and a method for each kind of node builds them:

```python
from dataclasses import dataclass


@dataclass
class Binary:
    left: object
    op: str
    right: object


@dataclass
class Not:
    operand: object


@dataclass
class Field:
    name: str


class ToAst(fpeg.Transformer, strict=True):
    def binary(self, left, op, right):
        return Binary(left, str(op), right)

    def negate(self, operand):
        return Not(operand)

    def field(self, name):
        return Field(str(name))


print(ToAst().transform(grammar.parse("not age < 18 or admin")))
```

```output
Binary(left=Not(operand=Binary(left=Field(name='age'), op='<', right=18)), op='or', right=Field(name='admin'))
```

Each method gets the fields of its node by name, with the values the children already became. Numbers and strings get their values from the builders `=> int` and `=> unquote`, so they need no method. `strict=True` checks that every method names a rule, which catches a misspelled one.

## Evaluating

A function walks the syntax tree with Python's `match`, and evaluates `and` and `or` only as far as it must:

```python
import operator

COMPARISONS = {
    "==": operator.eq,
    "!=": operator.ne,
    "<": operator.lt,
    "<=": operator.le,
    ">": operator.gt,
    ">=": operator.ge,
}


def evaluate(node, record):
    match node:
        case Binary(left, "and", right):
            return evaluate(left, record) and evaluate(right, record)
        case Binary(left, "or", right):
            return evaluate(left, record) or evaluate(right, record)
        case Binary(left, op, right):
            return COMPARISONS[op](evaluate(left, record), evaluate(right, record))
        case Not(operand):
            return not evaluate(operand, record)
        case Field(name):
            return record[name]
        case _:
            return node


people = [
    {"name": "Ada", "age": 36, "city": "London", "admin": True, "banned": False},
    {"name": "Bo", "age": 17, "city": "Berlin", "admin": False, "banned": False},
    {"name": "Cem", "age": 41, "city": "Berlin", "admin": False, "banned": False},
    {"name": "Dee", "age": 29, "city": "Berlin", "admin": False, "banned": True},
]
condition = ToAst().transform(
    grammar.parse('age >= 18 and (city == "Berlin" or admin) and not banned')
)
print([person["name"] for person in people if evaluate(condition, person)])
```

```output
['Ada', 'Cem']
```

The condition is parsed once and evaluated for each record.

## Errors

A condition that does not parse says where, and what could have come there:

```python
for text in ["age >= 18 and", 'age >= 18 and (city == "Berlin"']:
    try:
        grammar.parse(text)
    except fpeg.ParseError as error:
        print(error)
```

```output
line 1, column 14: expected "(", "not", NAME, NUMBER, STRING
line 1, column 32: expected ")", "and", "or"
```
