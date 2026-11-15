# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""ORD, a grammar that extends the Python grammar, against the language's own parser.

The transformer of `ord_ast.py` extends the one of `fpeg.python` and gives the `ast` that the
language's own parser gives, compared by `ast.dump` on the 76 files of the application.
"""

import ast
import importlib
import os
import sys
import warnings
from pathlib import Path
from typing import Any, cast

import ord_ast
import pytest

sys.path.insert(0, str(Path(__file__).parent.parent / "benchmarks"))

import corpus

pytestmark = pytest.mark.skipif(
    sys.version_info < (3, 15), reason="the Python grammar follows Python 3.15"
)

SAMPLES = Path(__file__).parent.parent / "crates" / "fpeg" / "grammars" / "samples" / "ord"

# The ast.dump of Python 3.15, whose show_empty the type checker, set to the oldest Python fpeg
# runs on, does not know.
dump: Any = ast.dump


def test_the_samples_give_an_ast_that_python_compiles() -> None:
    paths = sorted(SAMPLES.glob("*.ord"))
    assert paths
    for path in paths:
        module = ast.fix_missing_locations(ord_ast.parse(path.read_text(encoding="utf-8")))
        compile(module, str(path), "exec")


def own_parser() -> Any:
    """Return the module of the language's own parser, or skip without it or without Lark."""
    pytest.importorskip("lark")
    if not corpus.ORD_PACKAGE.is_dir() or not corpus.ORD_FILES.is_dir():
        pytest.skip("the ORD files are missing, run benchmarks/corpus.py first")
    # The parser keeps its table cache under XDG_CACHE_HOME, which stays out of the home folder.
    os.environ.setdefault("XDG_CACHE_HOME", str(corpus.CORPORA.parent / ".cache"))
    sys.path.insert(0, str(corpus.CORPORA))
    return importlib.import_module("ordpkg.parser")


def dumped(module: ast.AST) -> str:
    # The language's parser builds its ast for Python 3.13, which has no field is_lazy.
    for node in ast.walk(module):
        if isinstance(node, (ast.Import, ast.ImportFrom)) and getattr(node, "is_lazy", 0) is None:
            cast(Any, node).is_lazy = 0
    return dump(module, indent=1, show_empty=True)


@pytest.mark.corpus
def test_every_ord_file_gives_the_tree_of_the_language_s_own_parser_and_compiles() -> None:
    parser = own_parser()
    paths = sorted(corpus.ORD_FILES.rglob("*.ord"))
    differ: list[str] = []
    for path in paths:
        source = path.read_text(encoding="utf-8")
        shown = path.relative_to(corpus.ORD_FILES)
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", SyntaxWarning)
            theirs = dumped(parser.ord_to_py(source))
        module = ord_ast.parse(source)
        mine = dumped(module)
        if mine != theirs:
            pairs = zip([*mine.splitlines(), ""], [*theirs.splitlines(), ""], strict=False)
            line = next(i for i, (a, b) in enumerate(pairs) if a != b)
            differ.append(f"{shown}: line {line} of the dump")
        try:
            compile(ast.fix_missing_locations(module), str(path), "exec")
        except (SyntaxError, ValueError, TypeError) as error:
            differ.append(f"{shown}: does not compile: {error}")
    assert len(paths) == 76, f"{len(paths)} ORD files"
    assert not differ, f"{len(differ)} of {len(paths)} files differ:\n" + "\n".join(differ)
