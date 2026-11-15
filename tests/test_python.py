# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The Python grammar against CPython's `ast`: the same tree with the same positions.

The transformer of `fpeg.python` turns the grammar's tree into `ast` nodes, which are compared
with those of `ast.parse` by `ast.dump`, then node by node by their positions.
"""

import ast
import random
import sys
import sysconfig
import warnings
from collections import Counter
from pathlib import Path
from typing import Any

import fpeg
import pytest
from fpeg import python

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
    mine = python.parse(source)
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


def test_a_method_of_a_subclass_may_give_several_statements_for_one() -> None:
    class Twice(python.ToAst):
        def pass_(self) -> list[ast.stmt]:
            return [ast.Pass(), ast.Pass()]

    module = Twice().transform(python.GRAMMAR.parse("if x: pass; y\nwhile z:\n    pass\n"))
    assert [type(s).__name__ for s in module.body[0].body] == ["Pass", "Pass", "Expr"]
    loop = module.body[1]
    assert [type(s).__name__ for s in loop.body] == ["Pass", "Pass"]
    # Both take the place of the statement they stand for, and the loop keeps its end.
    assert [(s.lineno, s.col_offset) for s in loop.body] == [(3, 4), (3, 4)]
    assert (loop.end_lineno, loop.end_col_offset) == (3, 8)
    compile(module, "<twice>", "exec")

    # Decorators need one definition to stand before.
    class Listed(python.ToAst):
        def class_def(self, name: Any, params: Any, bases: Any, body: Any) -> list[ast.stmt]:
            return [super().class_def(name, params, bases, body)]

    with pytest.raises(TypeError, match="one statement"):
        Listed().transform(python.GRAMMAR.parse("@d\nclass C: pass\n"))


def test_the_ast_keeps_no_marks_of_the_transformer() -> None:
    module = python.parse('def f(a, b=1, *c, d=2, **e): pass\nf"{x=} {y:{z}}"\nt"{x}"\n')
    left = {name for node in ast.walk(module) for name in vars(node) if name.startswith("fpeg_")}
    assert left == set()


def test_the_transformer_refuses_a_python_older_than_its_ast(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(python, "_AST_PYTHON", (99, 0))
    with pytest.raises(RuntimeError, match="needs it"):
        python.ToAst()
    # parse refuses before it parses, also a text it would refuse.
    with pytest.raises(RuntimeError, match="needs it"):
        python.parse("(")


def test_the_default_depth_takes_the_deepest_nesting_cpython_takes() -> None:
    fstring = "1"
    for _ in range(149):
        fstring = 'f"{' + fstring + '}"'
    for source in ("x = " + "(" * 200 + "1" + ")" * 200, "y = " + "f(" * 200 + ")" * 200, fstring):
        assert ast.dump(python.parse(source + "\n")) == ast.dump(ast.parse(source + "\n"))


def test_parse_takes_limits_and_refuses_a_literal_that_python_cannot_read() -> None:
    deep = "(" * 300 + "1" + ")" * 300 + "\n"
    with pytest.raises(fpeg.LimitError):
        python.parse(deep)
    assert isinstance(python.parse(deep, max_depth=100_000), ast.Module)
    backslash = chr(92)
    # The grammar takes the form of these, and Python cannot read them.
    for source in [f'f"{backslash}N{{BOGUS}}"', '"a' + chr(0) + 'b"', "1" * 5000]:
        with pytest.raises(SyntaxError):
            python.parse(source + "\n")
    # The grammar refuses an incomplete escape in a format spec, as in a string, and a code
    # point beyond Unicode.
    for source in [f'f"{{x:{backslash}x1}}"', f'"{backslash}U00110000"']:
        with pytest.raises(fpeg.ParseError):
            python.parse(source + "\n")
    assert python.parse(f'"{backslash}U0010FFFF"').body[0].value.value == chr(0x10FFFF)


@pytest.mark.parametrize("line_break", ["\n", "\r\n", "\r"], ids=["LF", "CRLF", "CR"])
@pytest.mark.parametrize("path", FILES)
def test_a_file_gives_the_tree_of_cpython(path: Path, line_break: str) -> None:
    # Reading a file as text turns its line breaks into line feeds.
    compare(path.read_text(encoding="utf-8").replace("\n", line_break))


# Broken files: small files of the library, each with single edits that CPython rejects, and
# what the trees recovered from them keep, set from the first measurement on 2026-11-15, which
# gave all 160 a tree that kept 97.7 percent of the visible tokens.
BROKEN_FILES = 40
BROKEN_FILE_MAX_CHARS = 20_000
EDITS_PER_FILE = 20
INSERTED = "()[]{}:,.=+-*/\"' \n#abc019"
MIN_TREES_PERCENT = 99.0
MIN_KEPT_PERCENT = 97.0


def test_a_broken_file_keeps_the_statements_around_its_mistakes() -> None:
    source = "def f(x)\n    return x\n\ny = = 2\nz = 3\n"
    with pytest.raises(fpeg.ParseError) as raised:
        python.GRAMMAR.parse(source, recover=True)
    error = raised.value
    assert [(e.label, e.line, e.column) for e in error.errors] == [
        ("colon", 1, 9),
        ("newline", 4, 3),
    ]
    assert isinstance(error.tree, fpeg.Tree)
    kinds = [child.data for child in error.tree.children if isinstance(child, fpeg.Tree)]
    assert kinds == ["function_def", "simple_statements", "simple_statements"]


def visible_tokens(tree: fpeg.Tree | fpeg.Token) -> Counter[tuple[str, str]]:
    found = (
        tree.scan_values(lambda v: isinstance(v, fpeg.Token))
        if isinstance(tree, fpeg.Tree)
        else [tree]
    )
    return Counter((t.type, str(t)) for t in found if not t.type.startswith("<"))


@pytest.mark.corpus
def test_broken_files_of_the_library_keep_most_of_their_tokens() -> None:
    texts = [p.read_text(encoding="utf-8") for p in sorted(LIBRARY.glob("*.py"))[:BROKEN_FILES]]
    rng = random.Random(8)
    broken = trees = 0
    kept = 0.0
    for text in (t for t in texts if len(t) < BROKEN_FILE_MAX_CHARS):
        want = visible_tokens(python.GRAMMAR.parse(text))
        for _ in range(EDITS_PER_FILE):
            at = rng.randrange(len(text))
            kind, char = rng.choice(["delete", "insert", "replace"]), rng.choice(INSERTED)
            after = text[at + 1 :] if kind != "insert" else text[at:]
            edited = text[:at] + ("" if kind == "delete" else char) + after
            try:
                with warnings.catch_warnings():
                    warnings.simplefilter("ignore")
                    ast.parse(edited)
                continue
            except SyntaxError:
                pass
            try:
                python.GRAMMAR.parse(edited, recover=True)
                continue  # the grammar takes a superset that a later check narrows
            except fpeg.ParseError as error:
                broken += 1
                if error.tree is not None:
                    trees += 1
                    got = visible_tokens(error.tree)
                    kept += sum(min(n, got[key]) for key, n in want.items()) / want.total()
    trees_percent, kept_percent = 100 * trees / broken, 100 * kept / broken
    assert broken > 100, broken
    assert trees_percent >= MIN_TREES_PERCENT, f"{trees} of {broken} give a tree"
    assert kept_percent >= MIN_KEPT_PERCENT, f"{kept_percent:.1f} % of the tokens kept"


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
