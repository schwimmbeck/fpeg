# Diagnostics

Everything fpeg finds in a grammar has a stable code, and every code has a page here with a grammar that shows it and the grammar fixed. The message beside the code says what exactly is meant.

- A code that starts with `E` is an error: the grammar cannot be used until it is fixed.
- A code that starts with `W` is a warning: the grammar works, and likely not as its author meant. A warning is given only for what holds whatever the input is, so a grammar without warnings is not proven right, and a warning is never a guess.

`fpeg check` prints each one with its place in the grammar file, line and column counted from 1:

```text
grammar.fpeg:1:8: error[E201]: unknown rule item
```

The hundreds of a code tell its area.

| Codes | Area |
|---|---|
| E101 to E104 | reading the grammar text |
| E201 to E208 | names and references |
| E301 to E310 | forms that do not fit where they stand |
| E401, E402 | limits on the size of a grammar |
| E501 | what a later milestone of the roadmap brings |
| E901 | a bug of fpeg itself |
| W101 | alternatives that a choice never reaches |

```{toctree}
:glob:
:maxdepth: 1

E*
W*
```
