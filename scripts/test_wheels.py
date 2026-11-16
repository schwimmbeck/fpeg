# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Install each wheel of a folder with the interpreter it was built for and run the tests.

The release workflow builds the wheels of a platform and checks every one of them here, the
free-threaded ones included, in a fresh environment that holds the wheel and pytest alone. Only
the standard library runs this script, so that it works in the containers that build the Linux
wheels too.

Usage: python scripts/test_wheels.py DIST INTERPRETER...
"""

import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# Prints the tags of the wheels an interpreter installs, as in cp314-cp314t.
WHEEL_TAG = (
    "import sys, sysconfig; v = f'cp{sys.version_info[0]}{sys.version_info[1]}'; "
    "print(f'{v}-{v}' + ('t' if sysconfig.get_config_var('Py_GIL_DISABLED') else ''))"
)


def run(*command: str | Path, cwd: Path | None = None) -> str:
    """Run a command, stop at its failure, and return what it printed."""
    result = subprocess.run(command, cwd=cwd, capture_output=True, text=True, errors="replace")
    if result.returncode != 0:
        print(result.stdout, result.stderr, sep="\n")
        raise SystemExit(f"failed: {' '.join(map(str, command))}")
    return result.stdout


def pytest_requirement() -> str:
    """Return the pytest of the lock file, so the job that uploads the wheels runs a known one."""
    lock = tomllib.loads((ROOT / "uv.lock").read_text(encoding="utf-8"))
    (version,) = [package["version"] for package in lock["package"] if package["name"] == "pytest"]
    return f"pytest=={version}"


def test_wheel(dist: Path, interpreter: str, scratch: Path) -> None:
    """Install the wheel that fits an interpreter into a new environment and run the tests."""
    tag = run(interpreter, "-c", WHEEL_TAG).strip()
    wheels = sorted(dist.glob(f"*-{tag}-*.whl"))
    if len(wheels) != 1:
        raise SystemExit(f"{dist} holds {len(wheels)} wheels for {tag}, not one")
    environment = scratch / tag
    run(interpreter, "-m", "venv", environment)
    bin_folder = "Scripts" if sys.platform == "win32" else "bin"
    python = environment / bin_folder / "python"
    run(python, "-m", "pip", "install", "--quiet", pytest_requirement())
    run(python, "-m", "pip", "install", "--quiet", "--no-deps", wheels[0])
    summary = run(python, "-m", "pytest", "-q", "-p", "no:cacheprovider", "tests", cwd=ROOT)
    print(f"{wheels[0].name}: {summary.strip().splitlines()[-1]}")


def main() -> None:
    """Test the wheel of every interpreter given."""
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    dist = Path(sys.argv[1])
    with tempfile.TemporaryDirectory() as scratch:
        for interpreter in sys.argv[2:]:
            test_wheel(dist, interpreter, Path(scratch))


if __name__ == "__main__":
    main()
