# Interpolated strings

An f-string holds expressions between its quotes, a Lua long string ends where brackets with as many signs as at its start close it, and a closing tag of markup has to name the tag it closes. None of them is a terminal: an expression inside a string has a structure of its own, and a delimiter that the input chooses cannot be written in the grammar. This guide shows the two tools for them, lexical rules and captures.

## Code inside a string

A terminal is one token, whatever it matches, and skips nothing inside. A syntactic rule makes a node of its parts and skips ignored text before each of them. A lexical rule, marked `@lexical`, makes a node like a syntactic rule and skips nothing between its parts like a terminal, and the syntactic rules it calls skip as usual. So text keeps its spaces, and an expression inside it is parsed as one:

```python
import fpeg

templates = fpeg.Grammar(r"""
start: template
@lexical
template: "\"" parts=(TEXT | field)* "\""
field: "{" value=sum "}"
?sum: atom ("+" atom)+ -> sum | atom
?atom: NAME | NUMBER | template
TEXT: [^"{}]+
NAME: [a-z]+
NUMBER: [0-9]+
%ignore " "+
""")

print(templates.parse('"Hi {name}, next year {age + 1}: {"{n}"}"').pretty())
```

```output
start
  template
    parts=Hi 
    parts=field	value=name
    parts=, next year 
    parts=field
      value=sum
        age
        1
    parts=: 
    parts=field
      value=template
        parts=field	value=n
```

The texts keep their spaces, since the template skips nothing, and the spaces around `+` are skipped, since the field is a syntactic rule. A template may stand inside a field, quotes and all, as in Python's f-strings since 3.12: the inner quote is the start of a new template, not the end of the outer one, because the field is still open.

## The quotes as parameters

A language with several quotes needs the same rule for each of them, with the closing quote and the text that may not contain it changed. A rule with parameters says it once. Where a kind of string allows more than another, a flag tells the rules inside which kind they are in. Here only triple-quoted strings may hold a line break:

```python
quotes = fpeg.Grammar(r"""
%flag long
start: strings=string+
?string: %on(long, text<"\"\"\""> | text<"'''">) | %off(long, text<"\""> | text<"'">)
@lexical
text<quote>: quote parts=(TEXT<quote> | field)* quote
field: "{" value=NAME "}"
TEXT<quote>: (!quote ([^{}\n\\] | "\\" . | %if(long) "\n"))+
NAME: [a-z]+
%ignore " "+
""")

print(quotes.parse("'a {b}' \"c\" '''d'''").pretty())
quotes.parse("'''a\nb'''")
try:
    quotes.parse("'a\nb'")
except fpeg.ParseError as error:
    print(error)
```

```output
start
  strings=text
    parts=a 
    parts=field	value=b
  strings=text	parts=c
  strings=text	parts=d

line 1, column 3: expected "'", "{", TEXT
```

The triple quotes come first, so that `"""` is not read as an empty string and a quote. A flag holds for everything inside, the rules a field calls included, which is why each kind of string sets it or clears it itself. Python's grammar does the same with five flags: whether the parser is inside brackets, in a field, in a format spec, in a nested one, and in a triple-quoted string.

## Delimiters the input chooses

A Lua long string starts with `[`, any number of `=` and `[`, and ends at the first `]` with as many `=` and `]`. `%def(name, e)` remembers the text that `e` matches, and `%ref(name)` matches exactly that text again:

```python
long = fpeg.Grammar(r"""
start: strings=LONG_STRING*
LONG_STRING: "[" %def(level, "="*) "[" (!("]" %ref(level) "]") .)* "]" %ref(level) "]"
%ignore [ \n]+
""")

print(long.parse("[[a]] [==[ b ]] c ]==]").pretty())
```

```output
start
  strings=[[a]]
  strings=[==[ b ]] c ]==]
```

The second string holds `]]`, which would end a string of level 0 but not one of level 2. Raw strings with a count of `#`, as in Rust, and here-documents that end at a line of their own word, as in shells, are written the same way.

## Captures in syntactic rules

A capture lasts until the rule activation that made it returns, and the rules called from there see it. So in a syntactic rule that calls itself, each activation has its own, and a closing tag matches the opening tag of its own element:

```python
markup = fpeg.Grammar(r"""
start: element
element: "<" %def(tag, NAME) ">" children=(element | TEXT)* "</" %ref(tag) ">"
NAME: [a-z]+
TEXT: [^<]+
""")

print(markup.parse("<a>x<b>y</b>z</a>").pretty())
try:
    markup.parse("<a><b></a></b>")
except fpeg.ParseError as error:
    print(error)
```

```output
start
  element
    a
    children=x
    children=element
      b
      children=y
    children=z

line 1, column 9: expected %ref(tag)
```

The element `b` remembers its own tag while its children are parsed, and when it returns, `</a>` is matched against `a` again. In a syntactic rule the remembered text starts after ignored text, as a token would.

A remembered result is reused only where the flags, the reference column and the visible captures are those it was made with. The [performance guide](performance.md) says what that means for a grammar.
