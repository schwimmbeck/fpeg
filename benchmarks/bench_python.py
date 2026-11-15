# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Python source: fpeg beside CPython, tree-sitter, LibCST and Lark on one real source file.

Run through ``run.py``, or directly with pyperf options.
"""

import _symtable
import ast
import inspect
from collections.abc import Callable
from functools import partial
from typing import Any

import fpeg
import harness
import libcst
import pyperf
import tree_sitter_python
from corpus import PYTHON_FILE
from fpeg import python
from lark import Lark
from lark.indenter import PythonIndenter
from tree_sitter import Language, Parser, Tree

# Lark's Python grammar expects the file to end with a newline token.
LARK_FINAL_NEWLINE = "\n"


def positioned(value: Any, line: int, column: int, end_line: int, end_column: int) -> Any:
    """Give an `ast` node without a position the one of its node or token, columns from 1."""
    if isinstance(value, ast.AST) and not hasattr(value, "lineno"):
        value.lineno, value.col_offset = line, column - 1
        value.end_lineno, value.end_col_offset = end_line, end_column - 1
    return value


def from_meta(function: Callable[..., Any], set_positions: bool) -> Callable[..., Any]:
    """Wrap a node's method to take the node's meta, which positions its value if asked."""
    signature = inspect.signature(function)
    extra = inspect.Parameter("meta", inspect.Parameter.KEYWORD_ONLY)

    def method(self: Any, *args: Any, meta: Any, **kwargs: Any) -> Any:
        value = function(self, *args, **kwargs)
        if set_positions:
            return positioned(value, meta.line, meta.column, meta.end_line, meta.end_column)
        return value

    # The Transformer reads the signature to know what to pass.
    method.__signature__ = signature.replace(parameters=[*signature.parameters.values(), extra])
    return method


def from_token(function: Callable[..., Any], set_positions: bool) -> Callable[..., Any]:
    """Wrap a terminal's method to position its value from the token if asked."""

    def method(self: Any, token: fpeg.Token) -> Any:
        value = function(self, token)
        if set_positions:
            return positioned(value, token.line, token.column, token.end_line, token.end_column)
        return value

    return method


def wrapped(base: type, set_positions: bool) -> type:
    """Make a subclass of a transformer to `ast` with each method in such a wrapper."""
    methods = {}
    for name, function in vars(python.ToAstBase).items():
        if inspect.isfunction(function) and not name.startswith("_"):
            wrap = from_token if name.isupper() else from_meta
            methods[name] = wrap(function, set_positions)
    return type(f"{base.__name__}Wrapped", (base,), methods)


# Positions set in Python, as without native stamping. Columns count code points, which are
# the bytes `ast` counts on the ASCII benchmark file.
ToAstFromMeta = wrapped(python.ToAstBase, set_positions=True)
# The same wrappers around a transformer that stamps natively, so that the difference to it
# is the setting of positions in Python alone.
ToAstInWrappers = wrapped(python.ToAst, set_positions=False)


def add_fpeg(runner: pyperf.Runner, text: str) -> None:
    """Register the benchmarks of fpeg's Python grammar."""
    expected = ast.dump(ast.parse(text), include_attributes=True)

    def checked(transformer: fpeg.Transformer) -> Callable[[], object]:
        def to_ast() -> object:
            return transformer.transform(python.GRAMMAR.parse(text))

        if ast.dump(to_ast(), include_attributes=True) != expected:
            raise RuntimeError("the transformer does not give the tree of ast.parse")
        return to_ast

    harness.add(
        runner,
        "fpeg Python grammar load",
        lambda: partial(fpeg.Grammar, '%extends "python"'),
    )
    harness.add(
        runner, "fpeg parse, tree", lambda: partial(python.GRAMMAR.parse, text), PYTHON_FILE
    )
    harness.add(
        runner,
        "fpeg parse and transform to ast, positions stamped natively",
        lambda: checked(python.ToAst()),
        PYTHON_FILE,
    )
    harness.add(
        runner,
        "fpeg parse and transform to ast, stamped natively, each method in a wrapper",
        lambda: checked(ToAstInWrappers()),
        PYTHON_FILE,
    )
    harness.add(
        runner,
        "fpeg parse and transform to ast, positions set in Python by that wrapper",
        lambda: checked(ToAstFromMeta()),
        PYTHON_FILE,
    )


def walk(tree: Tree, touch: int) -> int:
    """Visit every node with a cursor and return the count.

    ``touch`` 1 creates a Python ``Node`` per node, 2 also reads the text of every leaf.
    """
    cursor = tree.walk()
    count = 0
    while True:
        count += 1
        if touch:
            node = cursor.node
            if touch > 1 and node is not None and node.child_count == 0:
                _ = node.text
        if cursor.goto_first_child():
            continue
        while not cursor.goto_next_sibling():
            if not cursor.goto_parent():
                return count


def add_tree_sitter(runner: pyperf.Runner, source: bytes) -> None:
    """Register the tree-sitter benchmarks."""
    parser = Parser(Language(tree_sitter_python.language()))

    def checked_tree() -> Tree:
        tree = parser.parse(source)
        # tree-sitter returns a tree with error nodes instead of failing.
        if tree.root_node.has_error:
            raise RuntimeError("tree-sitter found errors in the input")
        return tree

    def parse():
        checked_tree()
        return partial(parser.parse, source)

    def walker(touch: int):
        return lambda: partial(walk, checked_tree(), touch)

    harness.add(runner, "tree-sitter parse, native tree", parse, PYTHON_FILE)
    harness.add(runner, "tree-sitter cursor walk over every node", walker(0))
    harness.add(runner, "tree-sitter walk, a Python Node per node", walker(1))
    harness.add(runner, "tree-sitter walk, a Node per node and the text of every leaf", walker(2))


def lark_parser() -> Lark:
    """Load Lark's bundled Python grammar for LALR."""
    return Lark.open_from_package(
        "lark",
        "python.lark",
        ["grammars"],
        parser="lalr",
        postlex=PythonIndenter(),
        start="file_input",
    )


def main() -> None:
    """Register and run the benchmarks."""
    runner = harness.runner()
    text = PYTHON_FILE.read()
    source = text.encode("utf-8")

    harness.add(runner, "ast.parse", lambda: partial(ast.parse, text), PYTHON_FILE)
    add_fpeg(runner, text)
    harness.add(
        runner,
        "CPython parser and symbol table, no AST objects",
        lambda: partial(_symtable.symtable, text, "benchmark", "exec"),
        PYTHON_FILE,
    )
    harness.add(
        runner,
        "compile to bytecode",
        lambda: partial(compile, text, "benchmark", "exec"),
        PYTHON_FILE,
    )
    add_tree_sitter(runner, source)
    harness.add(
        runner, "LibCST parse_module", lambda: partial(libcst.parse_module, text), PYTHON_FILE
    )
    harness.add(runner, "Lark Python grammar load", lambda: lark_parser)

    def lark_tree():
        parser = lark_parser()
        return partial(parser.parse, text + LARK_FINAL_NEWLINE)

    harness.add(runner, "Lark LALR, bundled Python grammar, tree", lark_tree, PYTHON_FILE)


if __name__ == "__main__":
    main()
