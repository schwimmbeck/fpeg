# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT

"""JSON baseline: the standard library, Lark and pe on one 2.5 MB file.

Run through ``run.py``, or directly with pyperf options.
"""

import json
from functools import partial

import harness
import pe
import pyperf
from corpus import RECORDS_JSON
from lark import Lark, Transformer, v_args
from pe.actions import Capture, Fail, Pack, Pair
from pe.operators import Class, Star

LARK_GRAMMAR = r"""
?start: value
?value: object | array | string
      | SIGNED_NUMBER -> number
      | "true" -> true | "false" -> false | "null" -> null
array  : "[" [value ("," value)*] "]"
object : "{" [pair ("," pair)*] "}"
pair   : string ":" value
string : ESCAPED_STRING
%import common.ESCAPED_STRING
%import common.SIGNED_NUMBER
%import common.WS
%ignore WS
"""


class ToPython(Transformer):
    """Lark transformer from the JSON tree to Python objects."""

    @v_args(inline=True)
    def string(self, s: str) -> str:
        """Unquote a string, enough for this input."""
        return s[1:-1].replace('\\"', '"')

    array = list
    pair = tuple
    object = dict
    number = v_args(inline=True)(float)

    def null(self, _: object) -> None:
        """Return None."""
        return None

    def true(self, _: object) -> bool:
        """Return True."""
        return True

    def false(self, _: object) -> bool:
        """Return False."""
        return False


# Grammar and actions from pe's own examples/json.py.
PE_GRAMMAR = r"""
    Start    <- Value EOF
    Value    <  Object / Array / String / Number / Constant / BADVALUE
    Object   <  "{" (Member ("," Member)*)? BADCOMMA? "}"
    Member   <  String ":" Value
    Array    <  "[" (Value ("," Value)*)? BADCOMMA? "]"
    String   <- ["] ~( (!["\\] .)* ('\\' . / (!["\\] .)+)* ) ["]
    Number   <- "-"? ("0" / [1-9] [0-9]*) ("." [0-9]+)? ([eE] [-+]? [0-9]+)?
    Constant <- ~( "true" / "false" / "null" )
    EOF      <- !.
    BADVALUE <- ![}\]] .
    BADCOMMA <  ',' &[}\]]
"""
PE_ACTIONS = {
    "Object": Pair(dict),
    "Array": Pack(list),
    "Number": Capture(float),
    "Constant": {"true": True, "false": False, "null": None}.__getitem__,
    "BADVALUE": Fail("unexpected JSON value"),
    "BADCOMMA": Fail("trailing commas are not allowed"),
}


def lark_parser(**options: object) -> Lark:
    """Compile the Lark JSON grammar for LALR."""
    return Lark(LARK_GRAMMAR, parser="lalr", lexer="basic", maybe_placeholders=False, **options)


def pe_parser(engine: str, actions: bool) -> pe.Parser:
    """Compile the pe JSON grammar for one of pe's engines."""
    return pe.compile(
        PE_GRAMMAR,
        actions=PE_ACTIONS if actions else None,
        parser=engine,
        ignore=Star(Class("\t\n\f\r ")),
    )


def add_lark(runner: pyperf.Runner, text: str) -> None:
    """Register the Lark benchmarks."""
    harness.add(runner, "Lark grammar load", lambda: lark_parser)

    def tree():
        parser = lark_parser()
        return lambda: parser.parse(text)

    def transform():
        parsed = lark_parser().parse(text)
        return lambda: ToPython().transform(parsed)

    def inline():
        parser = lark_parser(transformer=ToPython())
        return lambda: parser.parse(text)

    harness.add(runner, "Lark LALR, tree", tree, RECORDS_JSON)
    harness.add(runner, "Lark Transformer over the tree", transform, RECORDS_JSON)
    harness.add(runner, "Lark LALR with inlined Transformer, Python objects", inline, RECORDS_JSON)


def add_pe(runner: pyperf.Runner, text: str, engine: str) -> None:
    """Register the benchmarks of one pe engine."""
    harness.add(runner, f"pe {engine} grammar load", lambda: partial(pe_parser, engine, True))

    def match():
        parser = pe_parser(engine, actions=False)
        return lambda: parser.match(text)

    def objects():
        parser = pe_parser(engine, actions=True)
        return lambda: parser.match(text).value()

    harness.add(runner, f"pe {engine}, match only", match, RECORDS_JSON)
    harness.add(runner, f"pe {engine}, Python objects", objects, RECORDS_JSON)


def main() -> None:
    """Register and run the benchmarks."""
    runner = harness.runner()
    text = RECORDS_JSON.read()
    harness.add(runner, "json.loads", lambda: partial(json.loads, text), RECORDS_JSON)
    add_lark(runner, text)
    add_pe(runner, text, "machine")
    add_pe(runner, text, "packrat")


if __name__ == "__main__":
    main()
