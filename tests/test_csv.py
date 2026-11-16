# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The CSV grammar against Python's `csv`, through a transformer with a method by field."""

import csv
import io
import random
from pathlib import Path
from typing import Any

import fpeg
import pytest

SAMPLES = Path(__file__).parent.parent / "crates" / "fpeg" / "grammars" / "samples" / "csv"
CSV = fpeg.Grammar.open(SAMPLES.parent.parent / "csv.fpeg")

# Documents that `csv.writer` writes for the comparison.
DOCUMENTS = 400


class Rows(fpeg.Transformer, strict=True):
    """Records as lists of strings, as `csv.reader` gives them."""

    def start(self, *records: list[str]) -> list[list[str]]:
        return list(records)

    def field(self, value: fpeg.Token | None) -> str:
        if value is None:
            return ""
        if value.type == "QUOTED":
            return value[1:-1].replace('""', '"')
        return str(value)


def rows(text: str) -> Any:
    return Rows().transform(CSV.parse(text))


def reference(text: str) -> list[list[str]]:
    return list(csv.reader(io.StringIO(text, newline=""), strict=True))


def test_the_samples_read_as_csv_reads_them() -> None:
    for path in sorted(SAMPLES.glob("*.csv")):
        text = path.read_text(encoding="utf-8")
        assert rows(text) == reference(text), path


def test_documents_written_by_csv_read_back_as_csv_reads_them() -> None:
    rng = random.Random(20261113)
    pieces = ["a", "b c", ",", '"', '""', "\n", "\r\n", "\r", " ", "é", "x,y", ""]
    for _ in range(DOCUMENTS):
        out = io.StringIO(newline="")
        writer = csv.writer(out, lineterminator=rng.choice(["\r\n", "\n"]))
        for _ in range(rng.randint(0, 5)):
            fields = rng.randint(0, 4)
            row = [
                "".join(rng.choice(pieces) for _ in range(rng.randint(0, 3))) for _ in range(fields)
            ]
            writer.writerow(row)
        text = out.getvalue()
        assert rows(text) == reference(text), repr(text)


def test_what_rfc_4180_forbids_fails_where_csv_is_lenient() -> None:
    # Python's csv takes a quote inside a field that does not start with one as text.
    stray = (SAMPLES / "invalid" / "stray_quote.csv").read_text(encoding="utf-8")
    assert reference(stray) == [["a", 'b"c']]
    with pytest.raises(fpeg.ParseError):
        CSV.parse(stray)
    # Both refuse text after a closing quote, csv when strict.
    with pytest.raises(csv.Error):
        reference('"a"b\n')
    with pytest.raises(fpeg.ParseError):
        CSV.parse('"a"b\n')
