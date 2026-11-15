# Performance

A grammar decides most of the time a parse takes: how often the parser tries something that fails, and how often it parses the same text twice. This guide shows how to see both, and what to change in a grammar.

## Look before changing

`fpeg profile` parses inputs and counts what each rule did. Take a grammar whose three alternatives all begin with the same rule, two of them through rules of their own, in `again.fpeg`:

```fpeg
start: dot | bang | item
dot: item "."
bang: item "!"
item: "(" start ")" | "x"
```

```text
fpeg profile again.fpeg input.txt
```

For the input `((((x))))` it prints:

```text
rule     calls  failed  steps  share  with callees  given back  repeats  hits  stored  grown
item       363       0   4473  66.0%          6753           0      358     0       0      0
start      121       0   1331  19.6%          6772           0      116     0       0      0
bang       121     121    484   7.1%          5505         237      116     0       0      0
dot        121     121    484   7.1%          5505         237      116     0       0      0
(parse)      0       0      2   0.0%             0           0        0     0       0      0

6774 steps, at most 37 entries on the stack and 10 nodes, 0 results remembered
@memo on item: 358 of its 363 calls repeat an earlier one, about 97% of all steps
@memo on start: 116 of its 121 calls repeat an earlier one, about 95% of all steps
@memo on bang: 116 of its 121 calls repeat an earlier one, about 64% of all steps
@memo on dot: 116 of its 121 calls repeat an earlier one, about 64% of all steps
```

A step is one instruction of the parser. Nine characters take 6774 of them, and the column `repeats` says why: of 363 calls of `item`, 358 are at a position where `item` was called before. Where `dot` fails at the dot, `bang` parses the same item again, and so does the last alternative, and at every level of the parentheses that triples the work. The [command line reference](../reference/cli.md#fpeg-profile) explains every column.

The numbers of steps change from one version of fpeg to the next. Their proportions are what to read.

## Remember a result

The profile ends with what it would try. `@memo` on a rule makes the parser remember each result of the rule with the position it was called at, and answer the next call there from memory:

```fpeg
start: dot | bang | item
dot: item "."
bang: item "!"
@memo
item: "(" start ")" | "x"
```

```text
rule     calls  failed  steps  share  with callees  given back  repeats  hits  stored  grown
item        15       0     71  38.8%           159           0       10    10       5      0
start        5       0     60  32.8%           181           0        0     0       0      0
bang         5       5     25  13.7%            25          25        0     0       0      0
dot          5       5     25  13.7%           164          25        0     0       0      0
(parse)      0       0      2   1.1%             0           0        0     0       0      0

183 steps, at most 37 entries on the stack and 10 nodes, 5 results remembered
```

The same input takes 183 steps. Ten of the fifteen calls of `item` are answered from the five results that were remembered.

- A rule that a choice tries twice at one position is remembered without a mark: one that two of its alternatives start with, as in `start: item "." | item "!" | item`, or a part that can end after a failed try and the part after it, as in `item? item`. Only the rules that a choice starts with itself count, not those they call in turn, as `item` above through `dot` and `bang`, and terminals do not, which take about as long to match again as to look up.
- A left-recursive rule is remembered without a mark where each pass over it tries its other alternatives again. One whose alternatives start with the rule itself before the others, as the levels of a precedence block do, runs as a loop that tries them once and is not.
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

## Columns, captures and remembered results

A remembered result holds for the position it was made at and for what the rule could see there: the flags, the reference column of [indentation](indentation.md) and the visible [captures](interpolated-strings.md#delimiters-the-input-chooses). The parser keeps the reference column and the captures together as one context, and a call at the same position in another context parses again.

- Most grammars call a rule at a position in one context only. A statement of Python is parsed inside one block, so the context costs nothing but its place in the key.
- A grammar that tries the same text inside a block and outside of it, as in `%indent(item) | item`, parses it twice, remembered or not. Say it once, as for any repeated try.
- A capture made in a terminal ends with the token, and the rules around it keep the context they had. A capture made in a syntactic rule holds for every rule it calls, which remember their results under it.
- Alternatives skip once at a position, as below, and a skip expression that checks a column or matches a capture skips once for each context there, since another reference column may end the skip elsewhere.

## Limits

A grammar can make a parse take time that grows fast with the input, as the first grammar of this guide does, and input may come from anywhere. So every parse has a limit on its steps, one on how deep rule calls nest and one on the memory it holds, and stops with a `LimitError` at any of them. A rule that calls itself counts at every level of the nesting, while a small rule that does not may be compiled into the rules that call it and count not at all. The defaults are high enough for real input, and `parse` takes lower ones:

```python
again = fpeg.Grammar(
    'start: dot | bang | item\ndot: item "."\nbang: item "!"\nitem: "(" start ")" | "x"'
)
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

A parse that takes more than a million steps, or a thousand for each byte of input where that is more, starts again with every rule remembered, for the steps its limit leaves. Remembering never changes what a parse gives, so a grammar that tries a rule again where nothing remembers it, as the first grammar of this guide does, still parses in time that grows with the input. Where remembering every rule needs more memory or deeper nesting than the limits allow, the parse runs once more as it started, for the steps left. The runs before are work lost, which the profile shows: it counts every run, and says how many parses started again. Their steps count against the limit, so a parse that needs nearly all of it may reach it.

A parse that reaches the step limit on real input, or that starts again, is a sign to profile the grammar on a smaller input of the same shape.

## What the parser does by itself

Some work that a grammar seems to ask for, the parser leaves out, and none of it changes what a parse gives:

- A repetition of a class, as `[0-9]*`, takes its whole run at once, and so does a repetition of a class with something else, as the characters and escapes of a string.
- Alternatives of a choice that skip at the same position skip once, since the parser keeps where the last skip ended. An `%ignore` that repeats a class, as `" "+`, takes its whole run at once, and a skip at a byte that no alternative of `%ignore` can start with ends at once.
- Before an alternative, the parser looks at the next byte and passes over an alternative that cannot start with it.
- A small rule that calls itself nowhere becomes part of the rules that call it.
- A rule whose alternatives start with the rule itself, before the others, as the levels of a precedence block do, runs as a loop over its operators.

The profile counts what the grammar asks for, with none of this but the loop, so that every rule keeps its calls and failures, and a rule that runs as a loop repeats what it repeats in the parse. A parse outside the profile takes fewer steps on most grammars.

## What else costs

- **Every node and token is made.** A terminal makes one token however much it matches, a rule a node for each match. Text that needs no structure belongs in a terminal.
- **Ignored text is skipped before every token.** Wherever the next byte can start one of its alternatives, a long `%ignore` choice is tried, so its commonest alternative goes first.
- **Failing late costs more than failing early.** The column `given back` of the profile counts the bytes that failed calls had got through before they failed. A rule that gives much back is tried where it does not belong, or shares its start with the alternative that then matches.

[How fpeg works](../explanation/how-fpeg-works.md) says where a parse and a transform spend their time. Measured times for whole grammars are on the page of [benchmarks](../explanation/benchmarks.md), and the scripts that take them in the `benchmarks` folder of the repository.
