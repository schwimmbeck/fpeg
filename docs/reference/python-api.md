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

`Grammar(text)` compiles a grammar given as text, and `Grammar.open(path)` reads one from a file. A grammar with problems raises `GrammarError`, which holds each problem as a `Diagnostic` with its [code](diagnostics/index.md) and its place. Both pickle, so that a grammar error can pass to another process.

```python
try:
    fpeg.Grammar("start: pair")
except fpeg.GrammarError as error:
    (problem,) = error.diagnostics
    assert (problem.code, problem.line, problem.column) == ("E201", 1, 8)
    assert str(problem) == "<grammar>:1:8: error[E201]: unknown rule pair"
```

`check(text)` returns diagnostics as a list and raises nothing. For a text that does not read or resolve these are its errors. Otherwise they are the warnings, forms that work and are likely not meant, which `Grammar` takes without a word, and beside them an error where the grammar is too large to compile. The functions that value builders call are not given to `check`, so a missing one, which `Grammar` reports, is not among them. The `severity` of a diagnostic tells errors and warnings apart.

```python
(found,) = fpeg.check('start: "a"? | "b"')
assert (found.severity, found.code) == ("warning", "W101")
print(found)
```

```output
<grammar>:1:15: warning[W101]: this alternative is never tried, since the one before it cannot fail
```

A grammar that extends or imports [modules](grammar.md#modules) takes them from `modules`, a mapping of their names to their texts, then, for `Grammar.open`, from the files beside its own, `m.fpeg` for the module `m`, then from the grammars that come with fpeg. A file beside that cannot be read is an error. `check` takes `modules` too.

```python
base = 'start: item+\nitem: WORD\nWORD: [a-z]+\n%ignore " "+'
words = fpeg.Grammar('%extends "base"\nitem: ... | N\nN: [0-9]+', modules={"base": base})
assert words.parse("ab 12").children[1] == fpeg.Tree("item", [fpeg.Token("N", "12")])
```

`parse` returns the one visible item at the top of the tree: a `Tree` for a node, or a `Token` for a token, which a transparent start rule gives when its one child is a token. A start rule that may leave no visible item or several, such as a rule `_name`, also where a recovery may leave an error item beside its item, is refused before parsing, as an unknown one is.

```python
numbers = fpeg.Grammar('?start: NUMBER | list\nlist: "[" (start ** ",") "]"\nNUMBER: [0-9]+')
assert isinstance(numbers.parse("7"), fpeg.Token)
assert numbers.parse("[7,8]").data == "list"
```

A grammar is immutable, so one grammar serves any number of parses from any thread. A parse of a longer input lets other Python threads run while it works.

```{eval-rst}
.. autoclass:: fpeg.Grammar
   :members:

.. autofunction:: fpeg.check
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

`Tree(data, children, meta=None)` builds a tree in Python. The children of a tree from a parse are a list that refuses changes, since a transform reads the parse itself: to change a tree, build one, with `copy()` or `Tree`. Trees compare equal when their data and children are, hash and pickle. Every walk over a tree runs without recursion, so trees of any depth print, compare and pickle.

```{eval-rst}
.. autoclass:: fpeg.Tree
   :members:

.. autoclass:: fpeg.Meta
   :members:
```

## Token

A `Token` is a `str` with the name of its kind in `type` and its place in these attributes: `start_pos` and `end_pos` count code points, `line` and `column` count from 1, and `end_column` is the column after the last character. Tokens compare and hash as their text. Their attributes are read-only, and `update()` makes a token with another type or text at the same place.

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

A `Transformer` turns a tree into values. A method named like a rule gives the value of its nodes, a method named like a terminal the value of its tokens, and each is called after the children of its node, with their values. A rule named like a Python keyword, such as `if`, or like a method the class needs itself, such as `transform`, has its method under the name with an underscore added, `if_`, as PEP 8 writes such names. A rule that an import brings in for the rules it takes, such as `common.DIGIT`, has its method under the dot written as two underscores, `common__DIGIT`. What a method gets, its signature says:

| Method | Gets |
|---|---|
| `def pair(self, key, value)`, every parameter a field | the fields by name: the value, or `None` when absent, for a field of at most one item, a list for a field that can hold several |
| `def pair(self, *children)` | the visible children as arguments |
| `def pair(self, children)`, a parameter that is no field | the list of the visible children |
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

An exception in a method keeps its type and gets a note with the method and where its node stands, which the traceback shows. A wrong signature fails at the first transform with the fields of the node. The methods of a class and their signatures are read at its first transform of a tree from a grammar and then kept: a method added to the class later is not called on that grammar's trees, and a signature changed later is not read again. `class T(fpeg.Transformer, strict=True)` also makes every public method of `T` name a rule or terminal, so a misspelled method fails instead of never being called.

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

`class T(fpeg.Transformer, positions="ast")` gives the values made for the nodes and tokens of a parse's tree their positions: a value that is an `ast.AST` without a `lineno` gets `lineno`, `col_offset`, `end_lineno` and `end_col_offset`, lines from 1 and columns from 0 in UTF-8 bytes, as `ast` counts them. So do each value a method yields and each item of a list it returns, as statements that one statement of a language turns into. A value passed up from a child keeps its own, as `(a)` keeps the position of `a` in Python. The nodes a method builds inside its value get no position, and `ast.fix_missing_locations` gives them the position of the node around them. `fpeg.Positions(start_line, start_column, end_line, end_column, types, line_base=1, column_base=0, column_unit="code points")` names other attributes, the classes whose instances get them, and how lines and columns count, in `"code points"`, `"utf-8"` bytes or `"utf-16"` units. A subclass keeps the positions of its base. A tree built in Python has no positions to give, and neither has a token transformed on its own, as `parse` returns one where the start rule leaves a single token, since a token holds no text of its line to count its columns in.

```python
import ast

sums = fpeg.Grammar(r"""
start: left=NAME "+" right=NAME
NAME: [a-zé]+
%ignore " "+
""")


class ToAst(fpeg.Transformer, positions="ast"):
    def start(self, left, right):
        return ast.BinOp(left, ast.Add(), right)

    def NAME(self, token):
        return ast.Name(str(token), ast.Load())


total = ToAst().transform(sums.parse("é + b"))
assert (total.col_offset, total.end_col_offset) == (0, 6)
assert (total.left.col_offset, total.left.end_col_offset) == (0, 2)
```

`grammar.transformer_template(name)` writes a starting transformer for a grammar: a method for every rule that makes nodes and every terminal, the parameters of a node's method named after its fields with type hints from how many items each holds, the builders a method replaces, and `strict=True`. A transparent rule `?name` whose every match leaves one visible item, also where a recovery may add an error item, never makes a node, so it gets no method.

```python
template = grammar.transformer_template("Pairs")
assert "    def pair(self, key: Any, value: Any) -> Any:" in template
assert "        key holds one item." in template
```

`fpeg.v_args` says by decorator how methods are called, on a method or a whole class: `inline=True` passes the children as arguments, `meta=True` the position first, `tree=True` a tree of the transformed children. A tree built in Python transforms too, by its `data`, without fields.

```{eval-rst}
.. autoclass:: fpeg.Transformer
   :members: transform, __default__, __default_token__

.. autoclass:: fpeg.Positions

.. autofunction:: fpeg.v_args
```

## Interpreter

An `Interpreter` walks a tree from the top. `visit(node)` calls the method named like the node, and the method decides what to visit of it and when, so a branch that is not taken is never looked at. That is what running a program needs, where a `Transformer` has computed every child before a method sees it.

A method gets the parts of its node as parsed, not yet visited, and visits one with `self.visit(part)`. Methods are named as for a `Transformer`, `if_` for a rule `if`, and the signature says what a method gets:

| Method | Gets |
|---|---|
| `def if_(self, test, then)`, every parameter a field | the fields by name: the node or token, or `None` when absent, for a field of at most one item, a list for a field that can hold several |
| `def block(self, *children)` | the visible children as arguments |
| `def block(self, node)`, a parameter that is no field | the node |
| `def block(self)` | nothing |
| `def NUMBER(self, token)` | the token |
| `meta`, `node` or `children` as keyword-only parameters | beside the rest: where the node spans, the node, the list of its children |

```python
program = fpeg.Grammar(r"""
start: statement*
?statement: assign | if
assign: target=NAME "=" value=expr ";"
if: "if" test=expr "{" then=statement* "}"
?expr: name=NAME -> load | NUMBER
NAME: [a-z]+
NUMBER: [0-9]+ => int
%ignore " "+
%word [a-z]
""")


class Run(fpeg.Interpreter):
    def __init__(self):
        self.variables = {}

    def assign(self, target, value):
        self.variables[str(target)] = self.visit(value)

    def if_(self, test, then):
        if self.visit(test):
            for statement in then:
                self.visit(statement)

    def load(self, name):
        return self.variables[str(name)]


run = Run()
run.visit(program.parse("x = 1; if x { y = 2; } if 0 { y = missing; }"))
assert run.variables == {"x": 1, "y": 2}
```

A node without a method gives `visit_children(node)`, the list of its visited children, which `start` does above. Override `__default__(self, node)` to give something else. Visiting a token gives what the method named like its terminal returns, else the value of the terminal's builder, as `int` for `NUMBER` above, else the token. The builders of rules are not applied, since a method decides when the children are visited. A tree built in Python is walked alike, by the names of its `data`. It has no fields, so its methods take the node or `*children`.

`visit(None)` is an error, and so is visiting a list: an optional field that is absent is `None`, and a field of several items is a list, so test the one and loop over the other. An exception keeps its type and gets one note, from the innermost method, with where its node stands. `class I(fpeg.Interpreter, strict=True)` makes every public method name a rule or terminal, as for a `Transformer`, and `v_args` is not for an `Interpreter`.

The walk recurses in Python, a few calls deep for every level of the tree, so at Python's default recursion limit it goes about 330 levels down. A tree nested deeper ends it with `RecursionError`, where a `Transformer` does not recurse and takes a tree of any depth. On Python 3.11 a recursion limit raised far above its default can let a deep tree overflow the stack and end the process instead, as Python's documentation warns for any deep recursion.

```{eval-rst}
.. autoclass:: fpeg.Interpreter
   :members: visit, visit_children, __default__
```

## Visitor

A `Visitor` calls a method for every node of a tree and gives the tree back as it is. It is for looking at a tree: collecting from it, counting, checking. `visit(tree)` calls the methods children before their parents, `visit_topdown(tree)` parents first. A method gets the parts of its node by its signature, as a method of an `Interpreter` does, and what it returns is dropped.

```python
class Names(fpeg.Visitor):
    def __init__(self):
        self.read = set()
        self.written = set()

    def load(self, name):
        self.read.add(str(name))

    def assign(self, target, value):
        self.written.add(str(target))


names = Names()
tree = program.parse("x = 1; if x { y = z; }")
assert names.visit(tree) is tree
assert names.read - names.written == {"z"}
```

Tokens have no methods in a `Visitor`, and a method named like a terminal is an error. Notes on exceptions, `strict=True` and the refusal of `None` and of a list are as for an `Interpreter`. The walk does not recurse, so a tree may nest as deeply as it likes.

```{eval-rst}
.. autoclass:: fpeg.Visitor
   :members: visit, visit_topdown
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

A token on its own, as `parse` returns it for a transparent start rule over a terminal, finds the builder of its terminal through its grammar. A tree keeps its grammar alive and a token does not, so keep the grammar while such a token is transformed.

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

A parse that does not match raises `ParseError`, a `ValueError`, with the place of the failure and what the grammar expected there. `label` holds the name of a [named error](grammar.md#errors), and `None` for others. A parse that reaches a limit raises its subclass `LimitError`. `max_steps`, `max_depth` and `max_memory` lower the limits for a parse, whose defaults suit untrusted input. `max_memory` is in bytes, and its default grows with the text: 4 KiB for each byte, at least 64 MiB and at most 4 GiB.

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

A parse with `recover=True` goes on past an error where the grammar says how: where a label that `%recover` gives a recovery fails, what the recovery matches becomes an `<error>` node, or a `<missing>` token where it matches nothing, and the parse goes on. A valid text gives its tree as without `recover`. A text with errors raises `ParseError` all the same, so that no broken text passes for a valid one. `error.tree` holds the recovered tree, `None` where the parse failed even so, and `error.errors` every error in the order of the text, each a `ParseError` of its own: `pos` is where the parse had got furthest when the label failed, which the message explains, as a parse without `recover` would have, and `end_pos` where what the recovery took ends, or `pos` where that comes first. The raised error describes the first, or the error that ended a parse that failed even so, and says each on a line of its own, the first ten and then how many more, since one mistake can give an error for most lines after it.

```python
numbers = fpeg.Grammar(r"""
start: "[" (item ("," (item ^ item))*)? "]" ^ close
item: NUMBER
NUMBER: [0-9]+
%ignore " "+
%recover item: (!("," | "]") .)*
%recover close: ()
""")

try:
    numbers.parse("[1, x, 3", recover=True)
except fpeg.ParseError as error:
    for problem in error.errors:
        print(problem)
    recovered = error.tree
print(recovered.pretty())
```

```output
line 1, column 5: item: expected NUMBER
line 1, column 9: close: expected ",", "]"
start
  item	1
  <error>	'x'
  item	3
  <missing>

```

`render` shows the errors as the command line does, each with the line of the text it stands in and carets under its span, and names the text by the path given:

```python
try:
    numbers.parse("[1, x, 3", recover=True)
except fpeg.ParseError as error:
    print(error.render("numbers.txt"))
```

```output
error[item]: expected NUMBER
 --> numbers.txt:1:5
  |
1 | [1, x, 3
  |     ^

error[close]: expected ",", "]"
 --> numbers.txt:1:9
  |
1 | [1, x, 3
  |         ^
```

A grammar writes the messages and display names that errors show, so `render` and `str(error)` show control and bidirectional characters as visible marks, where `message` keeps them as written. A terminal acts on none of them, even for a grammar from elsewhere.

In the recovered tree an `<error>` node holds what the recovery took, which its `text` gives, and a `<missing>` token stands where the recovery took nothing. A `Transformer` gives them to its methods `__error__` and `__missing__`, which take what the methods of nodes and tokens take, and keeps them as they are where a class defines neither. A field inside the failed expression is `None`, but where the failed expression is a field `name=e`, the error item takes the field `name`.

```python
class Numbers(fpeg.Transformer):
    def start(self, children):
        return children

    def item(self, *children):
        return int(children[0])

    def __error__(self, children, *, node):
        return f"skipped {node.text!r}"

    def __missing__(self, token):
        return "missing"


print(Numbers().transform(recovered))
```

```output
[1, "skipped 'x'", 3, 'missing']
```

```{eval-rst}
.. autoexception:: fpeg.ParseError

.. autoexception:: fpeg.LimitError

.. autoexception:: fpeg.GrammarError

.. autoclass:: fpeg.Diagnostic
   :members:
```

## The Python grammar

`fpeg.python` turns Python source into Python's own `ast`, with the Python grammar that comes with fpeg, and gives what `ast.parse` gives, positions included. The tests check it against CPython over the whole standard library. It builds the `ast` of Python 3.15 and needs that version.

```python
import ast

from fpeg import python

module = python.parse("total = price * (1 + rate)\n")
print(ast.dump(module.body[0].value))
```

```output
BinOp(left=Name(id='price'), op=Mult(), right=BinOp(left=Constant(value=1), op=Add(), right=Name(id='rate')))
```

`parse` takes the limits of `Grammar.parse`. The default depth takes the deepest nesting that CPython takes, 200 brackets or 149 f-strings, and `max_depth` allows more. It reads what the Python grammar reads, which takes a few forms that CPython refuses only after parsing, such as an assignment to a call. Where the grammar refuses a text it raises `ParseError`, and `SyntaxError` for an escape that Python cannot decode.

The grammar names its mistakes where a failure can only be one: `colon` for the colon of a compound statement, `newline` for the end of a simple statement, `indent` for a block that does not come, `statement` for a line that is no statement, and `indented` for a line indented further than its block. A parse with `recover=True` goes on past each, so that a file with mistakes gives a tree that keeps the statements around them:

```python
source = "def f(x)\n    return x\n\ny = = 2\nz = 3\n"
try:
    python.GRAMMAR.parse(source, recover=True)
except fpeg.ParseError as error:
    print([(problem.label, problem.line) for problem in error.errors])
    print([child.data for child in error.tree.children])
```

```output
[('colon', 1), ('newline', 4)]
['function_def', 'simple_statements', 'simple_statements']
```

`python.GRAMMAR` is the grammar and `python.ToAst` the transformer behind `parse`, with a method for each kind of node and positions stamped as `ast` counts them. A language that adds to Python extends both, a grammar that starts with `%extends "python"` and a subclass of `ToAst` with methods for its new rules, as the [guide to composing grammars](../guide/composing-grammars.md#extending-python) shows. A method may give a list of statements where one statement stands. `python.ToAstBase` holds the same methods without stamping, for a subclass that sets positions its own way.

```{eval-rst}
.. autofunction:: fpeg.python.parse

.. autoclass:: fpeg.python.ToAst
```
