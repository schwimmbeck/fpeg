# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-FileCopyrightText: 2017 Erez Shinan
# SPDX-License-Identifier: MIT

"""A calculator: fpeg beside `eval`, Lark and pe, on lines of arithmetic.

Every line holds one expression of `+ - * / **`, signs and parentheses, in Python's order of
operators, and each library computes its number. The rows that give numbers are checked
against `eval` once. The Lark grammar follows the calculator of Lark's examples. Run through
``run.py``, or directly with pyperf options.
"""

import operator
from collections.abc import Callable
from typing import Any

import fpeg
import harness
import pe
import pyperf
from corpus import CALCULATOR
from lark import Lark, Transformer, v_args
from pe.actions import Capture, Pack
from pe.operators import Class, Star

OPERATORS: dict[str, Callable[[Any, Any], Any]] = {
    "+": operator.add,
    "-": operator.sub,
    "*": operator.mul,
    "/": operator.truediv,
    "**": operator.pow,
}

FPEG_GRAMMAR = r"""
start: (expr _NL)*

expr: %precedence {
    sum: left=sum op=("+" | "-") right=product -> binary
    product: left=product op=("*" | "/") right=unary -> binary
    unary: "-" operand=unary -> negate
    power: left=atom op="**" right=unary -> binary
    atom: NUMBER | "(" expr ")"
}

NUMBER: [0-9]+ ("." [0-9]+)? => float
_NL: "\n"
%ignore " "+
"""


class FpegCalculate(fpeg.Transformer):
    """Computes the numbers of the lines. The grammar's builder makes the floats."""

    def start(self, *values: float) -> list[float]:
        """Make the list of the lines' numbers."""
        return list(values)

    def binary(self, left: Any, op: str, right: Any) -> Any:
        """Apply an operator."""
        return OPERATORS[op](left, right)

    def negate(self, operand: Any) -> Any:
        """Change the sign."""
        return -operand


# After Lark's examples/calc.py, with a power, and lines in place of assignments.
LARK_GRAMMAR = r"""
start: (sum _NL)*
?sum: product
    | sum "+" product -> add
    | sum "-" product -> sub
?product: unary
    | product "*" unary -> mul
    | product "/" unary -> div
?unary: power
    | "-" unary -> neg
?power: atom
    | atom "**" unary -> pow
?atom: NUMBER -> number
    | "(" sum ")"
_NL: "\n"
%import common.NUMBER
%ignore " "
"""


@v_args(inline=True)
class LarkCalculate(Transformer):
    """Lark transformer that computes while it parses, as the example does."""

    from operator import add, mul, neg, pow, sub
    from operator import truediv as div

    number = float

    def start(self, *values: float) -> list[float]:
        """Make the list of the lines' numbers."""
        return list(values)


PE_GRAMMAR = r"""
    Start   <- Line* EOF
    Line    <  Sum NL
    Sum     <  Product (SumOp Product)*
    Product <  Unary (ProdOp Unary)*
    Unary   <  Minus Unary / Power
    Power   <  Atom (POW Unary)?
    Atom    <  Number / "(" Sum ")"
    SumOp   <- ~[-+]
    ProdOp  <- ~("*" !"*" / "/")
    Minus   <- ~"-"
    Number  <- ~([0-9]+ ("." [0-9]+)?)
    POW     <- "**"
    NL      <- "\n"
    EOF     <- !.
"""


def fold(values: list[Any]) -> Any:
    """Apply the operators of a chain from the left: a value, then operator and value in turn."""
    result = values[0]
    for at in range(1, len(values), 2):
        result = OPERATORS[values[at]](result, values[at + 1])
    return result


PE_ACTIONS = {
    "Start": Pack(list),
    "Sum": Pack(fold),
    "Product": Pack(fold),
    "Unary": Pack(lambda values: -values[1] if len(values) == 2 else values[0]),
    "Power": Pack(lambda values: values[0] ** values[1] if len(values) == 2 else values[0]),
    "Number": Capture(float),
}


def evaluate(text: str) -> list[float]:
    """Compute every line with Python's own parser and interpreter."""
    return [eval(line) for line in text.splitlines()]


def add_fpeg(runner: pyperf.Runner, text: str, expected: list[float]) -> None:
    """Register the fpeg benchmarks."""

    def tree():
        grammar = fpeg.Grammar(FPEG_GRAMMAR)
        return lambda: grammar.parse(text)

    def numbers():
        grammar = fpeg.Grammar(FPEG_GRAMMAR)
        calculate = FpegCalculate()
        if calculate.transform(grammar.parse(text)) != expected:
            raise RuntimeError("fpeg does not give the numbers of eval")
        return lambda: calculate.transform(grammar.parse(text))

    harness.add(runner, "fpeg parse, tree", tree, CALCULATOR)
    harness.add(runner, "fpeg parse and Transformer, numbers", numbers, CALCULATOR)


def add_lark(runner: pyperf.Runner, text: str, expected: list[float]) -> None:
    """Register the Lark benchmarks."""

    def tree():
        parser = Lark(LARK_GRAMMAR, parser="lalr")
        return lambda: parser.parse(text)

    def numbers():
        parser = Lark(LARK_GRAMMAR, parser="lalr", transformer=LarkCalculate())
        if parser.parse(text) != expected:
            raise RuntimeError("Lark does not give the numbers of eval")
        return lambda: parser.parse(text)

    harness.add(runner, "Lark LALR, tree", tree, CALCULATOR)
    harness.add(runner, "Lark LALR with inlined Transformer, numbers", numbers, CALCULATOR)


def add_pe(runner: pyperf.Runner, text: str, expected: list[float], engine: str) -> None:
    """Register the benchmark of one pe engine."""

    def numbers():
        parser = pe.compile(PE_GRAMMAR, actions=PE_ACTIONS, parser=engine, ignore=Star(Class(" ")))
        match = parser.match(text)
        if match is None or match.value() != expected:
            raise RuntimeError(f"pe {engine} does not give the numbers of eval")
        return lambda: parser.match(text).value()

    harness.add(runner, f"pe {engine}, numbers", numbers, CALCULATOR)


def main() -> None:
    """Register and run the benchmarks."""
    runner = harness.runner()
    text = CALCULATOR.read()
    expected = evaluate(text)
    harness.add(runner, "eval, a line at a time", lambda: lambda: evaluate(text), CALCULATOR)
    add_fpeg(runner, text, expected)
    add_lark(runner, text, expected)
    add_pe(runner, text, expected, "machine")
    add_pe(runner, text, expected, "packrat")


if __name__ == "__main__":
    main()
