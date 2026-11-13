# Your first grammar

This tutorial writes a grammar for JSON, one step at a time: it parses documents with it, reads the tree, and at the end turns documents into Python objects. It needs the `fpeg` package, installed as the [start page](../index.md#install) describes.

## Terminals and rules

A grammar is a list of rules, written as text and loaded at runtime. Start with a document that is a single string or number:

```python
import fpeg

grammar = fpeg.Grammar(r"""
start: STRING | NUMBER
STRING: "\"" [^"]* "\""
NUMBER: "-"? [0-9]+ ("." [0-9]+)?
""")

tree = grammar.parse('"hello"')
print(tree)
```

```output
Tree('start', [Token('STRING', '"hello"')])
```

A name in lowercase is a rule, and a name in uppercase is a terminal. A terminal matches one token: here `"\""` is a literal, `[^"]` a class of every character but a quote, and `?`, `*` and `+` say how often the part before them occurs. `|` is an ordered choice: `STRING` is tried first, and `NUMBER` only when it fails.

`parse` starts at the rule `start` and returns its node, a `Tree`. Its `children` hold what the rule matched, here one `Token`, which is a `str` that knows its kind and its place in the input:

```python
token = tree.children[0]
assert token == '"hello"'
assert (token.type, token.line, token.column) == ("STRING", 1, 1)
```

Input the grammar does not match raises `ParseError`, which says where the parse failed and what the grammar expected there:

```python
try:
    grammar.parse("hello")
except fpeg.ParseError as error:
    print(error)
```

```output
line 1, column 1: expected NUMBER, STRING
```

## Rules that call rules

Objects and arrays hold other values, so the rules call each other. `(member ** ",")` matches members separated by commas, none or many, and `%ignore` names the text skipped between tokens:

```python
grammar = fpeg.Grammar(r"""
start: value
value: object | array | STRING | NUMBER | "true" | "false" | "null"
object: "{" (member ** ",") "}"
member: STRING ":" value
array: "[" (value ** ",") "]"
STRING: "\"" ([^"\\] | "\\" .)* "\""
NUMBER: "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)? ([eE] [+-]? [0-9]+)?
%ignore [ \t\r\n]+
""")

print(grammar.parse('{"tags": ["peg", true]}').pretty())
```

```output
start
  value
    object
      member
        "tags"
        value
          array
            value	"peg"
            value
```

The grammar matches, but the tree is clumsy in two ways. Every value sits in a node `value` of its own, and `true` is gone: a literal such as `"true"` or `","` is a hidden token, which the tree keeps but does not show among the children. The braces and commas should be hidden, and `true` should not.

## Shaping the tree

The grammar shapes the tree, so that code reading it has less to do:

- `?value` makes `value` transparent: when it has exactly one child, the child takes its place.
- `"true" -> true` names the node its alternative makes, so that `true` stays visible as a node.
- `key=STRING` and `value=value` name the parts of a member as fields, which code reads by name instead of by position.

```python
grammar = fpeg.Grammar(r"""
?start: value
?value: object | array | STRING | NUMBER
      | "true" -> true | "false" -> false | "null" -> null
object: "{" (member ** ",") "}"
member: key=STRING ":" value=value
array: "[" (value ** ",") "]"
STRING: "\"" ([^"\\] | "\\" .)* "\""
NUMBER: "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)? ([eE] [+-]? [0-9]+)?
%ignore [ \t\r\n]+
""")

tree = grammar.parse('{"tags": ["peg", true]}')
print(tree.pretty())
```

```output
object
  member
    key="tags"
    value=array
      "peg"
      true
```

A line starting with `|` continues the rule above it. A transparent start rule may leave a single token, so `parse` returns a `Token` for a document that is just a number:

```python
assert grammar.parse("12").type == "NUMBER"
```

## Reading the tree

A node's `data` is the name of its rule or alias, `field()` reads a field, `meta` says where the node spans, and `text` is the input it covers:

```python
member = tree.children[0]
assert member.data == "member"
assert member.field("key") == '"tags"'
assert member.field("value").data == "array"
assert (member.meta.line, member.meta.column, member.meta.end_column) == (1, 2, 23)
assert member.text == '"tags": ["peg", true]'
```

Error messages name the tokens the grammar would have accepted:

```python
try:
    grammar.parse('{"tags": ["peg" true]}')
except fpeg.ParseError as error:
    print(error)
```

```output
line 1, column 17: expected ",", "]"
```

## Values

To get Python objects, a grammar can say what each node and token is worth: `=> builder` after an alternative. Then a plain `fpeg.Transformer()` turns a tree into values without calling any Python code of yours:

```python
grammar = fpeg.Grammar(r"""
?start: value
?value: object | array | STRING | FLOAT | INT
      | "true" => True | "false" => False | "null" => None
object: "{" (member ** ",") "}" => dict
member: key=STRING ":" value=value => tuple
array: "[" (value ** ",") "]" => list
STRING: "\"" ([^"\\] | "\\" .)* "\"" => unquote
INT: "-"? ("0" | [1-9] [0-9]*) => int
FLOAT: "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+ ([eE] [+-]? [0-9]+)? | [eE] [+-]? [0-9]+) => float
%ignore [ \t\r\n]+
""")

text = '{"name": "caf\\u00e9", "tags": ["peg", "rust"], "stars": 5, "share": 0.5, "stable": false}'
value = fpeg.Transformer().transform(grammar.parse(text))
print(value)
```

```output
{'name': 'café', 'tags': ['peg', 'rust'], 'stars': 5, 'share': 0.5, 'stable': False}
```

`dict` builds a dictionary from its children's values, `tuple` and `list` do what their names say, `unquote` removes the quotes of a string and decodes its escapes, `int` and `float` read numbers, and `True`, `False` and `None` are constants. The [grammar reference](../reference/grammar.md#values) lists them all.

Numbers come in two terminals now, so that whole numbers become `int` as in `json.loads`. Their order in `?value` matters: an ordered choice takes the first alternative that matches and never comes back to try another. `INT` first would match the `0` of `0.5`, and the parse would then fail at the `.`. With `FLOAT` first, a whole number fails `FLOAT` and is taken by `INT`. The values now equal those of `json.loads`:

```python
import json

assert value == json.loads(text)
```

## Where next

- The [grammar language](../reference/grammar.md) describes every form a grammar can use, and the [Python API](../reference/python-api.md) everything the package offers.
