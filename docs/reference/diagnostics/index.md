# Diagnostics

Every problem fpeg finds in a grammar has a stable code, and every code has a page here with a grammar that has the problem and the grammar fixed. The message beside the code says what exactly is wrong.

`fpeg check` prints each problem with its place in the grammar file, line and column counted from 1:

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

```{toctree}
:glob:
:maxdepth: 1

E*
```
