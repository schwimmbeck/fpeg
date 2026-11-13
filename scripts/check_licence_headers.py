# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Check that every source file starts with SPDX lines.

A file of the project starts with the project's two lines. A file that holds outside code keeps
its own copyright lines and needs a licence compatible with both MIT and Apache-2.0. Reads the
files git tracks or would track, so ignored build output is skipped. A file type the check does
not know is an error, so that a new kind of source file cannot go unchecked.
"""

import subprocess
import sys
from pathlib import Path

# REUSE-IgnoreStart
COPYRIGHT_TAG = "SPDX-FileCopyrightText: "
LICENCE_TAG = "SPDX-License-Identifier: "
# REUSE-IgnoreEnd
PROJECT_HOLDER = "2026 Dominik Schwimmbeck"
PROJECT_LICENCE = "MIT OR Apache-2.0"
COMPATIBLE_LICENCES = {
    "MIT",
    "MIT OR Apache-2.0",
    "Apache-2.0 OR MIT",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "0BSD",
    "CC0-1.0",
}

COMMENT_BY_SUFFIX = {
    ".rs": "//",
    ".pest": "//",
    ".fpeg": "//",
    ".py": "#",
    ".pyi": "#",
    ".toml": "#",
    ".yml": "#",
    ".yaml": "#",
    ".sh": "#",
    ".ps1": "#",
}
EXEMPT_SUFFIXES = {".md", ".lock", ".json", ".txt", ".svg"}
EXEMPT_NAMES = {
    "LICENSE-MIT",
    "LICENSE-APACHE",
    ".gitignore",
    ".gitattributes",
    ".python-version",
    "py.typed",
}
# Sample inputs of the acceptance grammars are data in many formats, not source files.
EXEMPT_DIRECTORIES = ("crates/fastpeg/grammars/samples/",)


def header_problem(path: Path, comment: str) -> str | None:
    """Return what is wrong with the SPDX lines at the top of a file, or None."""
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    if lines and lines[0].startswith("#!"):
        lines = lines[1:]
    prefix = f"{comment} SPDX-"
    tags: list[str] = []
    for line in lines:
        if not line.startswith(prefix):
            break
        tags.append(line.removeprefix(f"{comment} "))
    holders = [t.removeprefix(COPYRIGHT_TAG) for t in tags if t.startswith(COPYRIGHT_TAG)]
    licences = [t.removeprefix(LICENCE_TAG) for t in tags if t.startswith(LICENCE_TAG)]
    if not holders or len(licences) != 1:
        return "missing the SPDX header"
    if holders == [PROJECT_HOLDER] and licences == [PROJECT_LICENCE]:
        return None
    if holders == [PROJECT_HOLDER]:
        return f"a project file must use the licence {PROJECT_LICENCE}"
    if licences[0] not in COMPATIBLE_LICENCES:
        return f"licence {licences[0]} is not compatible with both MIT and Apache-2.0"
    return None


def main() -> int:
    """Check the files of the repository and report the ones with a problem."""
    root = Path(__file__).resolve().parent.parent
    listed = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=root,
        capture_output=True,
        check=True,
    ).stdout.decode("utf-8")
    checked = 0
    problems: list[str] = []
    for name in listed.split("\0"):
        path = root / name
        if not name or not path.is_file():
            continue
        if path.suffix in EXEMPT_SUFFIXES or path.name in EXEMPT_NAMES:
            continue
        if name.startswith(EXEMPT_DIRECTORIES):
            continue
        comment = COMMENT_BY_SUFFIX.get(path.suffix)
        if comment is None:
            problems.append(f"{name}: unknown file type, add it to {Path(__file__).name}")
            continue
        checked += 1
        problem = header_problem(path, comment)
        if problem:
            problems.append(f"{name}: {problem}")
    for problem in problems:
        print(problem)
    print(f"{checked} source files checked, {len(problems)} problems")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
