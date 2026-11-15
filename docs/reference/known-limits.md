# Known limits

What fpeg does not do, or does at a cost, and why. Where another page says more about a limit, its link leads there.

## Sizes

- A text holds at most 4 GiB less one byte, since positions are 32-bit. A longer text raises `LimitError` with the limit `"input size"`.
- A grammar declares at most 64 [flags](grammar.md#flags), and its other sizes are bounded as [E402](diagnostics/E402.md) lists them.

## Parsing

- A parse that remembers results keeps the nodes that a remembered match built until the parse ends, also where the match around it failed later. The memory limit of [`Grammar.parse`](python-api.md#errors) bounds what a parse holds.
- A parse that [starts again](../guide/performance.md) with every rule remembered spends the steps of its first run from the step limit, so a parse that needs nearly all of the limit may reach it where it would not have without starting again.
- The parser remembers only the last stretch of ignored text it skipped, so where an alternative comes back to a position that another alternative skipped past, the ignored text is skipped again.
- `fpeg profile` counts the steps of the grammar as written. A parse outside the profile leaves out work by itself and takes fewer steps on most grammars, but the tests that let it leave work out cost steps too, so on some grammars it takes more steps than its [profile](cli.md#fpeg-profile) counts.
- The memory limit counts the hash tables of a parse by the layout of Rust's own, so a later version of Rust could make the count differ a little from what a table holds.
- A grammar that tests the column with `%indent` or `%same` at every character of one long line takes time that grows with the square of the line's length, until the step limit stops it.
- A recovery explains its error once for its label and its place, so where one label recovers twice at one place, as for missing tokens in a row, the second error says what the first said.

## Checks

- [W301](diagnostics/W301.md) takes for granted that every part of a grammar can match as written. Whether a part can match is undecidable in general.
- Where the `%ignore` expression can raise an error, [W201](diagnostics/W201.md) reports only the repetitions of terminals.
- [W101](diagnostics/W101.md) and [W201](diagnostics/W201.md) take a parameter of a template for an expression that can cut and raise, so they stay silent around templates more than they need to.

## Python package

- A parse of a text shorter than 4,096 bytes keeps the interpreter lock, since taking the lock back from a busy thread can take far longer than such a parse. A grammar written to be slow on a short text holds other threads until the step limit stops it.
- The methods of an [`Interpreter`](python-api.md) visit by recursion, about 330 levels deep at Python's default recursion limit, and a tree nested deeper ends the walk with `RecursionError`. A `Transformer` and a `Visitor` take a tree of any depth.
- A function given in `functions` that refers back to its grammar, as a closure over the grammar does, keeps both alive until the process ends, since the grammar holds its functions where the garbage collector cannot reach them. A function at module level makes no such cycle.
- A token on its own finds the builder of its terminal through its grammar and does not keep the grammar alive, so keep the grammar while such a token is transformed.
- A `Transformer` reads the methods and signatures of its class at its first transform of a tree from a grammar and keeps them, so a method added later is not called on that grammar's trees.
- A token transformed on its own, as `parse` returns one where the start rule leaves a single token, gets no position from `positions`, since it holds no text of its line to count its columns in.
- Views of a text beyond ASCII take longer to make than views of an ASCII text, since their positions count code points.
- `scan_values` collects its results before it yields them, so on a tree built in Python that shares subtrees many times over it can outgrow memory.
- A transparent rule whose alternative always leaves two items or more keeps its node, yet the fields that rules without nodes of their own pass on to it count for its parent too, where `field()` gives `None` for them and `transformer_template()` offers them.
- A text too large for memory raises `MemoryError`. When the process runs out of memory as a whole, there is no such promise: memory that fpeg's native code cannot get ends the process, and a small string the Python package makes, such as an attribute of an error, can raise `PanicException`.
- [`fpeg.python`](python-api.md) builds the `ast` of Python 3.15 and needs that version.

## Command line

- The indented output of [`fpeg parse`](cli.md) grows with the depth of a tree times its items, so deep trees want `--json`.

## Modules

- On Windows a module file is found whatever the case of its name, as the file system finds it, so a grammar can load there and not on Linux.
- `fpeg profile` may suggest `@memo` for a rule imported by name, which the grammar that imports it cannot mark.

## Grammars in the repository

- The Python grammar, which comes with fpeg, accepts a few forms that CPython refuses only after parsing, such as an assignment to a call, and indentation that mixes tabs and spaces where CPython finds it ambiguous. It nests as deep as the depth limit allows, where CPython takes at most 200 nested brackets and 149 nested f-strings. It rejects a valid line whose column after a backslash continuation CPython takes from before the backslash.
- The Lua grammar accepts any prefix expression as a statement or as a target.
