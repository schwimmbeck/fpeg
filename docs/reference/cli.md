# Command line

The `fpeg` command checks grammars, parses inputs with them and shows where a parse spends its work. It reads grammars and inputs as UTF-8 text.

## fpeg check

```text
fpeg check [--strict] GRAMMAR...
```

Reads, resolves and compiles each grammar file and prints what it finds, one finding per line:

```text
grammar.fpeg:1:8: error[E201]: unknown rule item
other.fpeg:3:14: warning[W101]: this alternative is never tried, since the one before it cannot fail
```

A line holds the file, the line and the column of the finding, both counted from 1 and the column in code points, then `error` or `warning` with the [diagnostic code](diagnostics/index.md), and the message. An error keeps the grammar from being used. A warning is about a grammar that works, and likely not as meant. It is given once the grammar has no error left. A grammar without findings prints nothing.

The exit status is 0 when no grammar has an error and 1 otherwise, so a build can run the command as a check. With `--strict` a warning fails the command as an error does.

## fpeg parse

```text
fpeg parse [--start RULE] [--spans] [--json] GRAMMAR [INPUT]
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

The exit status is 0 when the input parses, 1 when the grammar has problems, which `parse` prints as `check` does, or the input does not parse, and 2 for a command line `fpeg` cannot understand.

## fpeg profile

```text
fpeg profile [--start RULE] [--json] GRAMMAR INPUT...
```

Parses the input files and prints what each rule did, summed over the inputs, the rules that took the most steps first. With this grammar in `again.fpeg`

```fpeg
start: item "." | item "!" | item
item: "(" start ")" | "x"
```

the input `((((x))))` gives:

```text
rule     calls  failed  steps  share  with callees  given back  repeats  hits  stored  grown
item       363       0   4473  71.1%          6273           0      358     0       0      0
start      121       0   1815  28.9%          6288           0      116     0       0      0
(parse)      0       0      2   0.0%             0           0        0     0       0      0

6290 steps, at most 27 entries on the stack and 10 nodes, 0 results remembered
@memo on item: 358 of its 363 calls repeat an earlier one, about 98% of all steps
@memo on start: 116 of its 121 calls repeat an earlier one, about 95% of all steps
```

A step is one instruction of the parser, the unit its step limit counts. The numbers of steps change from one version of fpeg to the next, their proportions are what to read.

| Column | Meaning |
|---|---|
| `calls` | how often the rule was called, also where a remembered result or a left recursion answered |
| `failed` | calls that ran the rule and did not match |
| `steps`, `share` | steps spent in the rule itself, without the rules it calls, and their share of all steps |
| `with callees` | steps from the calls of the rule to their ends, the rules it calls included |
| `given back` | bytes that failed calls had got through before they failed, to the farthest point one of their tries failed at |
| `repeats` | calls at a position where the rule was called before: what remembering the rule answers |
| `hits`, `stored` | calls that a remembered result answered, and results that were remembered |
| `grown` | passes of a left recursion that made its match longer |

- The row `%ignore` holds the steps spent skipping ignored text, and `(parse)` those of starting and ending the parse. The rows sum to the steps of the parse.
- A rule with parameters has one row for all its uses.
- Below the table stand the steps of all inputs and the largest stack, node count and number of remembered results that one input needed.
- A line `@memo on` names a rule that is not remembered and whose repeated calls cost a twentieth of all steps or more. Marking the rule [`@memo`](grammar.md#remembered-results) and profiling again shows what it brings.
- `--json` prints the same counts as JSON: the totals `steps`, `peak_stack`, `peak_nodes` and `memo_entries`, and `rows` with `name`, `remembered`, `calls`, `failures`, `steps`, `steps_with_callees`, `backtracked_bytes`, `repeats`, `memo_hits`, `memo_stores` and `growths` for every rule that was reached.

An input that does not parse is reported as `parse` reports it and still counted. The exit status is 1 then, and 0 when every input parses.
