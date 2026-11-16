# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The JSON grammar against `json.loads`: equal values of equal types, and the same verdict."""

import json
import math
import random
from pathlib import Path
from typing import Any

import fpeg

SAMPLES = Path(__file__).parent.parent / "crates" / "fpeg" / "grammars" / "samples" / "json"
JSON = fpeg.Grammar.open(SAMPLES.parent.parent / "json.fpeg")
TO_PYTHON = fpeg.Transformer()

# Levels of arrays and objects inside each other at most in a generated value.
MAX_VALUE_DEPTH = 4
# Generated documents compared in the default run.
GENERATED_DOCUMENTS = 300
# Generated documents compared with one edit each.
EDITED_DOCUMENTS = 600
# The edited documents of each verdict are more than this, so that the comparison sees both.
MIN_VERDICTS = 50
# Values in the large document.
LARGE_DOCUMENT_VALUES = 2_000


def load(text: str) -> Any:
    return TO_PYTHON.transform(JSON.parse(text))


def refuse(name: str) -> Any:
    raise ValueError(f"{name} is not JSON")


def reference(text: str) -> Any:
    """`json.loads`, which takes NaN and the infinities too unless told not to."""
    return json.loads(text, parse_constant=refuse)


def same(a: Any, b: Any) -> bool:
    """Equal values of equal types, dict keys in the same order, -0.0 apart from 0.0."""
    if type(a) is not type(b):
        return False
    if isinstance(a, dict):
        return list(a) == list(b) and all(same(a[k], b[k]) for k in a)
    if isinstance(a, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b, strict=True))
    if isinstance(a, float):
        return a == b and math.copysign(1, a) == math.copysign(1, b)
    return a == b


def verdict(text: str) -> tuple[bool, Any]:
    """Whether a text is JSON to the reference, and its value."""
    try:
        return True, reference(text)
    except (ValueError, RecursionError):
        return False, None


def random_text(rng: random.Random, surrogates: bool) -> str:
    pieces = []
    for _ in range(rng.randint(0, 6)):
        kind = rng.random()
        if kind < 0.5:
            pieces.append(rng.choice('ab "\\/\b\f\n\r\t\x7fé€'))
        elif kind < 0.8:
            pieces.append(chr(rng.randint(0x20, 0x2FFF)))
        elif surrogates:
            pieces.append(chr(rng.randint(0xD800, 0xDFFF)))
        else:
            pieces.append(chr(rng.randint(0x10000, 0x10FFFF)))
    return "".join(pieces)


def random_value(rng: random.Random, depth: int, surrogates: bool) -> Any:
    kind = rng.randrange(8 if depth < MAX_VALUE_DEPTH else 6)
    if kind == 0:
        return rng.choice([True, False, None])
    if kind == 1:
        return rng.choice([0, -1, 7, rng.randint(-(10**30), 10**30)])
    if kind == 2:
        return rng.choice([0.0, -0.0, 1e300, 5e-324, rng.uniform(-1e6, 1e6), rng.random()])
    if kind in (3, 4, 5):
        return random_text(rng, surrogates)
    if kind == 6:
        return [random_value(rng, depth + 1, surrogates) for _ in range(rng.randint(0, 4))]
    return {
        random_text(rng, surrogates): random_value(rng, depth + 1, surrogates)
        for _ in range(rng.randint(0, 4))
    }


def random_document(rng: random.Random) -> str:
    # Without ensure_ascii a lone surrogate stays raw, which no UTF-8 input can hold.
    ensure_ascii = rng.random() < 0.5
    value = random_value(rng, 0, surrogates=ensure_ascii)
    return json.dumps(
        value,
        ensure_ascii=ensure_ascii,
        indent=rng.choice([None, 0, 2, "\t"]),
        separators=rng.choice([None, (",", ":"), (" , ", " : ")]),
    )


def test_the_samples_load_as_json_loads_them_and_the_invalid_ones_fail() -> None:
    for path in sorted(SAMPLES.glob("*.json")):
        text = path.read_text(encoding="utf-8")
        assert same(load(text), reference(text)), path
    for path in sorted((SAMPLES / "invalid").glob("*.json")):
        text = path.read_text(encoding="utf-8")
        assert not verdict(text)[0], path
        try:
            JSON.parse(text)
        except fpeg.ParseError:
            continue
        raise AssertionError(f"{path} parses")


def test_generated_documents_load_as_json_loads_them() -> None:
    rng = random.Random(20261113)
    for _ in range(GENERATED_DOCUMENTS):
        text = random_document(rng)
        assert same(load(text), reference(text)), text


def test_single_edits_of_documents_get_the_verdict_of_json_loads() -> None:
    rng = random.Random(7)
    alphabet = ',:[]{}"\\0123456789.eE+-tfnrul \n\x01'
    compared = rejected = 0
    for _ in range(EDITED_DOCUMENTS):
        text = random_document(rng)
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
    assert compared > MIN_VERDICTS
    assert rejected > MIN_VERDICTS


def test_a_large_document_loads_as_json_loads_it() -> None:
    rng = random.Random(11)
    records = [random_value(rng, 1, surrogates=True) for _ in range(LARGE_DOCUMENT_VALUES)]
    text = json.dumps(records)
    assert same(load(text), reference(text))
