# JSON to Python objects

JSON documents turned into the Python objects that `json.loads` gives. Value builders in the grammar make the strings, numbers and constants, and a `Transformer` builds the dictionaries and lists.

```python
import json

import fpeg

grammar = fpeg.Grammar(r"""
?start: value
?value: object | array | STRING | FLOAT | INT
      | "true" => True | "false" => False | "null" => None
object: "{" (member ** ",") "}"
member: key=STRING ":" value=value
array: "[" (value ** ",") "]"

STRING: "\"" ([^"\\] | "\\" .)* "\"" => unquote
INT: "-"? ("0" | [1-9] [0-9]*) => int
FLOAT: "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+ ([eE] [+-]? [0-9]+)? | [eE] [+-]? [0-9]+) => float
%ignore [ \t\r\n]+
""")


class ToPython(fpeg.Transformer):
    def object(self, *members):
        return dict(members)

    def member(self, key, value):
        return key, value

    def array(self, *values):
        return list(values)


TEXT = """
{
  "name": "fpeg",
  "version": [0, 1, 0],
  "keywords": ["parser", "PEG"],
  "stable": false,
  "homepage": null,
  "ratio": 2.5e-1
}
"""

value = ToPython().transform(grammar.parse(TEXT))
print(value)
assert value == json.loads(TEXT)
```

```output
{'name': 'fpeg', 'version': [0, 1, 0], 'keywords': ['parser', 'PEG'], 'stable': False, 'homepage': None, 'ratio': 0.25}
```

## How it works

- `?start` and `?value` give way to the one value they hold, so that no value sits in a node of its own.
- `FLOAT` comes before `INT`: an ordered choice takes the first alternative that matches. A whole number fails `FLOAT`, which needs a fraction or an exponent, and is taken by `INT`, so whole numbers become `int` as in `json.loads`.
- The builders make the values of tokens: `unquote` removes the quotes of a string and decodes its escapes, `int` and `float` read numbers, and `True`, `False` and `None` are the constants of the literals.
- `(member ** ",")` matches members separated by commas, none or many. `object` gets the values of its members, the pairs that `member` returns, and makes a dictionary of them.

The [first tutorial](../tutorial/first-grammar.md) writes this grammar step by step.

## The JSON grammar of fpeg

fpeg comes with a grammar for JSON, which a grammar of your own can extend or use as it is. Builders make the value of every rule, so a plain `Transformer` turns a parse into Python objects without calling any method of yours. Its errors name the mistake, and `render()` shows the line it stands in:

```python
standard = fpeg.Grammar('%extends "json"')
assert fpeg.Transformer().transform(standard.parse(TEXT)) == json.loads(TEXT)

try:
    standard.parse('{"sizes": [1, 2}')
except fpeg.ParseError as error:
    print(error.render())
```

```output
error[bracket]: expected ",", "]"
 --> 1:16
  |
1 | {"sizes": [1, 2}
  |                ^
```

[Composing grammars](../guide/composing-grammars.md) shows how to extend it, and [errors and recovery](../guide/errors.md) how its errors and a parse that goes on past them work.
