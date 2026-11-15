# Errors and recovery

A grammar says what a text may be. This guide is about the texts it refuses: what an error says and where it points, how to give the mistakes people make a message of their own, and how a parse goes on past them to give a tree of everything it could read. It builds up a grammar of lists of numbers.

## What a failed parse says

A parse that fails points at the farthest place it reached and lists what the grammar expected there. `render` shows that with the line of the text, as the command line does:

```python
import fpeg

lists = fpeg.Grammar(r"""
start: "[" (NUMBER ("," NUMBER)*)? "]"
NUMBER "number": [0-9]+
%ignore " "+
""")

try:
    lists.parse("[1, 2 3]")
except fpeg.ParseError as error:
    print(error.render("data.txt"))
```

```output
error: expected ",", "]"
 --> data.txt:1:7
  |
1 | [1, 2 3]
  |       ^
```

The farthest place is where most mistakes are, since the parse could not go on from there. A display name such as `"number"` says what a terminal is wherever an error names it, which matters most for terminals whose names say little.

## Naming errors

A label names an error where the grammar has committed. `e ^ name` makes a failure of `e` the error `name`, which says what was expected, and `e ^ "message"` an error with that message. A label stops the parse, so no other alternative is tried after it:

```python
lists = fpeg.Grammar(r"""
start: "[" (NUMBER ("," (NUMBER ^ number))*)? ("]" ^ close)
NUMBER "number": [0-9]+
%ignore " "+
""")

for text in ["[1, 2", "[1, ]"]:
    try:
        lists.parse(text)
    except fpeg.ParseError as error:
        print(error)
```

```output
line 1, column 6: close: expected ",", "]"
line 1, column 5: number: expected number
```

Put a label where the text can only be a mistake: after a comma a number must come, and after the numbers the bracket. A label stays out of the way where the parse got further than the label, which a label before a whole construct does when the construct fails further in: the error points where the parse got furthest, as it would without the label. So a label names the errors that happen where it stands.

## Messages for common mistakes

An error alternative, an alternative that ends in `%error("message")`, gives a mistake that people make a message of its own. It never matches, so it changes nothing the grammar accepts, and a parse that succeeds pays nothing for it. Where a parse fails, the first error alternative in the text whose parts before `%error` match where the parse tried it gives its message, over the text those parts matched:

```python
lists = fpeg.Grammar(r"""
start: list
list: "[" (NUMBER ("," (NUMBER ^ number))*)? ("]" ^ close)
    | "(" NUMBER ("," NUMBER)* ")" %error("a list takes square brackets")
NUMBER "number": [0-9]+
%ignore " "+
""")

try:
    lists.parse("(1, 2)")
except fpeg.ParseError as error:
    print(error.render())
```

```output
error: a list takes square brackets
 --> 1:1
  |
1 | (1, 2)
  | ^^^^^^
```

## Going on past mistakes

`%recover name: e` tells a parse with `recover=True` how to go on where the error `name` occurs: what `e` matches there becomes an `<error>` node in the tree, or a `<missing>` token where it matches nothing, and the parse goes on after it. Here a number that does not come is skipped up to the next comma or bracket, and a bracket that does not come is missing:

```python
lists = fpeg.Grammar(r"""
start: "[" (NUMBER ("," (NUMBER ^ number))*)? ("]" ^ close)
NUMBER "number": [0-9]+
%ignore " "+
%recover number: (!("," | "]") .)*
%recover close: ()
""")

try:
    lists.parse("[1, x, 3", recover=True)
except fpeg.ParseError as error:
    print(error)
    print(error.tree.pretty())
```

```output
line 1, column 5: number: expected number
line 1, column 9: close: expected ",", "]"
start
  1
  <error>	'x'
  3
  <missing>

```

A text with mistakes raises `ParseError` all the same, so that no broken text passes for a valid one. `tree` holds what the parse recovered and `errors` each mistake, as a parse without `recover` would have reported it had it stopped there. A recovery may parse what it skips, as `(NUMBER | !("," | "]") .)*` would keep the numbers in it, and a label inside a recovery recovers again once the recovery has made progress, so that the trees of a broken part stay.

A `Transformer` gives error items to its methods `__error__` and `__missing__`. One that returns `fpeg.Discard` drops them, so the values of a recovered tree are those of what the parse could read:

```python
class Numbers(fpeg.Transformer):
    def start(self, *items):
        return list(items)

    def NUMBER(self, token):
        return int(token)

    def __error__(self, children, *, node):
        return fpeg.Discard

    def __missing__(self, token):
        return fpeg.Discard


try:
    lists.parse("[1, x, 3", recover=True)
except fpeg.ParseError as error:
    print(Numbers().transform(error.tree))
```

```output
[1, 3]
```

## Text after the end

Text left after the grammar's start rule is an error that no label raises. `!.` is the end of the input, so a label on it recovers what is left:

```python
lists = fpeg.Grammar(r"""
start: "[" (NUMBER ("," (NUMBER ^ number))*)? ("]" ^ close) (!. ^ trailing)
NUMBER "number": [0-9]+
%ignore " "+
%recover number: (!("," | "]") .)*
%recover close: ()
%recover trailing: .*
""")

try:
    lists.parse("[1, 2] 3", recover=True)
except fpeg.ParseError as error:
    print(error)
```

```output
line 1, column 8: trailing: expected end of input
```

The grammars that come with fpeg do the same: `json` and `python` name their mistakes and recover from them, and a grammar that extends one takes its recoveries along. `fpeg check` warns about a recovery whose label no expression raises ([W403](../reference/diagnostics/W403.md)), mostly a name written two ways.
