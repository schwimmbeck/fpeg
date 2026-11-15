# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The Python grammar against CPython's `ast`: the same tree with the same positions.

The transformer of `python_ast.py` turns the grammar's tree into `ast` nodes, which are compared
with those of `ast.parse` by `ast.dump`, then node by node by their positions.
"""

import ast
import sys
import sysconfig
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
\ufb01, \u212bngstr\u00f6m.\u2168
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
LIBRARY = Path(sysconfig.get_path("stdlib"))
# Small files of the library that hold between them nearly every kind of node it has.
LIBRARY_FILES = [
    "_pyrepl/base_eventqueue.py",
    "_pyrepl/pager.py",
    "_pyrepl/types.py",
    "_pyrepl/utils.py",
    "asyncio/staggered.py",
    "asyncio/threads.py",
    "asyncio/trsock.py",
    "base64.py",
    "concurrent/interpreters/_crossinterp.py",
    "contextlib.py",
    "operator.py",
    "profiling/sampling/_format_utils.py",
    "reprlib.py",
    "string/templatelib.py",
    "turtledemo/paint.py",
    "xml/utils.py",
]
FILES = [
    *(pytest.param(path, id=path.name) for path in sorted(SAMPLES.glob("*.py"))),
    *(pytest.param(LIBRARY / name, id=name) for name in LIBRARY_FILES),
]


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


@pytest.mark.parametrize("line_break", ["\n", "\r\n", "\r"], ids=["LF", "CRLF", "CR"])
@pytest.mark.parametrize("path", FILES)
def test_a_file_gives_the_tree_of_cpython(path: Path, line_break: str) -> None:
    # Reading a file as text turns its line breaks into line feeds.
    compare(path.read_text(encoding="utf-8").replace("\n", line_break))


@pytest.mark.corpus
def test_every_file_of_the_library_gives_the_tree_of_cpython() -> None:
    compared = 0
    differ: list[str] = []
    for path in sorted(LIBRARY.rglob("*.py")):
        if "site-packages" in path.parts:
            continue
        try:
            source = path.read_text(encoding="utf-8")
            with warnings.catch_warnings():
                warnings.simplefilter("ignore", SyntaxWarning)
                ast.parse(source)
        except (SyntaxError, UnicodeDecodeError):
            continue  # test data that Python rejects
        compared += 1
        try:
            compare(source)
        except Exception as error:  # every file that differs joins the report
            differ.append(f"{path.relative_to(LIBRARY)}: {str(error).splitlines()[0]}")
    assert compared, f"no file to compare in {LIBRARY}"
    assert not differ, f"{len(differ)} of {compared} files differ:\n" + "\n".join(differ)
