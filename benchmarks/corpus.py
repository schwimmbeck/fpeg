# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT

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

# The first real application, pinned to the commit the plan's measurements used.
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

DOWNLOAD_TIMEOUT_S = 60
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
    path.write_text(text, encoding="utf-8", newline="\n")


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


def check(path: Path, expected: str) -> None:
    """Stop when a file does not have the pinned hash."""
    actual = sha256(path)
    if actual != expected:
        raise SystemExit(f"{path} has SHA-256 {actual}, expected {expected}")


def prepare() -> None:
    """Create every missing input and check the hashes of all of them."""
    CORPORA.mkdir(exist_ok=True)
    if not RECORDS_JSON.path.is_file():
        write_records_json(RECORDS_JSON.path)
    check(RECORDS_JSON.path, RECORDS_JSON.sha256)
    if not ORD_PACKAGE.is_dir():
        fetch_ord_package(ORD_PACKAGE)
    for name, expected in ORDEC_PACKAGE_SHA256.items():
        check(ORD_PACKAGE / name, expected)


if __name__ == "__main__":
    prepare()
    for item in INPUTS:
        print(f"{item.path}: {item.path.stat().st_size} bytes")
