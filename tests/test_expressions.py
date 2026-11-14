# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The expression grammar against trees of its own making.

A random syntax tree is printed with only the parentheses that the precedence table of the
language asks for, and the parse of that text must be the tree again. `eval` cannot judge
here: C and Python order the bitwise operators and the comparisons differently.
"""

import random
from pathlib import Path
from typing import Any

import fpeg

GRAMMARS = Path(__file__).parent.parent / "crates" / "fpeg" / "grammars"
EXPRESSIONS = fpeg.Grammar.open(GRAMMARS / "expressions.fpeg")

# The levels of the table from loosest to tightest. A binary operator stands on the level of
# its row, takes an operand of that level on its left and of the next on its right.
BINARY = [
    ["||"],
    ["&&"],
    ["|"],
    ["^"],
    ["&"],
    ["==", "!="],
    ["<=", ">=", "<", ">"],
    ["<<", ">>"],
    ["+", "-"],
    ["*", "/", "%"],
]
ASSIGN, CONDITIONAL, FIRST_BINARY = 0, 1, 2
PREFIX = FIRST_BINARY + len(BINARY)
POSTFIX = PREFIX + 1
PRIMARY = POSTFIX + 1
LEVELS = {"assign": ASSIGN, "conditional": CONDITIONAL, "unary": PREFIX}

# Trees compared in the default run.
TREES = 2000


def level(node: Any) -> int:
    if isinstance(node, str):
        return PRIMARY
    if node[0] == "binary":
        return next(FIRST_BINARY + row for row, ops in enumerate(BINARY) if node[2] in ops)
    return LEVELS.get(node[0], POSTFIX)


def text(node: Any, least: int) -> str:
    """Print a node where an operand of the level `least` or a tighter one may stand. Tokens
    are a space apart, so that `- - x` is not read as `--x`."""
    if isinstance(node, str):
        return node
    kind, *parts = node
    own = level(node)
    if kind == "assign":
        target, op, value = parts
        body = f"{text(target, CONDITIONAL)} {op} {text(value, ASSIGN)}"
    elif kind == "conditional":
        test, then, otherwise = parts
        body = f"{text(test, FIRST_BINARY)} ? {text(then, ASSIGN)} : {text(otherwise, CONDITIONAL)}"
    elif kind == "binary":
        left, op, right = parts
        body = f"{text(left, own)} {op} {text(right, own + 1)}"
    elif kind == "unary":
        op, operand = parts
        body = f"{op} {text(operand, PREFIX)}"
    elif kind == "call":
        func, args = parts
        body = f"{text(func, POSTFIX)} ( {' , '.join(text(arg, ASSIGN) for arg in args)} )"
    elif kind == "index":
        collection, index = parts
        body = f"{text(collection, POSTFIX)} [ {text(index, ASSIGN)} ]"
    elif kind == "member":
        owner, name = parts
        body = f"{text(owner, POSTFIX)} . {name}"
    else:
        operand, op = parts
        body = f"{text(operand, POSTFIX)} {op}"
    return f"( {body} )" if own < least else body


def tree(rng: random.Random, depth: int) -> Any:
    """A random syntax tree, of any shape that the node kinds allow."""

    def child() -> Any:
        return tree(rng, depth + 1)

    kind = rng.randrange(13) if depth < 5 else 0
    if kind < 3:
        return rng.choice(["x", "y", "name_1", "0", "42", "1.5", '"s"', '"a\\"b"'])
    if kind == 3:
        return ("assign", child(), rng.choice(["=", "+=", "-=", "*=", "/="]), child())
    if kind == 4:
        return ("conditional", child(), child(), child())
    if kind == 5:
        return ("unary", rng.choice(["++", "--", "!", "~", "-", "+"]), child())
    if kind == 6:
        return ("call", child(), [child() for _ in range(rng.randrange(3))])
    if kind == 7:
        return ("index", child(), child())
    if kind == 8:
        return ("member", child(), rng.choice(["x", "field"]))
    if kind == 9:
        return ("postfix", child(), rng.choice(["++", "--"]))
    return ("binary", child(), rng.choice(rng.choice(BINARY)), child())


class ToTuples(fpeg.Transformer):
    """Turns a parse into the tuples that `tree` makes."""

    def assign(self, target: Any, op: str, value: Any) -> Any:
        return ("assign", target, str(op), value)

    def conditional(self, test: Any, then: Any, otherwise: Any) -> Any:
        return ("conditional", test, then, otherwise)

    def binary(self, left: Any, op: str, right: Any) -> Any:
        return ("binary", left, str(op), right)

    def unary(self, op: str, operand: Any) -> Any:
        return ("unary", str(op), operand)

    def call(self, func: Any, args: list[Any]) -> Any:
        return ("call", func, args)

    def index(self, *children: Any) -> Any:
        return ("index", *children)

    def member(self, *children: Any) -> Any:
        return ("member", *children)

    def postfix(self, operand: Any, op: str) -> Any:
        return ("postfix", operand, str(op))

    def NUMBER(self, token: fpeg.Token) -> str:
        return str(token)

    def NAME(self, token: fpeg.Token) -> str:
        return str(token)

    def STRING(self, token: fpeg.Token) -> str:
        return str(token)


def kinds(node: Any, found: set[str]) -> None:
    """Add the kinds of the nodes of a tree."""
    if isinstance(node, tuple):
        found.add(node[0])
        for part in node[1:]:
            for item in part if isinstance(part, list) else [part]:
                kinds(item, found)


def test_a_tree_printed_with_the_parentheses_it_needs_parses_to_itself() -> None:
    rng = random.Random(7)
    to_tuples = ToTuples()
    met: set[str] = set()
    for _ in range(TREES):
        made = tree(rng, 0)
        written = text(made, ASSIGN)
        assert to_tuples.transform(EXPRESSIONS.parse(written)) == made, written
        kinds(made, met)
    assert met == {"assign", "conditional", "binary", "unary", "call", "index", "member", "postfix"}
