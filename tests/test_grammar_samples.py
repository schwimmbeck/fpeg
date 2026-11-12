# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT

import ast
import sys
import warnings
from pathlib import Path

import pytest

PYTHON_SAMPLES = Path(__file__).parent.parent / "grammars" / "samples" / "python"

pytestmark = pytest.mark.skipif(
    sys.version_info < (3, 15), reason="the Python grammar and its samples follow Python 3.15"
)


def parse(path: Path) -> None:
    # Some samples use unknown escapes on purpose, which Python only warns about.
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", SyntaxWarning)
        ast.parse(path.read_bytes(), filename=str(path))


def test_python_samples_are_valid_python() -> None:
    for path in sorted(PYTHON_SAMPLES.glob("*.py")):
        parse(path)


def test_invalid_python_samples_are_rejected_by_python() -> None:
    for path in sorted((PYTHON_SAMPLES / "invalid").glob("*.py")):
        with pytest.raises(SyntaxError):
            parse(path)
