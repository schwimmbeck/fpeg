# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The check of a release tag and the notes it takes from the changelog."""

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).parent.parent / "scripts"))

import release_notes

CHANGELOG = """\
# Changelog

## Unreleased

- Next.

## 0.2.0 (2027-01-05)

- Two.
- More.

## 0.1.0 (2026-11-20)

- One.
"""


def test_a_release_takes_its_dated_section_of_the_changelog() -> None:
    assert release_notes.notes("v0.2.0", "0.2.0", CHANGELOG) == "- Two.\n- More.\n"
    assert release_notes.notes("v0.1.0", "0.1.0", CHANGELOG) == "- One.\n"


def test_a_release_without_a_dated_section_is_refused() -> None:
    with pytest.raises(SystemExit, match="no section"):
        release_notes.notes("v0.3.0", "0.3.0", CHANGELOG)


def test_a_pre_release_needs_no_section() -> None:
    assert release_notes.notes("v0.3.0-rc.1", "0.3.0-rc.1", CHANGELOG) == ""


def test_a_tag_must_name_the_version_of_the_workspace() -> None:
    with pytest.raises(SystemExit, match="does not name the version"):
        release_notes.notes("v0.2.1", "0.2.0", CHANGELOG)
