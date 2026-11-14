# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The Python grammar against CPython's `ast`: the same tree with the same positions.

The transformer of `python_ast.py` turns the grammar's tree into `ast` nodes, which are compared
with those of `ast.parse` by `ast.dump`, then node by node by their positions.
"""

import ast
import sys
import warnings
from pathlib import Path

import pytest
import python_ast

pytestmark = pytest.mark.skipif(
    sys.version_info < (3, 15), reason="the Python grammar follows Python 3.15"
)

ATTRIBUTES = ("lineno", "col_offset", "end_lineno", "end_col_offset")

EXPRESSIONS = """\
a + b * c - d / e // f % g ** -h ** i @ j
a << b >> c & d | e ^ ~f
+a, -b, not c
a and b or c and not d or e
a < b <= c > d >= e == f != g in h not in i is j is not k
a if b else c if d else e
lambda: 0
lambda a, b=1, /, c=2, *d, e, f=3, **g: a
lambda *, a, b=1: (a, b)
(y := f(x)), [z := 1]
f(a, *b, c=d, **e)(g)(h for h in i)
a.b.c[d][e:f][::g][h, i:j][k,][l:m:n]
[a, *b], (c, *d), {e, *f}, {g: h, **i, j: k}, (), [], {}
[x for x in y if x if z for w in x]
{x for x in y}, {k: v for k, v in y}
[*x for x in y], {**x for x in y}
(x async for x in y)
await a
f((yield)), f((yield a, b)), f((yield from a))
1, 1.5, 1j, 0x1F, 0o17, 0b101, 1_000, 1e10, .5, 5.
True, False, None, ...
(a)
(
    a  # a comment
    + b
)
"""

STATEMENTS = """\
import a.b as c, d
from . import e
from ..f import (g as h, i,)
from j import *
lazy import k
lazy from l import m
x = y = 1
x: int = 1
(x): int
x.y: int
x[0] += 1; x -= 1
a, *b = c
[a, (b, c)] = d
del a, b[0], c.d
del (a, b)
assert a, b
global g
nonlocal n
pass; break; continue
return
return a, b
raise
raise a from b
yield
yield a, b
x = yield


@d
@e.f(g)
def f(a, /, b: int = 1, *c: int, d, e=2, **f: float) -> None:
    pass  # a comment

    # a comment after a blank line


async def g(*, a):
    async for x in y:
        pass
    else:
        pass
    async with a as b, c:
        pass


class C(B, metaclass=M):
    x = 1


class D: pass


if a:
    pass
elif b:
    pass
elif c:
    if d: pass
else:
    pass

while a:
    break
else:
    pass

for x, y in z:
    continue

with a as b, c as (d, e):
    pass

with (a as b, c):
    pass

try:
    pass
except E as e:
    pass
except (F, G):
    pass
except H, I:
    pass
except:
    pass
else:
    pass
finally:
    pass

try:
    pass
except* E:
    pass

try:
    pass
finally:
    pass
"""

SAMPLES = Path(__file__).parent.parent / "crates" / "fpeg" / "grammars" / "samples" / "python"
FILES = [pytest.param(path, id=path.name) for path in sorted(SAMPLES.glob("*.py"))]


def compare(source: str) -> None:
    """Assert that the transformer gives the tree `ast.parse` gives, positions included."""
    with warnings.catch_warnings():
        # Some samples use unknown escapes on purpose, which Python only warns about.
        warnings.simplefilter("ignore", SyntaxWarning)
        theirs = ast.parse(source)
    mine = python_ast.parse(source)
    assert ast.dump(mine, indent=1) == ast.dump(theirs, indent=1)
    assert positions(mine) == positions(theirs)


def positions(tree: ast.AST) -> list[tuple[str, tuple[object, ...]]]:
    """The type and the position of each node that has one, in the order of `ast.walk`."""
    return [
        (type(node).__name__, tuple(getattr(node, name, None) for name in ATTRIBUTES))
        for node in ast.walk(tree)
        if "lineno" in node._attributes
    ]


def test_expressions_give_the_tree_of_cpython() -> None:
    compare(EXPRESSIONS)


def test_statements_give_the_tree_of_cpython() -> None:
    compare(STATEMENTS)


@pytest.mark.parametrize("path", FILES)
def test_a_file_gives_the_tree_of_cpython(path: Path) -> None:
    compare(path.read_text(encoding="utf-8"))
