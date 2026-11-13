# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Python source baseline: CPython, tree-sitter, LibCST and Lark on one real source file.

Run through ``run.py``, or directly with pyperf options.
"""

import _symtable
import ast
from functools import partial

import harness
import libcst
import pyperf
import tree_sitter_python
from corpus import PYTHON_FILE
from lark import Lark
from lark.indenter import PythonIndenter
from tree_sitter import Language, Parser, Tree

# Lark's Python grammar expects the file to end with a newline token.
LARK_FINAL_NEWLINE = "\n"


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
