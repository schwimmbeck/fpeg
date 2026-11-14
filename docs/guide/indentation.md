# Indentation

Python, YAML and many configuration formats mark blocks by how far a line is indented instead of by brackets. A grammar says this with two forms: `%indent(e)` opens a block whose items stand right of the enclosing block, and `%same` says that an item starts exactly where its block's items start. This guide builds an outline language with them, then adds Python's rule that brackets turn layout off, continuation lines, and tabs.

## Blocks by columns

An outline has entries with a key and an optional value, and the entries indented under an entry are its children:

```python
import fpeg

OUTLINE = r"""
start: _NL? entries=(%same entry)*
entry: key=KEY ":" value=VALUE? (_NL | !.) children=children?
children: %indent(entries=(%same entry)+)
KEY: [a-z0-9]+
VALUE: [^ \n] [^\n]*
_NL: ("\n" [ \t]*)+
%ignore [ \t]+
"""
outline = fpeg.Grammar(OUTLINE)

text = """\
project: fpeg
milestones:
  m6: context
  m7: modules
license: MIT
"""
print(outline.parse(text).pretty())
```

```output
start
  entries=entry
    key=project
    value=fpeg
  entries=entry
    key=milestones
    children=children
      entries=entry
        key=m6
        value=context
      entries=entry
        key=m7
        value=modules
  entries=entry
    key=license
    value=MIT
```

- Every check compares the column of the next token with the reference column, which is 0 at the start of a parse. Columns count from 0 at the start of the line.
- `%indent(e)` checks that the next token starts right of the reference column. That token's column becomes the reference column inside `e`, so the first child decides where its siblings stand.
- `%same` checks that the next token starts exactly at the reference column, and consumes nothing.
- Ignored text is skipped before a check, as before a token. The line break and the indentation after it are a hidden terminal here, `_NL`, which also takes blank lines, so that they do not count.

## Where a block ends

A block ends where a line returns to a column left of its items, and its parent's next item, or the end of the input, has to match there. A line at a column that no open block uses fits nowhere:

```python
try:
    outline.parse("a:\n    b: 1\n  c: 2\n")
except fpeg.ParseError as error:
    print(error)
```

```output
line 3, column 3: expected end of input, indentation
```

A failed check expects indentation at the position it checked, so an error at a wrong column says so beside what else would have matched there. Columns in messages count from 1.

## Brackets turn layout off

In Python a line break inside brackets is not the end of a statement, and the next line may start at any column. A flag says where the parser is inside brackets, and the skip expression ignores line breaks only while it is set:

```python
blocks = fpeg.Grammar(r"""
%flag bracket
%word [a-z]
%ignore " "+ | %if(bracket) "\n" " "*
start: _NL? statements=(%same statement)*
?statement: if | call _NL
if: "if" test=expr ":" _NL body=block
block: %indent(statements=(%same statement)+)
call: name=NAME "(" %on(bracket, args=(expr ** ",") ")")
?expr: call | NAME | NUMBER
@reserved
NAME: [a-z]+
NUMBER: [0-9]+
_NL: ("\n" " "*)+
""")

text = """\
if ready:
    print(a,
          b)
    log(c)
done(1)
"""
print(blocks.parse(text).pretty())
```

```output
start
  statements=if
    test=ready
    body=block
      statements=call
        name=print
        args=a
        args=b
      statements=call
        name=log
        args=c
  statements=call
    name=done
    args=1
```

The closing `")"` stands inside `%on`, since ignored text is skipped before a token: the line break before `b)` is skipped while the flag is set. The `_NL` after the call is outside, so the line ends the statement. A rule that the brackets call keeps the flag, and so does the skip expression, which is why a nested call needs nothing more.

## Continuation lines

Some languages continue an item on the next line as long as that line is indented further than the item, as Haskell's layout does. The skip expression may check the column too, against the reference column of the token it stands before, so a line break can be ignored where the next line starts right of the reference column:

```python
layout = fpeg.Grammar(r"""
%ignore " "+ | _BREAK %indent(())
start: _BREAK? decls=((%same decl) ++ _BREAK) _BREAK?
decl: name=NAME "=" terms=(NUMBER ++ "+")
NAME: [a-z]+
NUMBER: [0-9]+
_BREAK: ("\n" " "*)+
""")

text = """\
a = 1 + 2
  + 3
b = 4
"""
print(layout.parse(text).pretty())
```

```output
start
  decls=decl
    name=a
    terms=1
    terms=2
    terms=3
  decls=decl
    name=b
    terms=4
```

The `+` on the second line starts right of column 0, so the line break before it is ignored and the sum goes on. `b` starts at column 0, the skip fails there, and `_BREAK` separates the declarations. Inside a block the same skip expression compares with the block's column, so the rule holds at every level.

## Tabs

A tab advances the column to the next multiple of 8, as in Python. `%tabs n` sets another width. With `%tabs 4`, a tab and four spaces stand at the same column:

```python
tabbed = fpeg.Grammar(OUTLINE + "%tabs 4\n")
print(tabbed.parse("a:\n\tb: 1\n    c: 2\n").pretty())

try:
    outline.parse("a:\n\tb: 1\n    c: 2\n")
except fpeg.ParseError as error:
    print(error)
```

```output
start
  entries=entry
    key=a
    children=children
      entries=entry
        key=b
        value=1
      entries=entry
        key=c
        value=2

line 3, column 5: expected end of input, indentation
```

With the default width the tab reaches column 8 and the four spaces column 4, which no block uses. Python also rejects some mixes of tabs and spaces that compare consistently at one width and not at another. A grammar leaves that check to the code that reads the tree.

## Complete languages

The grammars folder of the repository holds three languages built this way: `python.fpeg`, Python 3.15 with its blocks, brackets and continued lines, `outline.fpeg`, the outline above with comments, and `offside.fpeg`, a small functional language whose blocks follow the column of their first item, with braces that turn the layout off.
