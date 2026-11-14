# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""A calculator with a precedence block against Python's `eval` of the same text."""

import operator
import random
from collections.abc import Callable
from typing import Any

import fpeg

# Python's own order: a sign binds looser than a power and may stand in its exponent.
CALCULATOR = fpeg.Grammar(r"""
?start: expr

expr: %precedence {
    sum: left=sum op=("+" | "-") right=product -> binary
    product: left=product op=("*" | "/") right=unary -> binary
    unary: "-" operand=unary -> negate
    power: left=atom op="**" right=unary -> binary
    atom: NUMBER | "(" expr ")"
}

NUMBER: [0-9]+ ("." [0-9]+)? => float
%ignore " "+
""")

OPERATORS: dict[str, Callable[[Any, Any], Any]] = {
    "+": operator.add,
    "-": operator.sub,
    "*": operator.mul,
    "/": operator.truediv,
    "**": operator.pow,
}

# Expressions compared in the default run.
EXPRESSIONS = 3000


class Calculate(fpeg.Transformer):
    def binary(self, left: Any, op: str, right: Any) -> Any:
        return OPERATORS[op](left, right)

    def negate(self, operand: Any) -> Any:
        return -operand


def calculate(text: str) -> Any:
    return Calculate().transform(CALCULATOR.parse(text))


def outcome(evaluate: Callable[[str], Any], text: str) -> str:
    """The value as text, which tells nan, the infinities and complex numbers apart, or the
    error that arithmetic on floats raises."""
    try:
        return repr(evaluate(text))
    except (ZeroDivisionError, OverflowError) as error:
        return type(error).__name__


def expression(rng: random.Random, depth: int) -> str:
    """A random expression, with no more parentheses than chance puts in. Every number has a
    decimal point, so that `eval` computes with floats as the calculator does."""
    space = rng.choice(["", " "])
    kind = rng.randrange(10) if depth < 6 else 0
    if kind < 3:
        return f"{rng.randrange(20)}.{rng.randrange(10)}"
    if kind == 3:
        return f"({space}{expression(rng, depth + 1)}{space})"
    if kind == 4:
        return f"-{space}{expression(rng, depth + 1)}"
    op = rng.choice(list(OPERATORS))
    return f"{expression(rng, depth + 1)}{space}{op}{space}{expression(rng, depth + 1)}"


def test_the_calculator_gives_what_eval_gives() -> None:
    rng = random.Random(4)
    outcomes: set[str] = set()
    for _ in range(EXPRESSIONS):
        text = expression(rng, 0)
        expected = outcome(eval, text)
        assert outcome(calculate, text) == expected, text
        outcomes.add(expected if expected.endswith("Error") else "value")
    # The run met results and both errors, so the errors are compared too.
    assert outcomes == {"value", "ZeroDivisionError", "OverflowError"}
