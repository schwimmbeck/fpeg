# Composing grammars

A grammar does not have to start from nothing. It can start from another grammar and change some of its rules, and it can take single rules from others. This guide extends the JSON grammar that comes with fpeg, takes terminals from `common`, uses a grammar of your own as a module, and extends the Python grammar together with its transformer to `ast`.

## Starting from a grammar

`%extends` starts from all the rules and directives of another grammar. A rule written again replaces the inherited one, and every rule of the other grammar that refers to it gets the new one:

```python
import fpeg

jsonc = fpeg.Grammar(r"""
%extends "json"
object: "{" ((member ++ ",") ","?)? "}" => dict
array: "[" ((value ++ ",") ","?)? "]" => list
%ignore "//" [^\r\n]* | "/*" (!"*/" .)* "*/"
""")

print(fpeg.Transformer().transform(jsonc.parse('{"a": [1, 2,], /* two */ "b": null,}')))
```

```output
{'a': [1, 2], 'b': None}
```

Objects and arrays take a comma at their end, and the comments are skipped after the spaces that the JSON grammar skips. The builders `=> dict` and `=> list` come with the new rules, and every other rule keeps its own, so the values are those of the JSON grammar.

## Keeping what a rule had

`...` in a rule that replaces another stands for the alternatives it replaces, as a whole alternative of its own. Where it stands decides which alternatives are tried first:

```python
for value in ("NAME | ...", "... | NAME"):
    names = fpeg.Grammar(f'%extends "json"\n?value: {value}\nNAME: [a-z]+')
    print(fpeg.Transformer().transform(names.parse("[true, maybe]")))
```

```output
[Token('NAME', 'true'), Token('NAME', 'maybe')]
[True, Token('NAME', 'maybe')]
```

With the new alternative first, `true` is a name. With the inherited ones first, it is JSON's `true` with its builder, and only `maybe` is a name.

The labels of an inherited rule commit where they stand: after `{` JSON's `object` takes the text, and a `}` that does not come is an error, not a reason to try the next alternative. So where a new alternative starts as an inherited one does, as with `{`, it goes before `...`.

## Taking single rules

`%import` takes rules by name. The rules they need come along under the name of their grammar and a dot, so that they never clash with the rules of the grammar that imports them:

```python
sums = fpeg.Grammar(r"""
%import common (NUMBER, WS)
start: NUMBER ("+" NUMBER)*
%ignore WS
""")

print(sums.parse("1 + 2.5").pretty())
```

```output
start
  1
  2.5
```

`common` holds terminals that many grammars need, which the [grammar reference](../reference/grammar.md#modules) lists. An import leaves the other directives of its grammar behind: the grammar that imports says what it skips, here `WS`.

## Grammars of your own

A grammar takes the modules it extends and imports from those given with it, a mapping of names to texts, before the grammars that come with fpeg. `Grammar.open` also takes the files beside the grammar's own file, `m.fpeg` for the module `m`, and so does the command line.

```python
words = 'start: word ("," word)*\nword: WORD\nWORD: [a-z]+\n%ignore " "+'
grammar = fpeg.Grammar(
    '%extends "words"\nword: ... | NUMBER\nNUMBER: [0-9]+', modules={"words": words}
)
print(grammar.parse("one, 2").pretty())
```

```output
start
  word	one
  word	2
```

A problem in a module is reported at the directive that takes the module in, with its place in the module:

```python
try:
    fpeg.Grammar("%import broken (start)", modules={"broken": "start: missing"})
except fpeg.GrammarError as error:
    print(error.diagnostics[0])
```

```output
<grammar>:1:9: error[E201]: in the grammar broken, line 1, column 8: unknown rule missing
```

## Extending Python

The Python grammar comes with fpeg, and so does its transformer to Python's own `ast`, as `fpeg.python`, which builds the `ast` of Python 3.15 and needs that version. A language that adds to Python extends both. Here a statement `swap a, b` exchanges two names:

```python
import ast

from fpeg import python

swapping = fpeg.Grammar(r"""
%extends "python"
%keywords soft "swap"
?simple: swap | ...
swap: "swap" left=NAME "," right=NAME
""")


class ToAst(python.ToAst):
    def swap(self, left, right):
        def stored(name):
            return ast.Name(name.id, ast.Store())

        targets = ast.Tuple([stored(left), stored(right)], ast.Store())
        return ast.Assign([targets], ast.Tuple([right, left], ast.Load()))


module = ToAst().transform(swapping.parse("a, b = 1, 2\nswap a, b\nprint(a, b)\n"))
exec(compile(ast.fix_missing_locations(module), "<swap>", "exec"))
```

```output
2 1
```

`swap` stays a name everywhere else, since it is a soft keyword, and the new statement is tried before Python's own. The transformer stamps each value a method returns with its position, as `ast` counts it. The nodes that a method builds inside that value get theirs from `ast.fix_missing_locations`, which copies the position of the node around them, so that the compiled code reports errors at the line of the statement.

The [case study](a-language-over-python.md) extends Python in the same way with a whole language.
