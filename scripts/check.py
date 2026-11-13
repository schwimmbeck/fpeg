# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT

"""Run every check the project requires, in order, and stop at the first failure.

The same commands run in CI. Prints the time each check took.
"""

import subprocess
import sys
import time
from pathlib import Path

BENCH = "--manifest-path benchmarks/rust/Cargo.toml"
# The order of the list in AGENTS.md.
CHECKS = [
    ("Rust formatting", "cargo fmt --check"),
    ("Rust lints", "cargo clippy --all-targets -- -D warnings"),
    ("Rust lints without tests", "cargo clippy --workspace --lib --bins -- -D warnings"),
    ("Rust tests", "cargo test -q"),
    ("Python formatting", "uv run ruff format --check"),
    ("Python lints", "uv run ruff check"),
    ("Python types", "uv run pyright --warnings"),
    ("Python tests", "uv run pytest -q"),
    (
        "documentation",
        "uv run --group docs sphinx-build -W --keep-going -q -b html docs docs/_build/html",
    ),
    ("licence headers", "uv run python scripts/check_licence_headers.py"),
    ("benchmark crate formatting", f"cargo fmt --check {BENCH}"),
    ("benchmark crate lints", f"cargo clippy {BENCH} --all-targets -- -D warnings"),
]

# How much of a failed check's output to show, counted from its end, in characters.
OUTPUT_TAIL_CHARS = 4000


def main() -> int:
    """Run the checks and report the first failure."""
    root = Path(__file__).resolve().parent.parent
    total = time.perf_counter()
    for name, command in CHECKS:
        start = time.perf_counter()
        result = subprocess.run(
            command.split(), cwd=root, capture_output=True, text=True, errors="replace"
        )
        seconds = time.perf_counter() - start
        if result.returncode != 0:
            print(f"FAILED {name} ({seconds:.1f} s): {command}")
            print(result.stdout[-OUTPUT_TAIL_CHARS:])
            print(result.stderr[-OUTPUT_TAIL_CHARS:])
            return 1
        print(f"ok     {name} ({seconds:.1f} s)")
    print(f"all checks passed in {time.perf_counter() - total:.1f} s")
    return 0


if __name__ == "__main__":
    sys.exit(main())
