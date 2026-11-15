# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""JSON with comments and trailing commas, a grammar that extends the JSON grammar.

The cases are written by hand, each with the value `json.loads` gives the text without its
comments and trailing commas.
"""

from pathlib import Path
from typing import Any

import fpeg
import pytest

JSONC = fpeg.Grammar.open(
    Path(__file__).parent.parent / "crates" / "fpeg" / "grammars" / "jsonc.fpeg"
)

CASES = [
    ("[1, 2,]", [1, 2]),
    ('{"a": 1,}', {"a": 1}),
    ("// a comment first\n[true]", [True]),
    ('{"a": /* inside */ [null, {"b": 1.5,},],}', {"a": [None, {"b": 1.5}]}),
    ('"text // that is no comment"', "text // that is no comment"),
    ("[]", []),
    ("{}", {}),
]


@pytest.mark.parametrize(("text", "expected"), CASES)
def test_jsonc_gives_the_value_of_the_json_without_comments_and_commas(
    text: str, expected: Any
) -> None:
    assert fpeg.Transformer().transform(JSONC.parse(text)) == expected


@pytest.mark.parametrize("text", ["[,]", "{,}", "[1,,]", "[1 /* open"])
def test_jsonc_rejects_a_comma_with_nothing_before_it_and_an_open_comment(text: str) -> None:
    with pytest.raises(fpeg.ParseError):
        JSONC.parse(text)
