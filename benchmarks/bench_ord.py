# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The first real application: its Lark pipeline from source text to a Python ``ast``, and fpeg's.

Its parser package is downloaded by ``corpus.py`` and imported here under the name ``ordpkg``.
fpeg's ORD grammar and its transformer live with the tests, whose oracle that pipeline is.
Run through ``run.py``, or directly with pyperf options.
"""

import ast
import os
import sys
from collections.abc import Callable
from functools import partial
from pathlib import Path

import fpeg
import harness
from corpus import CORPORA, ORD_PACKAGE, PYTHON_FILE
from lark import Lark, Transformer, Tree

CACHE = Path(__file__).resolve().parent / ".cache"

# The package keeps its LALR table cache under XDG_CACHE_HOME. Keep it out of the home folder.
os.environ["XDG_CACHE_HOME"] = str(CACHE)
sys.path.insert(0, str(CORPORA))
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tests"))

import ord_ast  # noqa: E402  the path comes first
import ordpkg.parser as ord_parser  # noqa: E402  the path and cache location come first

GRAMMAR = ORD_PACKAGE / "ord.lark"
OPTIONS = {
    "parser": "lalr",
    "start": "file_input",
    "maybe_placeholders": False,
    "propagate_positions": True,
}


def load(cache: str | None = None) -> Lark:
    """Load the application's grammar the way its parser module does."""
    return Lark.open(
        str(GRAMMAR), postlex=ord_parser.PythonTokenAwareIndenter(), cache=cache, **OPTIONS
    )


def dumped(module: ast.AST) -> str:
    """Return the dump of a module with the field is_lazy of the application's Python set."""
    for node in ast.walk(module):
        if isinstance(node, (ast.Import, ast.ImportFrom)) and getattr(node, "is_lazy", 0) is None:
            node.is_lazy = 0
    return ast.dump(module)


def trivial_transformer(tree: Tree) -> type[Transformer]:
    """Return a Transformer class whose method for every rule of the tree returns the children."""
    rules = {str(subtree.data) for subtree in tree.iter_subtrees()}
    methods = {rule: lambda self, children: children for rule in rules}
    return type("Trivial", (Transformer,), methods)


def main() -> None:
    """Register and run the benchmarks."""
    runner = harness.runner()
    source = PYTHON_FILE.read()
    # The parser module appends this newline before it parses.
    text = source + "\n"
    parser = ord_parser.parser

    def warm_cache():
        cache_file = str(CACHE / "benchmark.lark_cache")
        load(cache_file)
        return partial(load, cache_file)

    def real_transform():
        tree = parser.parse(text)
        return lambda: ord_parser.OrdTransformer(source_text=text).transform(tree)

    def trivial_transform():
        tree = parser.parse(text)
        cls = trivial_transformer(tree)
        return lambda: cls().transform(tree)

    harness.add(runner, "grammar load without cache", lambda: load)
    harness.add(runner, "grammar load with a warm cache", warm_cache)
    harness.add(runner, "LALR parse, tree", lambda: partial(parser.parse, text), PYTHON_FILE)
    harness.add(runner, "Transformer with the real callbacks", real_transform)
    harness.add(runner, "Transformer, trivial method on every rule", trivial_transform)
    harness.add(
        runner,
        "end to end: parse, transform, fix locations",
        lambda: partial(ord_parser.ord_to_py, source),
        PYTHON_FILE,
    )

    def checked(fix: bool) -> Callable[[], ast.AST]:
        def to_ast() -> ast.AST:
            module = ord_ast.parse(source)
            return ast.fix_missing_locations(module) if fix else module

        if dumped(to_ast()) != dumped(ord_parser.ord_to_py(source)):
            raise RuntimeError("fpeg's ORD pipeline does not give the tree of the application's")
        return to_ast

    harness.add(
        runner, "fpeg ORD grammar load", lambda: partial(fpeg.Grammar.open, ord_ast.GRAMMAR_PATH)
    )
    harness.add(
        runner, "fpeg parse, tree", lambda: partial(ord_ast.GRAMMAR.parse, source), PYTHON_FILE
    )
    # Stamped natively, every node of a file of plain Python has its position, and the fix
    # finds nothing to do. It stays in the end to end row, as in the application's.
    harness.add(
        runner,
        "fpeg parse and transform to ast, positions stamped natively",
        lambda: checked(fix=False),
        PYTHON_FILE,
    )
    harness.add(
        runner,
        "fpeg end to end: parse, transform, fix locations",
        lambda: checked(fix=True),
        PYTHON_FILE,
    )


if __name__ == "__main__":
    main()
