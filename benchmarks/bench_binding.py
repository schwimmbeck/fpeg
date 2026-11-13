# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The cost of fpeg's Python binding apart from parsing.

Each number is the difference of two rows that differ in one thing, divided by the items it
concerns. A tree of the first records of the JSON file, parsed once, is transformed with no
method, which is the scan alone, and with a trivial method on every node, taking its children
as a list or as arguments, then with members by field. A trivial method on every token taking
the token, against one taking nothing, gives what making a token costs. Both return `Discard`,
which removes every token and so rebuilds every node, alike in both. A parse with a walk over all
its views, against the parse alone, gives what views cost. Beside them: many small CSV parses one
call each against `csv.reader`. Run through ``run.py``, or directly with pyperf options.
"""

import csv
import io
import json
import re
from functools import partial
from pathlib import Path
from typing import Any

import fpeg
import harness
import pyperf
from corpus import RECORDS_JSON

GRAMMARS = Path(__file__).resolve().parent.parent / "crates" / "fpeg" / "grammars"
# Records of the JSON file in the tree, enough for stable numbers in a fraction of a second.
JSON_RECORDS = 1000
# Short CSV records parsed one call each.
CSV_RECORDS = 2000


def json_without_builders() -> fpeg.Grammar:
    """Load the JSON grammar without its builders, so that a transform runs only the methods."""
    text = (GRAMMARS / "json.fpeg").read_text(encoding="utf-8")
    return fpeg.Grammar(re.sub(r" => \w+", "", text))


class NodeLists(fpeg.Transformer):
    """A trivial method on every node, taking the list of its children."""

    def object(self, children: list[Any]) -> None:
        """Do nothing."""

    def member(self, children: list[Any]) -> None:
        """Do nothing."""

    def array(self, children: list[Any]) -> None:
        """Do nothing."""

    def value(self, children: list[Any]) -> None:
        """Do nothing."""


class Nodes(fpeg.Transformer):
    """A trivial method on every node but a member, taking its children as arguments."""

    def object(self, *children: Any) -> None:
        """Do nothing."""

    def array(self, *children: Any) -> None:
        """Do nothing."""

    def value(self, *children: Any) -> None:
        """Do nothing."""


class NodeArguments(Nodes):
    """A member taking its children as arguments too."""

    def member(self, *children: Any) -> None:
        """Do nothing."""


class NodeFields(Nodes):
    """A member taking its fields, the only difference to `NodeArguments`."""

    def member(self, key: Any, value: Any) -> None:
        """Do nothing."""


class Tokens(fpeg.Transformer):
    """A trivial method on every token, taking the token, which the scan makes for it."""

    def STRING(self, token: fpeg.Token) -> object:
        """Remove the token."""
        return fpeg.Discard

    def INT(self, token: fpeg.Token) -> object:
        """Remove the token."""
        return fpeg.Discard

    def FLOAT(self, token: fpeg.Token) -> object:
        """Remove the token."""
        return fpeg.Discard


class TokenCalls(fpeg.Transformer):
    """A trivial method on every token, taking nothing, so that the scan makes no token."""

    def STRING(self) -> object:
        """Remove the token."""
        return fpeg.Discard

    def INT(self) -> object:
        """Remove the token."""
        return fpeg.Discard

    def FLOAT(self) -> object:
        """Remove the token."""
        return fpeg.Discard


class CsvRows(fpeg.Transformer):
    """Records as lists of strings, as `csv.reader` gives them."""

    def start(self, *records: list[str]) -> list[list[str]]:
        """Make the list of records."""
        return list(records)

    def field(self, value: fpeg.Token | None) -> str:
        """Unquote a field."""
        if value is None:
            return ""
        if value.type == "QUOTED":
            return value[1:-1].replace('""', '"')
        return str(value)


def walk_views(grammar: fpeg.Grammar, text: str) -> int:
    """Parse, make the views of every node and the tokens of the parse, and count the nodes."""
    tree = grammar.parse(text)
    if not isinstance(tree, fpeg.Tree):
        raise RuntimeError("the records parse to a token")
    # A view keeps its children once made, so only a fresh parse makes them again.
    return len(list(tree.iter_subtrees()))


def add_transforms(runner: pyperf.Runner, text: str) -> None:
    """Register the transforms of a tree of JSON records, and its parse with and without views."""
    grammar = json_without_builders()
    records = json.dumps(json.loads(text)[:JSON_RECORDS])

    def transform(transformer: fpeg.Transformer):
        def setup():
            tree = grammar.parse(records)
            return lambda: transformer.transform(tree)

        return setup

    harness.add(runner, "transform, no method", transform(fpeg.Transformer()))
    harness.add(runner, "transform, a method on every node, list", transform(NodeLists()))
    harness.add(runner, "transform, a method on every node, *children", transform(NodeArguments()))
    harness.add(runner, "transform, the same with a member by field", transform(NodeFields()))
    harness.add(
        runner, "transform, a method on every token, taking nothing", transform(TokenCalls())
    )
    harness.add(runner, "transform, a method on every token, taking it", transform(Tokens()))
    harness.add(runner, "parse", lambda: partial(grammar.parse, records))
    harness.add(
        runner, "parse, then every view and token", lambda: partial(walk_views, grammar, records)
    )


def add_small_parses(runner: pyperf.Runner) -> None:
    """Register many parses of one short CSV record each, with fpeg and with csv.reader."""
    grammar = fpeg.Grammar.open(GRAMMARS / "csv.fpeg")
    lines = [f'name{i},{i},"a ""quoted"" note"\n' for i in range(CSV_RECORDS)]
    transformer = CsvRows()
    expected = [next(csv.reader(io.StringIO(line, newline=""))) for line in lines]
    if [transformer.transform(grammar.parse(line))[0] for line in lines] != expected:
        raise RuntimeError("fpeg does not read the CSV records as csv does")

    def fpeg_calls():
        return lambda: [transformer.transform(grammar.parse(line)) for line in lines]

    def csv_calls():
        return lambda: [next(csv.reader(io.StringIO(line, newline=""))) for line in lines]

    harness.add(runner, f"{CSV_RECORDS} small CSV parses with a transformer", fpeg_calls)
    harness.add(runner, f"{CSV_RECORDS} small CSV parses with csv.reader", csv_calls)


def main() -> None:
    """Register and run the benchmarks."""
    runner = harness.runner()
    add_transforms(runner, RECORDS_JSON.read())
    add_small_parses(runner)


if __name__ == "__main__":
    main()
