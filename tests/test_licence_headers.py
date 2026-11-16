# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The check of the copyright and licence lines at the top of the repository's files."""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "scripts"))

import check_licence_headers as check

# The lines are put together here, so that the tools that read headers take none of them for
# this file's own.
PROJECT = f"# {check.COPYRIGHT_TAG}{check.PROJECT_HOLDER}"
PROJECT_LICENCE = f"# {check.LICENCE_TAG}{check.PROJECT_LICENCE}"
MIT = f"# {check.LICENCE_TAG}MIT"


def problem(tmp_path: Path, *lines: str) -> str | None:
    path = tmp_path / "file.py"
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return check.header_problem(path, "#")


def test_every_file_carries_the_projects_copyright_line(tmp_path: Path) -> None:
    assert problem(tmp_path, PROJECT, PROJECT_LICENCE) is None
    outside = f"# {check.COPYRIGHT_TAG}2017 Someone Else"
    assert problem(tmp_path, PROJECT, outside, MIT) is None
    # A holder that is not the project's, also one off by a letter, is refused.
    assert problem(tmp_path, outside, MIT) is not None
    misspelled = f"# {check.COPYRIGHT_TAG}2026 Dominik Schwimbeck"
    assert problem(tmp_path, misspelled, PROJECT_LICENCE) is not None
