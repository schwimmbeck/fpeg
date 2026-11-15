# A configuration file

A configuration format with sections, comments and typed values, read into dictionaries. Builders in the grammar make the values, and a `Transformer` puts the entries of each section into a dictionary, refusing a key that is set twice.

```python
import fpeg

grammar = fpeg.Grammar(r"""
start: _EOL? section*
section: "[" name=NAME "]" _EOL entries=entry*
entry: key=NAME "=" value=value _EOL
?value: STRING | NUMBER | "true" => True | "false" => False | list
list: "[" (value ** ",") "]" => list

NAME: [a-z_]+
STRING: "\"" ([^"\\\n] | "\\" .)* "\"" => unquote
NUMBER: [0-9]+ => int
_EOL "line break": "\n"+ | !.
%ignore [ \t]+ | "#" [^\n]*
""")


class ToConfig(fpeg.Transformer):
    def start(self, *sections):
        return dict(sections)

    def section(self, name, entries):
        values = {}
        for key, value in entries:
            if key in values:
                raise ValueError(f"line {key.line}: {key} is set twice in [{name}]")
            values[str(key)] = value
        return str(name), values

    def entry(self, key, value):
        return key, value


TEXT = """\
# Where the server listens
[server]
host = "example.org"
port = 8080

[storage]
paths = ["/var/data", "/mnt/backup"]
compress = true
"""

config = ToConfig().transform(grammar.parse(TEXT))
for name, values in config.items():
    print(name, values)
```

```output
server {'host': 'example.org', 'port': 8080}
storage {'paths': ['/var/data', '/mnt/backup'], 'compress': True}
```

## How it works

- `_EOL` matches one or more line breaks, or the end of the text, which `!.` stands for, so the last line needs no line break of its own. A terminal whose name starts with `_` makes hidden tokens, which stay out of the children of their node, and `"line break"` is the name that errors give it.
- `%ignore` skips spaces, tabs and comments before every token. A line break is a token, so a comment can stand on a line of its own or at the end of an entry.
- `?value` gives way to the one value it holds, and `"true" => True` and `"false" => False` give the constants.
- The builders make the values: `unquote` removes the quotes of a string and decodes the escapes of JSON, `int` reads a number, and `list` collects the values of a list.
- `entries=entry*` names a field that can hold several items, so `section` gets the values of its entries as a list.
- A key is a token: a `str` that knows its line, which the error of a key set twice reports.

## Errors

A text that does not follow the format raises `ParseError` with its line and column. The error of a key set twice comes from `section`, and keeps its type:

```python
for text in ["[server]\nhost = example.org\n", "[server]\nport = 80\nport = 81\n"]:
    try:
        ToConfig().transform(grammar.parse(text))
    except (fpeg.ParseError, ValueError) as error:
        print(error)
```

```output
line 2, column 8: expected "[", "false", "true", NUMBER, STRING
line 3: port is set twice in [server]
```
