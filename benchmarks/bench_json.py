# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-FileCopyrightText: 2017 Erez Shinan
# SPDX-FileCopyrightText: 2020 Michael Wayne Goodman
# SPDX-License-Identifier: MIT

"""JSON: fpeg beside the standard library, Lark and pe, on one 2.5 MB file.

The fpeg rows that make Python objects are checked against json.loads. The Lark and pe rows
keep the transformers of those libraries' examples, which leave escapes in strings and make
every number a float. Run through ``run.py``, or directly with pyperf options.
"""

import json
from functools import partial
from json.decoder import scanstring
from pathlib import Path
from typing import Any

import fpeg
import harness
import pe
import pyperf
from corpus import RECORDS_JSON
from lark import Lark, Transformer, v_args
from pe.actions import Capture, Fail, Pack, Pair
from pe.operators import Class, Star

# Grammar and transformer after Lark's examples/json_parser.py.
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
        """Unquote a string as the example does, leaving other escapes in place."""
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


FPEG_JSON = Path(__file__).resolve().parent.parent / "crates" / "fpeg" / "grammars" / "json.fpeg"


class FpegToPython(fpeg.Transformer):
    """A Python method for every rule and terminal, where the grammar's builders need none."""

    def object(self, *members: Any) -> dict[Any, Any]:
        """Make a dict."""
        return dict(members)

    def member(self, key: Any, value: Any) -> tuple[Any, Any]:
        """Make a pair."""
        return (key, value)

    def array(self, *values: Any) -> list[Any]:
        """Make a list."""
        return list(values)

    def value(self, *, node: fpeg.Tree) -> Any:
        """Make true, false or null."""
        return {"true": True, "false": False, "null": None}[node.text or ""]

    def STRING(self, token: fpeg.Token) -> str:
        """Unquote a string with json's own scanner."""
        return scanstring(token, 1)[0]

    INT = int
    FLOAT = float


def add_fpeg(runner: pyperf.Runner, text: str) -> None:
    """Register the fpeg benchmarks, each checked against json.loads once."""
    expected = json.loads(text)
    # From text, as Lark and pe load theirs.
    grammar_text = FPEG_JSON.read_text(encoding="utf-8")
    harness.add(runner, "fpeg grammar load", lambda: partial(fpeg.Grammar, grammar_text))

    def tree():
        grammar = fpeg.Grammar.open(FPEG_JSON)
        return lambda: grammar.parse(text)

    def objects(transformer: fpeg.Transformer):
        def setup():
            grammar = fpeg.Grammar.open(FPEG_JSON)
            if transformer.transform(grammar.parse(text)) != expected:
                raise RuntimeError(f"{type(transformer).__name__} does not give json's values")
            return lambda: transformer.transform(grammar.parse(text))

        return setup

    harness.add(runner, "fpeg parse, tree", tree, RECORDS_JSON)
    harness.add(
        runner,
        "fpeg parse and grammar builders, Python objects",
        objects(fpeg.Transformer()),
        RECORDS_JSON,
    )
    harness.add(
        runner,
        "fpeg parse and Transformer methods, Python objects",
        objects(FpegToPython()),
        RECORDS_JSON,
    )


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
        # pe returns None when the input does not match, which would time nothing useful.
        if parser.match(text) is None:
            raise RuntimeError(f"pe {engine} does not match the input")
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
    add_fpeg(runner, text)
    add_lark(runner, text)
    add_pe(runner, text, "machine")
    add_pe(runner, text, "packrat")


if __name__ == "__main__":
    main()
