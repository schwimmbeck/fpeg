# How fpeg works

What happens between the text of a grammar and the values a program gets, and what each step costs. It explains why the advice of the [performance guide](../guide/performance.md) works, and what a program can expect of a parse.

```{diagram} pipeline.svg
Loading a grammar compiles it into a program once. Each parse runs that program in the parsing machine, which builds the tree in native memory: the work in red. Python objects come into being only where code reads the tree or a `Transformer` asks for values.
```

## Loading a grammar

`Grammar(text)` reads the text with a parser of the grammar language, written in that language itself. It joins the modules the grammar extends and imports, resolves every name, and checks the grammar, which raises `GrammarError` for a problem it cannot compile. Then it compiles the grammar to a program for the parsing machine. On the way, optimization passes rewrite the program so that a parse leaves out work the grammar seems to ask for, and none of them changes what a parse gives.

A grammar is immutable once loaded, so one grammar serves every parse of a program, from any thread. Loading costs far more than a parse of a short input, so a program loads a grammar once and keeps it.

## Parsing

The parsing machine runs the program over the input. It tries the alternatives of a choice in order, takes the first that matches and goes back to where the choice started when one fails, as a parsing expression grammar means. So every input has at most one tree, and the grammar decides it, never an ambiguity resolved afterwards.

The time a parse takes is the work the machine does, counted in steps, and a grammar decides most of it:

- **Trying again.** An alternative that fails after matching a long stretch gives that work back, and the next alternative may parse the same text again. The machine remembers the results of the rules that a choice can try twice at one position, and `@memo` marks more. The performance guide shows how to find what repeats.
- **Starting again.** A parse that takes far more steps than its input's length calls for starts again with every rule remembered. Remembering never changes what a parse gives, so even a grammar that repeats work parses in time that grows with the input.
- **Limits.** Every parse has a limit on its steps, on how deep rule calls nest and on the memory it holds, and stops with a `LimitError` at any of them. A grammar and an input can come from anywhere, so no parse runs without bounds. The machine keeps its own stack in memory it counts, so a deeply nested input reaches the depth limit, never the end of the process's stack.

## The tree

A parse builds one tree in native memory: a compact array in postorder, where the children of a node come before the node. Each entry holds a kind, a span of byte offsets into the input, the size of its subtree and its field, so a node takes the same few bytes whatever it holds. Hidden tokens such as punctuation stay in the array, which keeps the tree lossless. Lines and columns are worked out from the offsets when something asks for them.

The [parse tree](../reference/tree.md) reference describes the tree, and the [semantics](../reference/semantics.md) what a grammar makes of an input.

## From the tree to Python

Making a Python object costs more than parsing the text it stands for, so the Python package makes objects only where a program asks for them:

- A view, a `Tree` for a node or a `Token` for a token, is made when code first reads it. A program that reads a part of a large tree pays for the views of that part.
- A `Transformer` scans the array once, from the start, in native code. It calls a Python method only for the nodes and tokens whose rule or terminal has one, and a subtree that no method changes comes back as the tree it was.
- A built-in value builder, such as `=> int` or `=> dict`, makes its value in native code, without calling Python code. A tree whose every rule has one becomes values without running any code of the program.
- A parse of a longer input lets other Python threads run while it works.

So the cost of turning a tree into values grows with the methods called and the objects made. A grammar that shapes its tree, with text that needs no structure in a terminal, transparent rules for nodes that only pass a child on and hidden tokens for punctuation, makes fewer nodes to visit, and builders make the common values without a call.

The [benchmarks](benchmarks.md) show what these steps take for whole grammars, beside other parsers.
