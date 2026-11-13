# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Benchmark inputs, generated or downloaded into ``benchmarks/corpora`` and checked by hash.

Run it as a script to prepare every input.
"""

import hashlib
import json
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

# Seconds to wait for the download of the first real application's sources.
DOWNLOAD_TIMEOUT_S = 60
# Records in the generated JSON file, and the seed of their content. Changing either changes the
# bytes of records.json, and its pinned hash with them.
JSON_RECORDS = 9000
JSON_SEED = 20261112


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
PYTHON_FILE = Input(
    "python_transformer.py",
    ORD_PACKAGE / "python_transformer.py",
    ORDEC_PACKAGE_SHA256["python_transformer.py"],
)
INPUTS = (RECORDS_JSON, PYTHON_FILE)


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

    text = json.dumps([record(i) for i in range(JSON_RECORDS)], indent=1)
    # Written beside the target and renamed, so an interrupted run leaves no partial file.
    partial = path.with_suffix(".partial")
    partial.write_text(text, encoding="utf-8", newline="\n")
    partial.replace(path)


def fetch_ord_package(target: Path) -> None:
    """Download the first real application's parser package into ``target``."""
    prefix = f"ordec-{ORDEC_COMMIT}/{ORDEC_PACKAGE_PATH}/"
    with tempfile.TemporaryDirectory(dir=CORPORA) as scratch:
        archive = Path(scratch) / "ordec.tar.gz"
        with urllib.request.urlopen(ORDEC_ARCHIVE_URL, timeout=DOWNLOAD_TIMEOUT_S) as response:
            archive.write_bytes(response.read())
        unpacked = Path(scratch) / "ordpkg"
        unpacked.mkdir()
        with tarfile.open(archive) as tar:
            for member in tar.getmembers():
                name = member.name.removeprefix(prefix)
                if member.isfile() and name in ORDEC_PACKAGE_SHA256:
                    member.name = name
                    tar.extract(member, unpacked, filter="data")
        shutil.move(unpacked, target)


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
    if not all(matches(ORD_PACKAGE / n, h) for n, h in ORDEC_PACKAGE_SHA256.items()):
        shutil.rmtree(ORD_PACKAGE, ignore_errors=True)
        fetch_ord_package(ORD_PACKAGE)
        for name, expected in ORDEC_PACKAGE_SHA256.items():
            check(ORD_PACKAGE / name, expected)


if __name__ == "__main__":
    prepare()
    for item in INPUTS:
        print(f"{item.path}: {item.path.stat().st_size} bytes")
