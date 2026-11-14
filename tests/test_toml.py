# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The TOML grammar against `tomllib`: equal values of equal types, and the same verdict.

The grammar reads TOML 1.1 without dates and times, and leaves out the check that nothing is
defined twice. So the documents made here hold no dates, and their keys are two edits apart,
which keeps a single edit from defining one twice.
"""

import math
import random
import re
import string
import tomllib
from pathlib import Path
from typing import Any, NamedTuple

import fpeg
import pytest

SAMPLES = Path(__file__).parent.parent / "crates" / "fpeg" / "grammars" / "samples" / "toml"
TOML = fpeg.Grammar.open(SAMPLES.parent.parent / "toml.fpeg")

# Documents compared in the default run, and documents with one edit. An edit seldom lands
# where a rule of the grammar decides, so there are many.
DOCUMENTS = 500
EDITED_DOCUMENTS = 8000


def reads_toml_1_1() -> bool:
    try:
        tomllib.loads('a = "\\e"')
    except tomllib.TOMLDecodeError:
        return False
    return True


pytestmark = pytest.mark.skipif(
    not reads_toml_1_1(), reason="the tomllib of this Python reads TOML 1.0, the grammar 1.1"
)

ESCAPE = re.compile(
    r'\\(?:([btnfre"\\])|x([0-9A-Fa-f]{2})|u([0-9A-Fa-f]{4})|U([0-9A-Fa-f]{8})|[ \t]*\n[ \t\n]*)'
)
ESCAPED = {"b": "\b", "t": "\t", "n": "\n", "f": "\f", "r": "\r", "e": "\x1b", '"': '"', "\\": "\\"}


def unescape(body: str) -> str:
    """Decode the escapes of a basic string. A backslash at the end of a line takes the line
    break and the white space after it."""

    def decoded(match: re.Match[str]) -> str:
        name, *codes = match.groups()
        if name:
            return ESCAPED[name]
        code = next((code for code in codes if code), None)
        return chr(int(code, 16)) if code else ""

    return ESCAPE.sub(decoded, body)


def lines(token: str) -> str:
    """The content of a multi-line string: line breaks as one character, the first one dropped."""
    return token[3:-3].replace("\r\n", "\n").removeprefix("\n")


class Pair(NamedTuple):
    key: list[str]
    value: Any


class Header(NamedTuple):
    key: list[str]
    array: bool


def table(root: dict[str, Any], key: list[str]) -> dict[str, Any]:
    """The table a dotted key names, made where it is missing. An array of tables stands for
    its last table."""
    for part in key:
        inner: Any = root.setdefault(part, {})
        root = inner[-1] if isinstance(inner, list) else inner
    return root


class ToPython(fpeg.Transformer):
    """Makes the dict of a document. The grammar's builders make booleans, floats and arrays."""

    def start(self, *items: Pair | Header) -> dict[str, Any]:
        root: dict[str, Any] = {}
        current = root
        for item in items:
            if isinstance(item, Pair):
                table(current, item.key[:-1])[item.key[-1]] = item.value
            elif item.array:
                current = {}
                table(root, item.key[:-1]).setdefault(item.key[-1], []).append(current)
            else:
                current = table(root, item.key)
        return root

    def table(self, key: list[str]) -> Header:
        return Header(key, array=False)

    def table_array(self, key: list[str]) -> Header:
        return Header(key, array=True)

    def pair(self, key: list[str], value: Any) -> Pair:
        return Pair(key, value)

    def key(self, parts: list[str]) -> list[str]:
        return parts

    def inline_table(self, pairs: list[Pair]) -> dict[str, Any]:
        made: dict[str, Any] = {}
        for pair in pairs:
            table(made, pair.key[:-1])[pair.key[-1]] = pair.value
        return made

    def BARE_KEY(self, token: fpeg.Token) -> str:
        return str(token)

    def BASIC_STRING(self, token: fpeg.Token) -> str:
        return unescape(token[1:-1])

    def LITERAL_STRING(self, token: fpeg.Token) -> str:
        return token[1:-1]

    def ML_BASIC_STRING(self, token: fpeg.Token) -> str:
        return unescape(lines(token))

    def ML_LITERAL_STRING(self, token: fpeg.Token) -> str:
        return lines(token)

    def INTEGER(self, token: fpeg.Token) -> int:
        return int(token.replace("_", ""), 0)


TO_PYTHON = ToPython()


def load(text: str) -> Any:
    return TO_PYTHON.transform(TOML.parse(text))


def same(a: Any, b: Any) -> bool:
    """Equal values of equal types, keys in the same order, nan equal to nan."""
    if type(a) is not type(b):
        return False
    if isinstance(a, dict):
        return list(a) == list(b) and all(same(a[k], b[k]) for k in a)
    if isinstance(a, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b, strict=True))
    if isinstance(a, float):
        both_nan = math.isnan(a) and math.isnan(b)
        return both_nan or (a == b and math.copysign(1, a) == math.copysign(1, b))
    return a == b


class Writer:
    """Writes random documents. Every key is used once, and any two keys are two edits apart."""

    def __init__(self, rng: random.Random) -> None:
        self.rng = rng
        # No letter that an edit may write, so that no edit makes one key of another, and none
        # of the quoted keys, which an edit that drops their space makes bare ones.
        letters = "abcdfghijklmopqsvwyz" + string.ascii_uppercase[4:]
        self.keys = [letter * 2 for letter in letters] + [
            f'"{letter} {letter}"' for letter in string.ascii_uppercase[:4]
        ]
        rng.shuffle(self.keys)
        self.newline = rng.choice(["\n", "\n", "\r\n"])

    def key(self) -> str:
        return self.keys.pop()

    def string(self, pieces: list[str], quote: str, multiline: bool) -> str:
        body = "".join(self.rng.choice(pieces) for _ in range(self.rng.randrange(6)))
        # Three quotes in a row would end the string early. One or two may end the content.
        while quote * 3 in body:
            body = body.replace(quote * 3, quote * 2 + "a")
        quotes = quote * 3 if multiline else quote
        return f"{quotes}{body}{quotes}"

    def basic(self, multiline: bool) -> str:
        pieces = ["a", " ", "é", "😀", "\\n", "\\t", '\\"', "\\\\", "\\u00e9", "\\U0001F600"]
        pieces += ["\\e", "\\x41", "#", "'"]
        if multiline:
            pieces += ["\n", '"', '""', "\\\n   ", "\\  \n\n "]
        return self.string(pieces, '"', multiline)

    def literal(self, multiline: bool) -> str:
        pieces = ["a", " ", "é", "\\", '"', "#", "\\n"]
        if multiline:
            pieces += ["\n", "'", "''"]
        return self.string(pieces, "'", multiline)

    def scalar(self) -> str:
        rng = self.rng
        kind = rng.randrange(8)
        if kind == 0:
            return rng.choice(["true", "false"])
        if kind == 1:
            return rng.choice(["0", "-0", "+7", "1_000", "-42", "0xDEAD_beef", "0o7_1", "0b1_01"])
        if kind == 2:
            return rng.choice(
                ["0.0", "-0.0", "+1.5", "6.02e23", "1e-3", "1_0.2_5E+2", "inf", "-inf"]
            )
        if kind == 3:
            return rng.choice(["nan", "+nan", "-nan", "9007199254740993", "1e400"])
        if kind == 4:
            return self.literal(multiline=rng.random() < 0.3)
        return self.basic(multiline=rng.random() < 0.3)

    def value(self, depth: int) -> str:
        rng = self.rng
        kind = rng.randrange(10) if depth < 3 else 0
        if kind < 7:
            return self.scalar()
        if kind < 9:
            items = [self.value(depth + 1) for _ in range(rng.randrange(4))]
            if rng.random() < 0.5:
                return "[" + ", ".join(items) + "]"
            body = "".join(f"  {item}, # item{self.newline}" for item in items)
            return f"[{self.newline}{body}]"
        pairs = [f"{self.dotted()} = {self.value(depth + 1)}" for _ in range(rng.randrange(3))]
        if rng.random() < 0.5 or not pairs:
            return "{ " + ", ".join(pairs) + " }"
        return "{" + self.newline + f",{self.newline}".join(pairs) + f",{self.newline}" + "}"

    def dotted(self) -> str:
        parts = [self.key() for _ in range(self.rng.choice([1, 1, 1, 2]))]
        return self.rng.choice([".", " . "]).join(parts)

    def pairs(self) -> list[str]:
        made = []
        for _ in range(self.rng.randrange(3)):
            equals = self.rng.choice(["=", " = ", "\t=  "])
            comment = self.rng.choice(["", "", " # why"])
            made.append(f"{self.dotted()}{equals}{self.value(0)}{comment}")
        return made

    def document(self) -> str:
        rng = self.rng
        out = self.pairs()
        for _ in range(rng.randrange(3)):
            name = self.dotted()
            if rng.random() < 0.6:
                out += ["", f"[{name}]", *self.pairs()]
                continue
            for _ in range(rng.randrange(1, 3)):
                out += [f"[[{name}]]", "# an element", *self.pairs()]
        return self.newline.join(out) + rng.choice(["", self.newline])


def document(rng: random.Random) -> str:
    return Writer(rng).document()


def verdict(text: str) -> tuple[bool, Any]:
    """Whether a text is TOML to `tomllib`, and its value."""
    try:
        return True, tomllib.loads(text)
    except tomllib.TOMLDecodeError:
        return False, None


def test_the_samples_load_as_tomllib_loads_them_and_the_invalid_ones_fail() -> None:
    for path in sorted(SAMPLES.glob("*.toml")):
        text = path.read_text(encoding="utf-8")
        assert same(load(text), tomllib.loads(text)), path
    for path in sorted((SAMPLES / "invalid").glob("*.toml")):
        text = path.read_bytes().decode("utf-8")
        assert not verdict(text)[0], path
        with pytest.raises(fpeg.ParseError):
            TOML.parse(text)


def test_generated_documents_load_as_tomllib_loads_them() -> None:
    rng = random.Random(20261114)
    for _ in range(DOCUMENTS):
        text = document(rng)
        assert same(load(text), tomllib.loads(text)), text


def test_single_edits_of_documents_get_the_verdict_of_tomllib() -> None:
    rng = random.Random(3)
    # No colon, which would make a time of a float, and the grammar has no times.
    alphabet = "=[]{}.,\"'#\\ \n\t0123456789abx_+-etrue"
    compared = rejected = 0
    for _ in range(EDITED_DOCUMENTS):
        text = document(rng)
        if not text:
            continue
        at = rng.randrange(len(text) + 1)
        edit = rng.randrange(3)
        if edit == 0:
            text = text[:at] + text[at + 1 :]
        elif edit == 1:
            text = text[:at] + rng.choice(alphabet) + text[at:]
        else:
            text = text[:at] + rng.choice(alphabet) + text[at + 1 :]
        valid, value = verdict(text)
        try:
            ours = load(text)
        except fpeg.ParseError:
            assert not valid, text
            rejected += 1
            continue
        assert valid, text
        assert same(ours, value), text
        compared += 1
    # Both kinds of verdict occur, so the comparison sees each.
    assert compared > 100
    assert rejected > 100
