# Performance

A grammar decides most of the time a parse takes: how often the parser tries something that fails, and how often it parses the same text twice. This guide shows how to see both, and what to change in a grammar.

## Look before changing

`fpeg profile` parses inputs and counts what each rule did. Take a grammar whose three alternatives all begin with the same rule, in `again.fpeg`:

```fpeg
start: item "." | item "!" | item
item: "(" start ")" | "x"
```

```text
fpeg profile again.fpeg input.txt
```

For the input `((((x))))` it prints:

```text
rule     calls  failed  steps  share  with callees  given back  repeats  hits  stored  grown
item       363       0   4473  71.1%          6273           0      358     0       0      0
start      121       0   1815  28.9%          6288           0      116     0       0      0
(parse)      0       0      2   0.0%             0           0        0     0       0      0

6290 steps, at most 27 entries on the stack and 10 nodes, 0 results remembered
@memo on item: 358 of its 363 calls repeat an earlier one, about 98% of all steps
@memo on start: 116 of its 121 calls repeat an earlier one, about 95% of all steps
```

A step is one instruction of the parser. Nine characters take 6290 of them, and the column `repeats` says why: of 363 calls of `item`, 358 are at a position where `item` was called before. Where `item "."` fails at the dot, `item "!"` parses the same item again, and at every level of the parentheses that triples the work. The [command line reference](../reference/cli.md#fpeg-profile) explains every column.

The numbers of steps change from one version of fpeg to the next. Their proportions are what to read.

## Remember a result

The profile ends with what it would try. `@memo` on a rule makes the parser remember each result of the rule with the position it was called at, and answer the next call there from memory:

```fpeg
start: item "." | item "!" | item
@memo
item: "(" start ")" | "x"
```

```text
rule     calls  failed  steps  share  with callees  given back  repeats  hits  stored  grown
start        5       0     90  55.2%           161           0        0     0       0      0
item        15       0     71  43.6%           143           0       10    10       5      0
(parse)      0       0      2   1.2%             0           0        0     0       0      0

163 steps, at most 27 entries on the stack and 10 nodes, 5 results remembered
```

The same input takes 163 steps. Ten of the fifteen calls of `item` are answered from the five results that were remembered.

- The levels of a precedence block and the rules that are left recursive are remembered without a mark, since they repeat by their nature.
- `@memo` adds a rule to those, and `@nomemo` takes one out.
- A remembered result takes memory until the parse ends. Remember the rules that repeat, not every rule: a rule that is never called twice at a position only pays for storing.

## Say it once

Better than remembering a repeated parse is a grammar that does not repeat it. The three alternatives differ only in what follows the item:

```fpeg
start: item ("." | "!")?
item: "(" start ")" | "x"
```

```text
rule     calls  failed  steps  share  with callees  given back  repeats  hits  stored  grown
item         5       0     71  55.5%           115           0        0     0       0      0
start        5       0     55  43.0%           126           0        0     0       0      0
(parse)      0       0      2   1.6%             0           0        0     0       0      0

128 steps, at most 22 entries on the stack and 10 nodes, 0 results remembered
```

No call repeats and nothing has to be remembered. The same holds for the order of alternatives: the one that matches most often goes first, as long as an earlier one does not hide a later one. `fpeg check` warns about alternatives that are never reached, which cost their tries and give nothing.

## Commit with a cut

A cut `~` says that the alternative it stands in is the only one from there on. What fails after the cut fails the whole choice, and no later alternative is tried:

```python
import fpeg

RULES = r"""
start: statement*
statement: "let" CUT NAME "=" NUMBER ";" | NAME ("=" NUMBER)? ";"
NAME: [a-z]+
NUMBER: [0-9]+
%ignore " "+
%word [a-z]
"""
plain = fpeg.Grammar(RULES.replace("CUT ", ""))
committed = fpeg.Grammar(RULES.replace("CUT", "~"))

# `let` is a name here, so without the cut the second alternative reads a statement about it.
assert plain.parse("let = 1;").children[0].children == ["let", "1"]
try:
    committed.parse("let = 1;")
except fpeg.ParseError as error:
    print(error)
```

```output
line 1, column 5: expected NAME
```

A cut saves the tries that would follow a failure, so it shortens the parse of an input that is wrong. An input that parses takes about the same steps, a couple more for each cut it passes. And it changes what the grammar accepts wherever a later alternative could have matched, as above. Write a cut where the text before it leaves no other reading, mostly right after a keyword.

## Limits

A grammar can make a parse take time that grows fast with the input, as the first grammar of this guide does, and input may come from anywhere. So every parse has a limit on its steps and one on how deep rule calls nest, and stops with a `LimitError` at either. The defaults are high enough for real input, and `parse` takes lower ones:

```python
again = fpeg.Grammar('start: item "." | item "!" | item\nitem: "(" start ")" | "x"')
try:
    again.parse("(" * 40 + "x" + ")" * 40, max_steps=100_000)
except fpeg.LimitError as error:
    print(error.limit)

nested = fpeg.Grammar('start: item\n?item: "x" | "(" item ")" -> group')
try:
    nested.parse("(" * 50_000 + "x" + ")" * 50_000)
except fpeg.LimitError as error:
    print(error.limit)
```

```output
steps
depth
```

A parse that reaches the step limit on real input is a sign to profile the grammar on a smaller input of the same shape.

## What else costs

- **Every node and token is made.** A terminal makes one token however much it matches, a rule a node for each match. Text that needs no structure belongs in a terminal.
- **Ignored text is skipped before every token.** A long `%ignore` choice is tried at each of them, so its commonest alternative goes first.
- **Failing late costs more than failing early.** The column `given back` of the profile counts the bytes that failed calls had got through before they failed. A rule that gives much back is tried where it does not belong, or shares its start with the alternative that then matches.

Measured times for whole grammars, and the scripts that take them, are in the `benchmarks` folder of the repository.
