# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Check a release tag against the version and print the release's section of the changelog.

The release workflow runs this first. A tag names the version of the workspace, which every
crate and the Python package share, with a v before it. A release prints its section of
`CHANGELOG.md`, headed `## X.Y.Z (YYYY-MM-DD)`, which becomes the notes of its GitHub release,
and fails without one. A pre-release, such as `v0.1.0-rc.1`, rehearses on TestPyPI and needs
no section.

Usage: python scripts/release_notes.py TAG
"""

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HEADING = re.compile(r"^## (?P<version>\S+) \(\d{4}-\d{2}-\d{2}\)$")


def workspace_version() -> str:
    """Return the version that the workspace gives every crate."""
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    return manifest["workspace"]["package"]["version"]


def section(changelog: str, version: str) -> str | None:
    """Return the text of a version's dated section of a changelog, or None."""
    lines = changelog.splitlines()
    for start, line in enumerate(lines):
        heading = HEADING.match(line)
        if heading and heading["version"] == version:
            body: list[str] = []
            for line in lines[start + 1 :]:
                if line.startswith("## "):
                    break
                body.append(line)
            return "\n".join(body).strip() + "\n"
    return None


def notes(tag: str, version: str, changelog: str) -> str:
    """Return the notes of a tag for a version, empty for a pre-release, or raise SystemExit."""
    if tag != f"v{version}":
        raise SystemExit(f"the tag {tag} does not name the version {version} of Cargo.toml")
    if "-" in version:
        return ""
    text = section(changelog, version)
    if text is None:
        raise SystemExit(f"CHANGELOG.md has no section headed ## {version} (YYYY-MM-DD)")
    return text


def main() -> None:
    """Print the notes of the tag given."""
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    print(notes(sys.argv[1], workspace_version(), changelog), end="")


if __name__ == "__main__":
    main()
