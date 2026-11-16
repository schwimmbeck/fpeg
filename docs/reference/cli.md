# Command line

The `fpeg` command checks grammars, parses inputs with them and shows where a parse spends its work. It reads grammars and inputs as UTF-8 text. A grammar that extends or imports a module `m` takes it from the file `m.fpeg` beside the grammar's file where there is one, otherwise from the grammars that come with fpeg ([modules](grammar.md#modules)). Such a file that cannot be read, as one that is no UTF-8, is an error.

## fpeg check

```text
fpeg check [--strict] GRAMMAR...
```

Reads, resolves and compiles each grammar file and prints what it finds, one finding per line:

```text
grammar.fpeg:1:8: error[E201]: unknown rule item
other.fpeg:3:14: warning[W101]: this alternative is never tried, since the one before it cannot fail
```

A line holds the file, the line and the column of the finding, both counted from 1 and the column in code points, then `error` or `warning` with the [diagnostic code](diagnostics/index.md), and the message. An error keeps the grammar from being used. A warning is about a grammar that works, and likely not as meant. Warnings are given once the grammar reads and resolves. A grammar without findings prints nothing.

The exit status is 0 when no grammar has an error, 1 otherwise, and 2 for a command line `fpeg` cannot understand, so a build can run the command as a check. With `--strict` a warning fails the command as an error does.

## fpeg parse

```text
fpeg parse [--start RULE] [--spans] [--json] [--recover] GRAMMAR [INPUT]
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

`--json` prints the tree as JSON instead: an array of the top items, where a node has `kind`, `start`, `end` and `children`, a token has `kind`, `start`, `end` and `text`, and either has `field` when it fills one. A hidden token has `"hidden": true`. Spans are byte offsets. Controls and the characters that change the direction of text are escaped, so that printing the JSON cannot rewrite the terminal.

```text
[{"kind":"start","start":0,"end":6,"children":[{"kind":"N","field":"first","start":0,"end":1,"text":"1"},{"kind":"\"+\"","start":2,"end":3,"text":"+","hidden":true},{"kind":"N","start":4,"end":6,"text":"23"}]}]
```

An input that does not parse prints its error on standard error: what the grammar expected, the file, line and column where the parse failed, both counted from 1 and the column in code points, and the line with a caret under the place, followed by a blank line.

```text
error: expected N
 --> input.txt:1:4
  |
1 | 1 +
  |    ^
```

A [named error](grammar.md#errors) shows its name in brackets, as `error[close]: expected "]"`. Tabs show as spaces, control characters as visible marks, so that no input can rewrite the terminal, and a line too long to show is cut around the error.

With `--recover` the parse goes on past an error whose label the grammar gives a recovery. The tree printed holds an `<error>` node for what each recovery took and a `<missing>` token where it took nothing, and each error follows on standard error where the parse had got furthest when its label failed, with carets to the end of what its recovery took. An input with errors fails the command all the same. A parse that fails even so prints the errors it recovered from before the one that ended it.

The exit status is 0 when the input parses, 1 when the grammar has problems, which `parse` prints as `check` does, or the input has errors, and 2 for a command line `fpeg` cannot understand.

## fpeg profile

```text
fpeg profile [--start RULE] [--json] GRAMMAR INPUT...
```

Parses the input files and prints what each rule did, summed over the inputs, the rules that took the most steps first. With `sums.fpeg` from above, the input `1 + 23` gives:

```text
rule     calls  failed  steps  share  with callees  given back  repeats  hits  stored  grown
%ignore      0       0     40  49.4%             0           0        0     0       0      0
start        1       0     26  32.1%            72           0        0     0       0      0
N            2       0     12  14.8%            12           0        0     0       0      0
(parse)      0       0      3   3.7%             0           0        0     0       0      0

81 steps, at most 3 entries on the stack and 1 node, 0 results remembered
```

The [guide to performance](../guide/performance.md) reads a profile step by step, with what the lines `@memo on` below the totals suggest. A step is one instruction of the parser, the unit its step limit counts. The profile runs the parser without its optimizations, but for left recursion run as a loop, so that every rule keeps the calls, failures and steps of the grammar as written, and a parse outside a profile takes fewer steps on most grammars. The numbers of steps change from one version of fpeg to the next, their proportions are what to read.

| Column | Meaning |
|---|---|
| `calls` | how often the rule was called, also where a remembered result or a left recursion answered |
| `failed` | calls that ran the rule and did not match |
| `steps`, `share` | steps spent in the rule itself, without the rules it calls, and their share of all steps |
| `with callees` | steps from the calls of the rule to their ends, the rules it calls included |
| `given back` | bytes that failed calls had got through before they failed, to the farthest point one of their tries failed at |
| `repeats` | calls at a position where the rule was called before: what remembering the rule answers, but for the passes that grow a left recursion, which call the rules of its cycle again by design |
| `hits`, `stored` | calls that a remembered result answered, and results that were remembered |
| `grown` | passes of a left recursion that made its match longer |

- The row `%ignore` holds the steps spent skipping ignored text, and `(parse)` those of starting and ending the parse. The rows sum to the steps of the parse.
- A rule with parameters has one row for all its uses.
- Below the table stand the steps of all inputs and the largest stack, node count and number of remembered results that one input needed. A further line counts the parses that ran out of their budget of steps and started again with every rule remembered, whose counts hold both runs.
- A line `@memo on` names a rule that is not remembered and whose repeated calls cost a twentieth of all steps or more: the steps they took, the rules they called included. Calls made while skipping ignored text count none, since a parse outside the profile does not skip again from where the last skip started. Marking the rule [`@memo`](grammar.md#remembered-results) and profiling again shows what it brings. No line names a rule marked `@nomemo`, a precedence block or one of its levels, which a mark cannot stand on.
- `--json` prints the same counts as JSON: the totals `steps`, `peak_stack`, `peak_nodes`, `memo_entries` and `restarts`, and `rows` with `name`, `remembered`, `calls`, `failures`, `steps`, `steps_with_callees`, `backtracked_bytes`, `repeats`, `memo_hits`, `memo_stores` and `growths` for every rule that was reached.

An input that does not parse is reported as `parse` reports it and still counted, and one that cannot be read is reported and left out. A start rule that the grammar lacks is reported once, and nothing is counted. The exit status is 1 then, and 0 when every input parses.
