# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Train the profile of a wheel built with profile-guided optimization.

maturin's ``--pgo`` installs the instrumented module in a fresh environment and runs this script
from the root of the repository, and the counts it leaves steer the optimized build. The work is
what parses of users resemble: grammars loaded, the samples of every grammar parsed, broken ones
with recovery too, and larger texts turned into Python values by builders and by methods. It
needs nothing but the standard library and the repository.
"""

import csv
import io
import json
import random
import sys
from pathlib import Path
from typing import Any

import fpeg
import fpeg.python

ROOT = Path(__file__).resolve().parent.parent
GRAMMARS = ROOT / "crates" / "fpeg" / "grammars"
SAMPLES = GRAMMARS / "samples"
# How often each text is parsed, so that parsing outweighs loading in the counts.
ROUNDS = 20
# Sizes of the generated texts, about those of the benchmarks.
JSON_RECORDS = 20_000
CSV_ROWS = 20_000
# fpeg.python builds the ast of Python 3.15, which an older Python lacks.
AST_PYTHON = (3, 15)


class ToPython(fpeg.Transformer):
    """JSON to Python objects by methods, so that the counts see calls into Python."""

    def object(self, *members: tuple[str, Any]) -> dict[str, Any]:
        """Return the dictionary of an object."""
        return dict(members)

    def member(self, key: str, value: Any) -> tuple[str, Any]:
        """Return the pair of a member."""
        return key, value

    def array(self, *values: Any) -> list[Any]:
        """Return the list of an array."""
        return list(values)


class Rows(fpeg.Transformer):
    """CSV to lists of strings, the way the tests read it."""

    def start(self, *records: list[str]) -> list[list[str]]:
        """Return the records of a file."""
        return list(records)

    def field(self, value: fpeg.Token | None) -> str:
        """Return the text of a field, unquoted."""
        if value is None:
            return ""
        if value.type == "QUOTED":
            return value[1:-1].replace('""', '"')
        return str(value)


def samples() -> None:
    """Parse the samples of every grammar, and the broken ones with recovery as well."""
    for folder in sorted(path for path in SAMPLES.iterdir() if path.is_dir()):
        grammar = fpeg.Grammar.open(GRAMMARS / f"{folder.name}.fpeg")
        for path in sorted(folder.iterdir()):
            if path.is_file() and path.suffix != ".tree":
                text = path.read_text(encoding="utf-8")
                tree = grammar.parse(text)
                for _ in range(ROUNDS - 1):
                    grammar.parse(text)
                fpeg.Transformer().transform(tree)
                if isinstance(tree, fpeg.Tree):
                    tree.pretty()
        for path in sorted((folder / "invalid").glob("*")):
            text = path.read_text(encoding="utf-8")
            for recover in (False, True):
                try:
                    grammar.parse(text, recover=recover)
                except fpeg.ParseError as error:
                    error.render()


def values() -> None:
    """Turn JSON and CSV into Python objects, by builders and by methods."""
    rng = random.Random(0)
    records = [
        {
            "id": n,
            "name": f"item {rng.randrange(10**6)}",
            "price": round(rng.uniform(0, 100), 2),
            "tags": [rng.choice("abcdef") for _ in range(rng.randrange(4))],
            "active": rng.random() < 0.5,
            "note": None,
        }
        for n in range(JSON_RECORDS)
    ]
    text = json.dumps(records, indent=1)
    grammar = fpeg.Grammar('%extends "json"')
    tree = grammar.parse(text)
    fpeg.Transformer().transform(tree)
    ToPython().transform(tree)
    out = io.StringIO(newline="")
    writer = csv.writer(out)
    for n in range(CSV_ROWS):
        writer.writerow([n, f"name {n}", f'say "hi", {n}', ""])
    Rows().transform(fpeg.Grammar.open(GRAMMARS / "csv.fpeg").parse(out.getvalue()))


def python_source() -> None:
    """Parse the Python files of the repository, and build their ast where the module can."""
    for path in sorted(ROOT.glob("python/fpeg/*.py")) + sorted(ROOT.glob("tests/*.py")):
        text = path.read_text(encoding="utf-8")
        fpeg.python.GRAMMAR.parse(text)
        if sys.version_info >= AST_PYTHON:
            fpeg.python.parse(text)


def main() -> None:
    """Run every workload."""
    samples()
    values()
    python_source()


if __name__ == "__main__":
    main()
