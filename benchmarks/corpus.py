# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Benchmark inputs and test corpora, generated or downloaded into ``benchmarks/corpora``.

Each is checked by hash. Run it as a script to prepare every input.
"""

import hashlib
import json
import math
import random
import shutil
import tarfile
import tempfile
import urllib.request
from dataclasses import dataclass
from pathlib import Path

CORPORA = Path(__file__).resolve().parent / "corpora"

# The first real application, pinned to the commit of the first baseline measurements.
ORDEC_COMMIT = "1ce99ab763455eac674f8cc4e55ff1e6ca9fd31b"
ORDEC_ARCHIVE_URL = f"https://codeload.github.com/tub-msc/ordec/tar.gz/{ORDEC_COMMIT}"
ORDEC_PACKAGE_PATH = "src/ordec/ord"
ORDEC_PACKAGE_SHA256 = {
    "__init__.py": "fa37e88e704e354c481f33ad10000dfa38c45836cc47cbe28eef109e7c20b5a7",
    "ord.lark": "59e66b107dc974327fdf97755ff24bc33136d51b1ef2dc51bb05234ca48d1fa9",
    "ord_transformer.py": "b5295b37a8bafa4c642fb04488989aaf8da0a54fdd07a0e9082ac8ef48982b95",
    "parser.py": "bf09618fe631d77fa2e22707e2d12dea495adcac7047832ef3087b9d6b718dbb",
    "python_transformer.py": "9b35b499e1dfe631ebd1ab6b9fcc591541bebbaaa3e9bea63ca01888b557cf1a",
}
# The hash of its 76 ORD files, over their paths below the archive's top folder and their contents.
ORD_FILES_SHA256 = "916d9da4aae54e33a786104cffc81cb69d2ad19689cb5610835703dfa021af18"

# The test suite of Lua 5.4, which the Lua grammar must parse, pinned to the hash lua.org lists.
LUA_TESTS_URL = "https://www.lua.org/tests/lua-5.4.9-tests.tar.gz"

# Seconds to wait for a download.
DOWNLOAD_TIMEOUT_S = 60
# Records in the generated JSON file, and the seed of their content. Changing either changes the
# bytes of records.json, and its pinned hash with them.
JSON_RECORDS = 9000
JSON_SEED = 20261112
# The size of the generated expression, and the seed of its content, pinned alike.
EXPRESSION_BYTES = 100_000
EXPRESSION_SEED = 4
# How deep the terms of the expression nest at most.
EXPRESSION_DEPTH = 6
# The lines of arithmetic for the calculators, the seed of their content and how deep one nests.
CALCULATOR_LINES = 2000
CALCULATOR_SEED = 5
CALCULATOR_DEPTH = 5


@dataclass(frozen=True)
class Input:
    """A benchmark input file with the hash that pins its content."""

    name: str
    path: Path
    sha256: str

    def read(self) -> str:
        """Return the text of the input, which ``prepare`` must have created."""
        if not self.path.is_file():
            raise SystemExit(f"{self.path} is missing, run benchmarks/corpus.py first")
        return self.path.read_text(encoding="utf-8")


RECORDS_JSON = Input(
    "records.json",
    CORPORA / "records.json",
    "35148527f24c4ab316fed81f290d8c490b16896c0029a941fa71b1266e3c76e3",
)
ORD_PACKAGE = CORPORA / "ordpkg"
# The ORD files, which the ORD grammar of the tests must read as the application's parser does.
ORD_FILES = CORPORA / "ord"
PYTHON_FILE = Input(
    "python_transformer.py",
    ORD_PACKAGE / "python_transformer.py",
    ORDEC_PACKAGE_SHA256["python_transformer.py"],
)
EXPRESSIONS = Input(
    "expressions.c",
    CORPORA / "expressions.c",
    "9186ae97b00659573f1e161e833ee0a08520218567fb839a2012714f7f326b67",
)
CALCULATOR = Input(
    "calculator.txt",
    CORPORA / "calculator.txt",
    "5d776b59a1c5b3b7fc77fe20f6ff65232d51c5dc98e06417896af6cba5862162",
)
INPUTS = (RECORDS_JSON, PYTHON_FILE, EXPRESSIONS, CALCULATOR)
LUA_TESTS = Input(
    "lua-5.4.9-tests.tar.gz",
    CORPORA / "lua-5.4.9-tests.tar.gz",
    "7d971845f545ffc09fbb3128a86b2c6524161c70d0fdf0154a16e8c00c343fca",
)


def sha256(path: Path) -> str:
    """Return the SHA-256 of a file as hex."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write_records_json(path: Path) -> None:
    """Write about 2.5 MB of JSON records, the same bytes on every run."""
    rng = random.Random(JSON_SEED)
    words = [
        "alpha",
        "beta",
        "gamma",
        "delta",
        "resistor",
        "nmos",
        "pmos",
        "net",
        "path",
        "cell",
        "viewgen",
        "symbol",
    ]

    def record(i: int) -> dict[str, object]:
        return {
            "id": i,
            "name": f"{rng.choice(words)}_{i}",
            "enabled": rng.random() < 0.5,
            "ratio": round(rng.uniform(-1e3, 1e3), 4),
            "tags": rng.sample(words, 3),
            "pos": {"x": rng.randint(-5000, 5000), "y": rng.randint(-5000, 5000)},
            "parent": None if i % 7 == 0 else i // 2,
            "note": 'line one\\nline "two" with a tab\t and unicode äöü',
        }

    write_whole(path, json.dumps([record(i) for i in range(JSON_RECORDS)], indent=1))


def write_expressions(path: Path) -> None:
    """Write one expression of a C-like language, a sum of random terms, the same on every run."""
    rng = random.Random(EXPRESSION_SEED)
    binary = ["+", "-", "*", "/", "%", "<<", ">>", "<", "<=", "==", "!=", "&", "^", "|", "&&", "||"]

    def expression(depth: int) -> str:
        if depth == 0 or rng.random() < 0.25:
            return rng.choice(["1", "42", "x", "name", "3.5", '"s"'])

        def inner() -> str:
            return expression(depth - 1)

        kind = rng.random()
        if kind < 0.55:
            return f"{inner()} {rng.choice(binary)} {inner()}"
        if kind < 0.65:
            return f"({inner()})"
        if kind < 0.75:
            return f"{rng.choice(['-', '!', '~'])}{inner()}"
        if kind < 0.85:
            return f"f({inner()}, {inner()})"
        if kind < 0.92:
            return f"a[{inner()}].m"
        return f"{inner()} ? {inner()} : {inner()}"

    terms: list[str] = []
    size = 0
    while size < EXPRESSION_BYTES:
        terms.append(expression(EXPRESSION_DEPTH))
        size += len(terms[-1]) + 3
    write_whole(path, " +\n".join(terms) + "\n")


def write_calculator(path: Path) -> None:
    """Write lines of arithmetic in Python's notation, each of which `eval` gives a finite float."""
    rng = random.Random(CALCULATOR_SEED)

    def expression(depth: int) -> str:
        # A line is at least one operation, and no operand nests without end.
        kind = 5 if depth == 0 else rng.randrange(10) if depth < CALCULATOR_DEPTH else 0
        if kind < 3:
            # A decimal point, so that every library computes with floats.
            return f"{rng.randrange(20)}.{rng.randrange(10)}"
        if kind == 3:
            return f"({expression(depth + 1)})"
        if kind == 4:
            return f"-{expression(depth + 1)}"
        operator = rng.choice(["+", "-", "*", "/", "**"])
        return f"{expression(depth + 1)} {operator} {expression(depth + 1)}"

    lines: list[str] = []
    while len(lines) < CALCULATOR_LINES:
        line = expression(0)
        try:
            value = eval(line)
        except (ZeroDivisionError, OverflowError):
            continue
        # A negative number to a broken power is complex, which the other libraries need not know.
        if isinstance(value, float) and math.isfinite(value):
            lines.append(line)
    write_whole(path, "\n".join(lines) + "\n")


def write_whole(path: Path, text: str) -> None:
    """Write a file beside its target and rename it, so an interrupted run leaves no part of it."""
    partial = path.with_suffix(".partial")
    partial.write_text(text, encoding="utf-8", newline="\n")
    partial.replace(path)


def fetch(url: str, path: Path, expected: str) -> None:
    """Download a file beside its target, and rename it once it has the pinned hash."""
    partial = path.with_suffix(".partial")
    with urllib.request.urlopen(url, timeout=DOWNLOAD_TIMEOUT_S) as response:
        partial.write_bytes(response.read())
    check(partial, expected)
    partial.replace(path)


def fetch_ordec() -> None:
    """Download the first real application for its parser package and its ORD files.

    Both are checked before they take their names.
    """
    top = f"ordec-{ORDEC_COMMIT}/"
    package = f"{top}{ORDEC_PACKAGE_PATH}/"
    with tempfile.TemporaryDirectory(dir=CORPORA) as scratch:
        archive = Path(scratch) / "ordec.tar.gz"
        with urllib.request.urlopen(ORDEC_ARCHIVE_URL, timeout=DOWNLOAD_TIMEOUT_S) as response:
            archive.write_bytes(response.read())
        unpacked_package = Path(scratch) / "ordpkg"
        unpacked_files = Path(scratch) / "ord"
        unpacked_package.mkdir()
        unpacked_files.mkdir()
        with tarfile.open(archive) as tar:
            for member in tar.getmembers():
                if not member.isfile():
                    continue
                if member.name.removeprefix(package) in ORDEC_PACKAGE_SHA256:
                    member.name = member.name.removeprefix(package)
                    tar.extract(member, unpacked_package, filter="data")
                elif member.name.startswith(top) and member.name.endswith(".ord"):
                    member.name = member.name.removeprefix(top)
                    tar.extract(member, unpacked_files, filter="data")
        for name, expected in ORDEC_PACKAGE_SHA256.items():
            check(unpacked_package / name, expected)
        if ord_files_digest(unpacked_files) != ORD_FILES_SHA256:
            raise SystemExit(f"the ORD files do not have the pinned SHA-256 {ORD_FILES_SHA256}")
        for unpacked, target in ((unpacked_package, ORD_PACKAGE), (unpacked_files, ORD_FILES)):
            shutil.rmtree(target, ignore_errors=True)
            shutil.move(unpacked, target)


def ord_files_digest(folder: Path) -> str:
    """Return the hash of the ORD files below a folder, over their paths and their contents."""
    digest = hashlib.sha256()
    paths = sorted(folder.rglob("*.ord"), key=lambda path: path.relative_to(folder).as_posix())
    for path in paths:
        digest.update(path.relative_to(folder).as_posix().encode() + b"\0")
        digest.update(path.read_bytes() + b"\0")
    return digest.hexdigest()


def matches(path: Path, expected: str) -> bool:
    """Return whether a file exists and has the pinned hash."""
    return path.is_file() and sha256(path) == expected


def check(path: Path, expected: str) -> None:
    """Stop when a file that was just made does not have the pinned hash."""
    if not matches(path, expected):
        raise SystemExit(f"{path} does not have the pinned SHA-256 {expected}")


def prepare() -> None:
    """Make every input that is missing or damaged, then check all of them."""
    CORPORA.mkdir(exist_ok=True)
    if not matches(RECORDS_JSON.path, RECORDS_JSON.sha256):
        # Python's random module could change its output, which this check would catch.
        write_records_json(RECORDS_JSON.path)
        check(RECORDS_JSON.path, RECORDS_JSON.sha256)
    if not matches(EXPRESSIONS.path, EXPRESSIONS.sha256):
        write_expressions(EXPRESSIONS.path)
        check(EXPRESSIONS.path, EXPRESSIONS.sha256)
    if not matches(CALCULATOR.path, CALCULATOR.sha256):
        write_calculator(CALCULATOR.path)
        check(CALCULATOR.path, CALCULATOR.sha256)
    package = all(matches(ORD_PACKAGE / n, h) for n, h in ORDEC_PACKAGE_SHA256.items())
    files = ORD_FILES.is_dir() and ord_files_digest(ORD_FILES) == ORD_FILES_SHA256
    if not (package and files):
        fetch_ordec()
    if not matches(LUA_TESTS.path, LUA_TESTS.sha256):
        fetch(LUA_TESTS_URL, LUA_TESTS.path, LUA_TESTS.sha256)


if __name__ == "__main__":
    prepare()
    for item in INPUTS:
        print(f"{item.path}: {item.path.stat().st_size} bytes")
