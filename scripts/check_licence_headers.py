# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT

"""Check that every source file starts with the two SPDX lines of the project.

Reads the files git tracks or would track, so ignored build output is skipped.
Prints each file without the header and exits with status 1 if there is one.
"""

import subprocess
import sys
from pathlib import Path

HEADER = ("SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck", "SPDX-License-Identifier: MIT")

COMMENT_BY_SUFFIX = {
    ".rs": "//",
    ".pest": "//",
    ".fpeg": "#",
    ".py": "#",
    ".pyi": "#",
    ".toml": "#",
    ".yml": "#",
    ".yaml": "#",
    ".sh": "#",
    ".ps1": "#",
}


def has_header(path: Path, comment: str) -> bool:
    """Return whether the file starts with the header, after an optional shebang line."""
    lines = path.read_text(encoding="utf-8").splitlines()
    if lines and lines[0].startswith("#!"):
        lines = lines[1:]
    return tuple(lines[:2]) == tuple(f"{comment} {line}" for line in HEADER)


def main() -> int:
    """Check the files of the repository and report the ones without a header."""
    root = Path(__file__).resolve().parent.parent
    listed = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=root,
        capture_output=True,
        check=True,
    ).stdout.decode("utf-8")
    checked = 0
    missing: list[str] = []
    for name in listed.split("\0"):
        path = root / name
        comment = COMMENT_BY_SUFFIX.get(path.suffix)
        if not name or comment is None or not path.is_file():
            continue
        checked += 1
        if not has_header(path, comment):
            missing.append(name)
    for name in missing:
        print(f"{name}: missing the SPDX header")
    print(f"{checked} source files checked, {len(missing)} without the header")
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
