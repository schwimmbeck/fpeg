# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The tiny language, evaluated from its tree.

An Interpreter walks the tree of a program and computes its result. Each program in the
samples states the result it must give.

A program computes with 64-bit integers that wrap around. Division rounds towards zero and a
remainder has the sign of its dividend. A comparison and a logical operator give 1 or 0, and
anything but 0 counts as true. The statements outside functions are the program, and its
`return` gives the result. A function sees its parameters and what it declares, a block has
variables of its own. Strings are left out.
"""

import operator
import re
from collections.abc import Callable
from pathlib import Path
from typing import Any, NamedTuple

import fpeg
import pytest

SAMPLES = Path(__file__).parent.parent / "crates" / "fpeg" / "grammars" / "samples" / "tiny"
TINY = fpeg.Grammar.open(SAMPLES.parent.parent / "tiny.fpeg")
RESULT = re.compile(r"// result: (-?\d+)\n")

# Python reserves the names of these rules, so the methods for them carry an underscore and
# get their rule's name after the class.
RESERVED = ("if", "while", "return")

COMPARE: dict[str, Callable[[int, int], bool]] = {
    "==": operator.eq,
    "!=": operator.ne,
    "<": operator.lt,
    "<=": operator.le,
    ">": operator.gt,
    ">=": operator.ge,
}


def programs() -> list[tuple[str, fpeg.Tree, int]]:
    """The samples that state a result, each with its tree and that result."""
    found = []
    for path in sorted(SAMPLES.glob("*.tiny")):
        text = path.read_text(encoding="utf-8")
        stated = RESULT.match(text)
        tree = TINY.parse(text)
        if stated and isinstance(tree, fpeg.Tree):
            found.append((path.name, tree, int(stated[1])))
    return found


def wrap(value: int) -> int:
    """What 64 bits keep of an integer, read as a signed number."""
    return (value + 2**63) % 2**64 - 2**63


def divide(a: int, b: int) -> int:
    """The quotient rounded towards zero, as the machine divides."""
    quotient = abs(a) // abs(b)
    return -quotient if (a < 0) != (b < 0) else quotient


class Returned(NamedTuple):
    """What a statement gives when a `return` ran in it."""

    value: int


class Run(fpeg.Interpreter):
    """Evaluates a program. A statement gives `Returned` to end its function, else `None`."""

    def __init__(self) -> None:
        self.functions: dict[str, fpeg.Tree] = {}
        self.scopes: list[dict[str, int]] = []

    def start(self, items: list[fpeg.Tree]) -> int:
        functions = [item for item in items if item.data == "function"]
        self.functions = {str(function.field("name")): function for function in functions}
        returned = self.run([item for item in items if item.data != "function"])
        return returned.value if returned else 0

    def run(self, statements: list[fpeg.Tree]) -> Returned | None:
        """Run statements with variables of their own, up to a `return`."""
        self.scopes.append({})
        try:
            for statement in statements:
                returned = self.visit(statement)
                if returned is not None:
                    return returned
            return None
        finally:
            self.scopes.pop()

    def scope(self, name: fpeg.Token) -> dict[str, int]:
        """The innermost scope that declares a variable."""
        for scope in reversed(self.scopes):
            if name in scope:
                return scope
        raise NameError(f"{name} is not declared")

    def block(self, statements: list[fpeg.Tree]) -> Returned | None:
        return self.run(statements)

    def let(self, name: fpeg.Token, value: Any) -> None:
        self.scopes[-1][str(name)] = self.visit(value)

    def assign(self, target: fpeg.Token, value: Any) -> None:
        self.scope(target)[str(target)] = self.visit(value)

    def if_(self, test: Any, then: fpeg.Tree, otherwise: fpeg.Tree | None) -> Returned | None:
        if self.visit(test):
            return self.visit(then)
        return None if otherwise is None else self.visit(otherwise)

    def while_(self, test: Any, body: fpeg.Tree) -> Returned | None:
        while self.visit(test):
            returned = self.visit(body)
            if returned is not None:
                return returned
        return None

    def return_(self, value: Any) -> Returned:
        return Returned(0 if value is None else self.visit(value))

    def expression_statement(self, expr: Any) -> None:
        self.visit(expr)

    def binary(self, left: Any, op: str, right: Any) -> int:
        # The right side of `and` and `or` is visited only where it decides.
        if op == "and":
            return int(bool(self.visit(left)) and bool(self.visit(right)))
        if op == "or":
            return int(bool(self.visit(left)) or bool(self.visit(right)))
        a, b = self.visit(left), self.visit(right)
        if op == "+":
            return wrap(a + b)
        if op == "-":
            return wrap(a - b)
        if op == "*":
            return wrap(a * b)
        return wrap(divide(a, b) if op == "/" else a - b * divide(a, b))

    def compare(self, left: Any, op: str, right: Any) -> int:
        return int(COMPARE[op](self.visit(left), self.visit(right)))

    def unary(self, op: str, operand: Any) -> int:
        value = self.visit(operand)
        return int(not value) if op == "not" else wrap(-value)

    def call(self, func: Any, args: list[Any]) -> int:
        function = self.functions[str(func)]
        names = [str(name) for name in function.field("params")]
        values = [self.visit(arg) for arg in args]
        if len(names) != len(values):
            raise TypeError(f"{func} takes {len(names)} arguments")
        # A function sees its parameters, and nothing of where it is called.
        outer, self.scopes = self.scopes, [dict(zip(names, values, strict=True))]
        try:
            returned = self.visit(function.field("body"))
        finally:
            self.scopes = outer
        return returned.value if returned else 0

    def true(self) -> int:
        return 1

    def false(self) -> int:
        return 0

    def NUMBER(self, token: fpeg.Token) -> int:
        return wrap(int(token))

    def NAME(self, token: fpeg.Token) -> int:
        return self.scope(token)[str(token)]


for reserved in RESERVED:
    setattr(Run, reserved, getattr(Run, f"{reserved}_"))


PROGRAMS = programs()
each_program = pytest.mark.parametrize(
    ("tree", "expected"),
    [(tree, expected) for _, tree, expected in PROGRAMS],
    ids=[name for name, _, _ in PROGRAMS],
)


def test_the_samples_hold_programs_that_state_their_results() -> None:
    assert len(PROGRAMS) >= 7, [name for name, _, _ in PROGRAMS]


@each_program
def test_an_evaluated_program_gives_the_result_it_states(tree: fpeg.Tree, expected: int) -> None:
    assert Run().visit(tree) == expected
