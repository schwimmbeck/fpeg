# A language over Python

ORD is a language for designing integrated circuits: Python with a few statements of its own. A cell holds view generators, and inside them a statement places an instance or declares a pin, port, net or path, and sets the attributes and parameters of what it placed. A program of ORD runs as a Python module. This page shows ORD written for fpeg, a grammar that extends the Python grammar and a transformer that extends the one to Python's `ast`, checked against the language's own parser.

## The language

```text
cell Divider:
    viewgen symbol(self) -> Symbol:
        input vin
        output vout

    viewgen schematic(self) -> Schematic:
        port vin: .pos=(0, 6)
        port vout: .pos=(8, 3)
        Res r1: .$r=10k; .p -- vin; .m -- vout
        Res r2: .$r=4.7k; .p -- vout; .m -- gnd
        ! .vout.pos.y < .vin.pos.y
```

- `cell Divider:` is a class, and `viewgen schematic(self) -> Schematic:` a method that builds a view of it.
- `input vin` declares a pin, `port vin: ...` a port, and `Res r1: ...` places an instance of the cell `Res`, each with a body that the placed thing is the root of.
- `.pos` is an attribute of that root, `.$r` one of its parameters, `10k` a number with an SI prefix, and `!` states a constraint.
- `.p -- vin` connects two nets, which needs nothing from the grammar: Python reads it as `.p - (-vin)`.

## The grammar

```{literalinclude} ../../crates/fpeg/grammars/ord.fpeg
:language: fpeg
:lines: 4-
```

Every rule that ORD changes is an ordinary rule of the Python grammar: the compound and the simple statements, the primary expressions and the atoms. `...` keeps what Python has. The new forms come first, since a statement such as `input vin` would otherwise start an expression statement with `input`, and the line would fail at its second name, as Python fails on it. A statement of ORD tries a primary before Python's statements try the same primary again, which `@memo` turns into one try.

## The transformer

The transformer is a subclass of `fpeg.python.ToAst` with a method for each new rule. A cell gives a `ClassDef`, a view generator a decorated `FunctionDef`, and a statement that places something an assignment from the view's context with a `with` around its body. A method gives a list where one statement of ORD becomes two. `ast.fix_missing_locations` gives the nodes that a method builds the position of the statement around them.

## Checked against the language's own parser

The tests read the 76 programs of the application at a pinned commit with this grammar and transformer, and compare the `ast` with the one the language's own parser makes, by `ast.dump` with every field shown. All 76 agree. The language's parser also takes two statements on one line without a `;`, as `net a b`, which Python rejects and this grammar rejects too.

## Time

The benchmarks run the language's own pipeline and fpeg's on the same file, from source text to an `ast` with positions, and the [benchmarks page](../benchmarks.md) shows both.
