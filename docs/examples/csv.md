# CSV

Comma-separated values with a header line, read into one dictionary per row, as Python's `csv.DictReader` reads them. Quoted fields may hold commas, line breaks and quotes written twice.

```python
import fpeg

grammar = fpeg.Grammar(r"""
start: header=record (_EOL rows=record)* _EOL?
record: &[^\r\n] (field ++ ",") => list
field: value=(QUOTED | BARE)?

QUOTED: "\"" ([^"] | "\"\"")* "\""
BARE: [^,"\r\n]+ => text
_EOL "line break": "\r"? "\n"
""")


class ToRows(fpeg.Transformer):
    def start(self, header, rows):
        return [dict(zip(header, row, strict=True)) for row in rows]

    def field(self, value):
        return "" if value is None else value

    def QUOTED(self, token):
        return token[1:-1].replace('""', '"')


TEXT = """\
name,city,note
Ada,London,"wrote ""notes"", 1843"
Grace,"New York",
"""

rows = ToRows().transform(grammar.parse(TEXT))
for row in rows:
    print(row)
```

```output
{'name': 'Ada', 'city': 'London', 'note': 'wrote "notes", 1843'}
{'name': 'Grace', 'city': 'New York', 'note': ''}
```

The rows equal those of the `csv` module:

```python
import csv
import io

assert rows == list(csv.DictReader(io.StringIO(TEXT, newline="")))
```

## How it works

- The grammar has no `%ignore`, since spaces belong to the fields in CSV. Every character of the text is matched by a terminal.
- `field ++ ","` matches one or more fields separated by commas. A field may be empty, as the last one of Grace's row, so `value=` is optional, and `field` gets `None` for an empty field.
- `&[^\r\n]` tests that a record starts with something other than a line break, without matching it. So a record is never empty, and the line break at the end of the text is not read as a last row with one empty field.
- `header=` and `rows=` name the parts of the file, so `start` gets the header and the list of rows by name. `strict=True` makes a row with another number of fields than the header an error.
- The builders do the simple parts: `=> list` makes the list of a record's fields and `=> text` the string of a bare field. A quoted field needs a method, which removes the quotes and turns each `""` back into one quote.
- `_EOL` takes a line break in either convention. Its name starts with `_`, so its tokens stay out of the children, and `"line break"` is the name that errors give it.
