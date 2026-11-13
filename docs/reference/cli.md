# Command line

The `fastpeg` command checks grammars and parses inputs with them. It reads grammars and inputs as UTF-8 text.

## fastpeg check

```text
fastpeg check GRAMMAR...
```

Reads, resolves and compiles each grammar file and prints its problems, one per line:

```text
grammar.fpeg:1:8: error[E201]: unknown rule item
```

A line holds the file, the line and the column of the problem, both counted from 1 and the column in code points, then the [diagnostic code](diagnostics/index.md) and the message. A grammar without problems prints nothing. The exit status is 0 when no grammar has a problem and 1 otherwise, so a build can run the command as a check.

## fastpeg parse

```text
fastpeg parse [--start RULE] [--spans] [--json] GRAMMAR [INPUT]
```

Parses the file INPUT, or standard input when INPUT is missing or `-`, from the rule `start` or the rule `--start` names, and prints its [tree](tree.md). With this grammar in `sums.fpeg`

```fpeg
start: first=N ("+" N)*
N: [0-9]+
%ignore " "+
```

the input `1 + 23` gives one item per line, each indented by the nodes it lies in:

```text
start
  first=N"1"
  ~"+"
  N"23"
```

A token shows its kind and its text, and a field its name before the item. `~` marks a hidden token, here the anonymous literal `"+"`, whose kind is the literal itself. With `--spans` every item also shows its byte span, as in `start@0..6` and `first=N"1"@0..1`.

`--json` prints the tree as JSON instead: an array of the top items, where a node has `kind`, `start`, `end` and `children`, a token has `kind`, `start`, `end` and `text`, and either has `field` when it fills one. A hidden token has `"hidden": true`. Spans are byte offsets.

```text
[{"kind":"start","start":0,"end":6,"children":[{"kind":"N","field":"first","start":0,"end":1,"text":"1"},{"kind":"\"+\"","start":2,"end":3,"text":"+","hidden":true},{"kind":"N","start":4,"end":6,"text":"23"}]}]
```

An input that does not parse prints where the parse failed and what the grammar expected there, on standard error:

```text
input.txt:1:4: error: expected N
```

The exit status is 0 when the input parses, 1 when the grammar has problems, which `parse` prints as `check` does, or the input does not parse, and 2 for a command line `fastpeg` cannot understand.
