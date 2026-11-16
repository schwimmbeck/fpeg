# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The positions that a transformer stamps on the values it gives."""

import time
from collections.abc import Iterator
from typing import Any

import fpeg
import pytest

SUMS = fpeg.Grammar(r"""
start: expr
expr: left=atom "+" right=atom -> add | value=atom
atom: name=NAME | "(" inner=expr ")"
NAME: [a-zé\u{1D51E}]+
%ignore " "+
""")


class ToAst(fpeg.Transformer, positions="ast"):
    def start(self, children: list[Any]) -> Any:
        return children[0]

    def add(self, left: Any, right: Any) -> Any:
        import ast

        return ast.BinOp(left, ast.Add(), right)

    def expr(self, value: Any) -> Any:
        return value

    def atom(self, name: Any, inner: Any) -> Any:
        return name if name is not None else inner

    def NAME(self, token: fpeg.Token) -> Any:
        import ast

        return ast.Name(str(token), ast.Load())


def test_positions_ast_gives_values_the_positions_python_s_ast_has() -> None:
    import ast

    # Columns count UTF-8 bytes, and the parenthesized name keeps its own position.
    text = "é + (b)"
    mine = ToAst().transform(SUMS.parse(text))
    theirs = ast.parse(text, mode="eval").body
    assert ast.dump(mine, include_attributes=True) == ast.dump(theirs, include_attributes=True)


def test_a_subclass_keeps_the_positions_of_its_base() -> None:
    class Again(ToAst):
        pass

    assert Again().transform(SUMS.parse("b")).end_col_offset == 1


# Names on the long line, enough that counting each column from the line's start takes seconds.
LONG_LINE_NAMES = 20_000
# Letters of each name on that line.
LONG_NAME_CHARS = 40
# Time the transform of that line may take, about ten times what it needs.
LONG_LINE_SECONDS = 0.5


class Spot:
    line: int
    column: int
    end_line: int
    end_column: int


@pytest.mark.parametrize(("unit", "column"), [("code points", 5), ("utf-8", 8), ("utf-16", 6)])
def test_a_positions_record_names_the_attributes_the_units_and_the_classes(
    unit: Any, column: int
) -> None:
    record = fpeg.Positions("line", "column", "end_line", "end_column", Spot, 0, 1, unit)

    class Spots(fpeg.Transformer, positions=record):
        def start(self, children: list[Any]) -> Any:
            return children[0]

        def NAME(self, token: fpeg.Token) -> Any:
            return Spot() if token == "b" else str(token)

        def atom(self, name: Any, inner: Any) -> Any:
            return name

        def add(self, left: Any, right: Any) -> Any:
            return (left, right)

    fraktur_a = "\U0001d51e"
    first, second = Spots().transform(SUMS.parse(fraktur_a + " + b"))
    # A value of no class the record names stays as it is, and the first letter, beyond the
    # Basic Multilingual Plane, takes one code point, four UTF-8 bytes or two UTF-16 units.
    assert first == fraktur_a
    assert (second.line, second.column, second.end_line, second.end_column) == (
        0,
        column,
        0,
        column + 1,
    )


def test_positions_reach_what_builders_defaults_and_generators_give() -> None:
    grammar = fpeg.Grammar(
        'start: NAME pair\npair: "(" NAME ")"\nNAME: [a-z]+ => spot\n%ignore " "+',
        functions={"spot": lambda text: Spot()},
    )
    record = fpeg.Positions("line", "column", "end_line", "end_column", Spot)

    class Spots(fpeg.Transformer, positions=record):
        def __default__(self, data: Any, children: list[Any], meta: Any) -> Spot:
            return Spot()

        def start(self, *children: Spot) -> Iterator[Spot]:
            yield from children
            yield Spot()

    built, defaulted, yielded = Spots().transform(grammar.parse("ab (cd)"))
    spans = [(spot.column, spot.end_column) for spot in (built, defaulted, yielded)]
    assert spans == [(0, 2), (3, 7), (0, 7)]


def test_utf16_columns_take_no_longer_on_one_long_line() -> None:
    names = fpeg.Grammar('start: NAME*\nNAME: [a-z\\u{1D51E}]+\n%ignore " "+')
    record = fpeg.Positions("line", "column", "end_line", "end_column", Spot, 0, 0, "utf-16")

    class Spots(fpeg.Transformer, positions=record):
        def NAME(self, token: fpeg.Token) -> Any:
            return Spot()

    tree = names.parse("\U0001d51e " + ("b" * LONG_NAME_CHARS + " ") * LONG_LINE_NAMES)
    begin = time.perf_counter()
    spots = Spots().transform(tree).children
    # Encoding the line from its start again at every value takes seconds.
    assert time.perf_counter() - begin < LONG_LINE_SECONDS
    width = LONG_NAME_CHARS + 1
    assert (spots[1].column, spots[-1].end_column) == (3, 2 + LONG_LINE_NAMES * width)


def test_a_positions_record_refuses_what_it_cannot_stamp() -> None:
    names = ("line", "column", "end_line", "end_column")
    number: Any = 3
    with pytest.raises(TypeError, match="are names"):
        fpeg.Positions("line", "column", "end_line", number, Spot)
    with pytest.raises(ValueError, match="four names"):
        fpeg.Positions("line", "line", "end_line", "end_column", Spot)
    listed: Any = [Spot]
    with pytest.raises(TypeError, match="a class or a tuple"):
        fpeg.Positions(*names, listed)
    with pytest.raises(ValueError, match="at most"):
        fpeg.Positions(*names, Spot, line_base=2**63 - 1)
    unit: Any = "UTF-8"
    with pytest.raises(ValueError, match="column_unit"):
        fpeg.Positions(*names, Spot, column_unit=unit)
    assert fpeg.Positions(*names, Spot).types == (Spot,)


def test_positions_take_ast_or_a_record() -> None:
    with pytest.raises(TypeError, match="positions takes 'ast' or a Positions"):

        class Wrong(fpeg.Transformer, positions="lsp"):  # type: ignore[call-arg]  # the wrong value is the test
            pass
