# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

import contextlib
import io
import re
from pathlib import Path
from typing import Any

README = Path(__file__).resolve().parent.parent / "README.md"
FENCED = re.compile(r"^```(\w+)\n(.*?)^```$", re.MULTILINE | re.DOTALL)


def lines(text: str) -> list[str]:
    return [line.rstrip() for line in text.strip("\n").splitlines()]


def test_the_readme_examples_run_and_print_what_the_readme_shows() -> None:
    namespace: dict[str, Any] = {"__name__": "readme"}
    printed: str | None = None
    ran = 0
    for language, code in FENCED.findall(README.read_text(encoding="utf-8")):
        if language == "output":
            assert printed is not None, "an output block follows no Python block"
            assert lines(code) == lines(printed)
        else:
            assert not printed, "a block prints, and no output block after it shows what"
        printed = None
        if language == "python":
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                exec(compile(code, str(README), "exec"), namespace)
            printed = out.getvalue()
            ran += 1
    assert not printed, "the last block prints, and no output block after it shows what"
    assert ran > 0
