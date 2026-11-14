# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Run the benchmark suites and write their results.

Prepares the inputs, runs the pyperf suites and the criterion suite, and writes ``results.json``,
a ``summary.md`` and the raw pyperf files to ``benchmarks/.cache/last-run``. With ``--pgo`` the
criterion suite runs a second time, built with a profile of fpeg's machine. With ``--record`` a
complete run is then moved to ``benchmarks/results/<date>-<platform>``, with a letter after the
date for each later run of the same day.
"""

import argparse
import datetime
import importlib.metadata
import json
import os
import platform
import re
import shutil
import string
import subprocess
import sys
import tomllib
from dataclasses import asdict, dataclass
from pathlib import Path

import corpus
import pyperf
from harness import INPUT_METADATA_KEY

BENCHMARKS = Path(__file__).resolve().parent
RESULTS = BENCHMARKS / "results"
LAST_RUN = BENCHMARKS / ".cache" / "last-run"
RUST = BENCHMARKS / "rust"
RUST_TARGET = RUST / "target"

PYPERF_SUITES = ("json", "binding", "calculator", "python", "ord")
SUITES = (*PYPERF_SUITES, "rust")
# The criterion suite built with a profile, which only --pgo runs.
PGO_SUITE = "rust-pgo"
MODES = ("default", "fast", "rigorous", "smoke")
RECORD_MODES = ("default", "rigorous")
PYPERF_MODE_ARGS = {
    "default": [],
    "fast": ["--fast"],
    "rigorous": ["--rigorous"],
    "smoke": ["--debug-single-value"],
}
# Criterion has no mode that measures longer than its default. Smoke takes a short real
# measurement, so the CI smoke run reads criterion's files too.
CRITERION_MODE_ARGS = {"default": [], "fast": ["--quick"], "rigorous": [], "smoke": ["--quick"]}

PYTHON_PACKAGES = ("lark", "pe", "libcst", "tree-sitter", "tree-sitter-python", "pyperf")
RUST_CRATES = ("pest", "pest_derive", "pest_meta", "pest_vm", "serde_json", "criterion")
# The criterion suites.
RUST_BENCHES = ("json", "expressions", "passes")
# The input of each criterion group whose benchmarks report a throughput of a corpus.
RUST_INPUTS = {
    "json": corpus.RECORDS_JSON,
    "expressions": corpus.EXPRESSIONS,
    "passes on JSON": corpus.RECORDS_JSON,
    "passes on expressions": corpus.EXPRESSIONS,
    "passes on Python": corpus.PYTHON_FILE,
}
PLATFORM_NAMES = {"win32": "windows", "linux": "linux", "darwin": "macos"}
NS_PER_S = 1e9
BYTES_PER_MB = 1e6


@dataclass(frozen=True)
class Row:
    """One measured benchmark. Times are seconds per call."""

    suite: str
    name: str
    input: str | None
    mean_s: float
    stdev_s: float
    median_s: float
    min_s: float
    values: int


@dataclass(frozen=True)
class InputInfo:
    """Size and hash of one benchmark input."""

    bytes: int
    lines: int
    sha256: str


@dataclass(frozen=True)
class Environment:
    """The machine, the software and the inputs of a run."""

    commit: str
    platform: str
    cpu: str
    logical_cpus: int | None
    python: str
    python_build: str
    packages: dict[str, str]
    rustc: str
    crates: dict[str, str]
    inputs: dict[str, InputInfo]


def run_pyperf_suite(suite: str, mode: str, out: Path) -> list[Row]:
    """Run one pyperf script and read its results back."""
    raw = out / f"{suite}.pyperf.json"
    script = BENCHMARKS / f"bench_{suite}.py"
    subprocess.run(
        [sys.executable, str(script), "-o", str(raw), *PYPERF_MODE_ARGS[mode]], check=True
    )
    scrub_pyperf_file(raw)
    rows = []
    for bench in pyperf.BenchmarkSuite.load(str(raw)).get_benchmarks():
        values = bench.get_values()
        rows.append(
            Row(
                suite=suite,
                name=bench.get_name(),
                input=bench.get_metadata().get(INPUT_METADATA_KEY),
                mean_s=bench.mean(),
                stdev_s=bench.stdev() if len(values) > 1 else 0.0,
                median_s=bench.median(),
                min_s=min(values),
                values=len(values),
            )
        )
    return rows


def scrub_pyperf_file(path: Path) -> None:
    """Drop the host name and the home folder from pyperf's metadata, since results are public."""
    # Windows paths compare without case, so a path may spell the home folder differently.
    flags = re.IGNORECASE if sys.platform == "win32" else 0
    home = re.compile(re.escape(str(Path.home())), flags)

    def scrub(value: object) -> object:
        if isinstance(value, dict):
            return {k: scrub(v) for k, v in value.items() if k != "hostname"}
        if isinstance(value, list):
            return [scrub(v) for v in value]
        if isinstance(value, str):
            return home.sub("~", value)
        return value

    data = json.loads(path.read_text(encoding="utf-8"))
    path.write_text(json.dumps(scrub(data)), encoding="utf-8", newline="\n")


def cargo_env(target: Path) -> dict[str, str]:
    """Return the environment of cargo, building into `target`."""
    # Pin where criterion writes, and run in the package so rustup picks its pinned toolchain.
    env = {k: v for k, v in os.environ.items() if k not in ("CRITERION_HOME", "RUSTFLAGS")}
    env["CARGO_TARGET_DIR"] = str(target)
    return env


def llvm_profdata() -> Path:
    """Find the merger of profiles that rustup's component llvm-tools brings."""
    run = {"cwd": RUST, "capture_output": True, "text": True, "check": True}
    sysroot = subprocess.run(["rustc", "--print", "sysroot"], **run).stdout.strip()
    host = re.search(r"^host: (\S+)", subprocess.run(["rustc", "-vV"], **run).stdout, re.M)
    name = "llvm-profdata.exe" if sys.platform == "win32" else "llvm-profdata"
    tool = Path(sysroot) / "lib" / "rustlib" / (host[1] if host else "") / "bin" / name
    if not tool.is_file():
        raise SystemExit("--pgo needs llvm-profdata: rustup component add llvm-tools")
    return tool


def train_profile() -> Path:
    """Build the passes suite to count what runs, run it briefly, and merge its counts."""
    data = RUST_TARGET / "pgo-data"
    shutil.rmtree(data, ignore_errors=True)
    data.mkdir(parents=True)
    env = cargo_env(RUST_TARGET / "pgo-train")
    env["RUSTFLAGS"] = f"-Cprofile-generate={data}"
    command = ["cargo", "bench", "--bench", "passes", "--", "--quick"]
    subprocess.run(command, cwd=RUST, env=env, check=True, stdout=subprocess.DEVNULL)
    merged = data / "merged.profdata"
    subprocess.run([llvm_profdata(), "merge", "-o", merged, data], check=True)
    return merged


def run_rust_suite(mode: str, pgo: bool = False) -> list[Row]:
    """Run the criterion benchmarks and read their estimates.

    With `pgo`, the benchmarks are built with a profile of the passes suite.
    """
    target = RUST_TARGET / "pgo" if pgo else RUST_TARGET
    criterion = target / "criterion"
    shutil.rmtree(criterion, ignore_errors=True)
    env = cargo_env(target)
    if pgo:
        env["RUSTFLAGS"] = f"-Cprofile-use={train_profile()}"
    benches = [arg for name in RUST_BENCHES for arg in ("--bench", name)]
    command = ["cargo", "bench", *benches, "--", *CRITERION_MODE_ARGS[mode]]
    subprocess.run(command, cwd=RUST, env=env, check=True)
    rows = []
    for meta_path in sorted(criterion.rglob("new/benchmark.json")):
        folder = meta_path.parent
        meta = json.loads(meta_path.read_text(encoding="utf-8"))
        estimates = json.loads((folder / "estimates.json").read_text(encoding="utf-8"))
        sample = json.loads((folder / "sample.json").read_text(encoding="utf-8"))
        per_iteration = [t / n for t, n in zip(sample["times"], sample["iters"], strict=True)]
        rows.append(
            Row(
                suite=PGO_SUITE if pgo else "rust",
                name=meta["full_id"],
                input=RUST_INPUTS[group].name
                if (group := meta["group_id"]) in RUST_INPUTS
                else None,
                mean_s=estimates["mean"]["point_estimate"] / NS_PER_S,
                stdev_s=estimates["std_dev"]["point_estimate"] / NS_PER_S,
                median_s=estimates["median"]["point_estimate"] / NS_PER_S,
                min_s=min(per_iteration) / NS_PER_S,
                values=len(per_iteration),
            )
        )
    if not rows:
        raise SystemExit(f"criterion wrote no estimates under {criterion}")
    return rows


def cpu_name() -> str:
    """Return the processor's marketing name, or what the platform module knows."""
    try:
        if sys.platform == "win32":
            import winreg

            key = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0"
            with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, key) as handle:
                return str(winreg.QueryValueEx(handle, "ProcessorNameString")[0]).strip()
        if sys.platform == "linux":
            for line in Path("/proc/cpuinfo").read_text(encoding="utf-8").splitlines():
                if line.startswith("model name"):
                    return line.split(":", 1)[1].strip()
        if sys.platform == "darwin":
            command = ["sysctl", "-n", "machdep.cpu.brand_string"]
            return subprocess.run(command, capture_output=True, text=True).stdout.strip()
    except OSError:
        pass
    return platform.processor() or "unknown"


def git(*args: str) -> str:
    """Return the output of a git command in the repository."""
    return subprocess.run(
        ["git", *args], cwd=BENCHMARKS, capture_output=True, text=True, check=True
    ).stdout.strip()


def environment() -> Environment:
    """Describe the machine, the software and the inputs of this run."""
    commit = git("rev-parse", "--short", "HEAD")
    if git("status", "--porcelain", "--untracked-files=no"):
        commit += " with uncommitted changes"
    rustc = subprocess.run(["rustc", "--version"], cwd=RUST, capture_output=True, text=True)
    lock = tomllib.loads((RUST / "Cargo.lock").read_text(encoding="utf-8"))
    crates = {p["name"]: p["version"] for p in lock["package"] if p["name"] in RUST_CRATES}
    inputs = {}
    for item in corpus.INPUTS:
        text = item.read()
        inputs[item.name] = InputInfo(len(text.encode("utf-8")), text.count("\n"), item.sha256)
    return Environment(
        commit=commit,
        platform=platform.platform(),
        cpu=cpu_name(),
        logical_cpus=os.cpu_count(),
        python=f"{platform.python_implementation()} {platform.python_version()}",
        python_build=sys.version,
        packages={name: importlib.metadata.version(name) for name in PYTHON_PACKAGES},
        rustc=rustc.stdout.strip(),
        crates=dict(sorted(crates.items())),
        inputs=inputs,
    )


def format_time(seconds: float) -> str:
    """Format a duration with three significant digits in a fitting unit."""
    for unit, scale in (("s", 1.0), ("ms", 1e-3), ("µs", 1e-6), ("ns", 1e-9)):
        if seconds >= scale or unit == "ns":
            value = seconds / scale
            decimals = 2 if value < 10 else 1 if value < 100 else 0
            return f"{value:.{decimals}f} {unit}"
    raise AssertionError("unreachable")


def throughput(row: Row, inputs: dict[str, InputInfo]) -> str:
    """Format the throughput of a row from its mean time, in lines/s for source code."""
    if row.input is None or row.mean_s <= 0:
        return ""
    info = inputs[row.input]
    if row.input.endswith(".py"):
        return f"{info.lines / row.mean_s:,.0f} lines/s"
    return f"{info.bytes / row.mean_s / BYTES_PER_MB:.1f} MB/s"


def summary(title: str, mode: str, env: Environment, rows: list[Row]) -> str:
    """Render the results as Markdown."""
    packages = ", ".join(f"{k} {v}" for k, v in env.packages.items())
    crates = ", ".join(f"{k} {v}" for k, v in env.crates.items())
    inputs = "; ".join(
        f"{name} {info.bytes:,} bytes, {info.lines:,} lines" for name, info in env.inputs.items()
    )
    lines = [
        f"# Benchmark results {title}",
        "",
        f"Written by `benchmarks/run.py` in {mode} mode at commit {env.commit}.",
        "Times are per call.",
        "",
        f"- Machine: {env.platform}, {env.cpu}, {env.logical_cpus} logical CPUs",
        f"- {env.python}, {env.rustc}",
        f"- Python packages: {packages}",
        f"- Rust crates: {crates}",
        f"- Inputs: {inputs}",
    ]
    for suite in (*SUITES, PGO_SUITE):
        suite_rows = [r for r in rows if r.suite == suite]
        if not suite_rows:
            continue
        lines += [
            "",
            f"## {suite}",
            "",
            "| Benchmark | Mean | Std dev | Min | Throughput |",
            "|---|--:|--:|--:|--:|",
        ]
        for r in suite_rows:
            cells = (
                r.name,
                format_time(r.mean_s),
                format_time(r.stdev_s) if r.stdev_s else "",
                format_time(r.min_s),
                throughput(r, env.inputs),
            )
            lines.append("| " + " | ".join(cells) + " |")
    return "\n".join(lines) + "\n"


def record_folder(today: str, platform_name: str) -> Path:
    """Find the folder for a recorded run of the day on the platform.

    A later run of the same day takes a letter after the date, so that in order of names the last
    is the newest.
    """
    for letter in ["", *string.ascii_lowercase[1:]]:
        folder = RESULTS / f"{today}{letter}-{platform_name}"
        if not folder.exists():
            return folder
    raise SystemExit(f"every record of {today} on {platform_name} is taken")


def main() -> None:
    """Parse the command line and run the chosen suites."""
    parser = argparse.ArgumentParser(description="Run the benchmark suites and write the results.")
    parser.add_argument("--suite", choices=SUITES, action="append", help="run only these suites")
    parser.add_argument("--mode", choices=MODES, default="default", help="how long to measure")
    parser.add_argument(
        "--pgo", action="store_true", help="run the Rust suites built with a profile as well"
    )
    parser.add_argument(
        "--record", action="store_true", help="move a complete run to benchmarks/results"
    )
    args = parser.parse_args()

    today = datetime.date.today().isoformat()
    record = record_folder(today, PLATFORM_NAMES.get(sys.platform, sys.platform))
    title = record.name
    if args.record and (args.suite or not args.pgo or args.mode not in RECORD_MODES):
        modes = " or ".join(RECORD_MODES)
        raise SystemExit(f"--record needs every suite, --pgo and the mode {modes}")
    shutil.rmtree(LAST_RUN, ignore_errors=True)
    LAST_RUN.mkdir(parents=True)

    corpus.prepare()
    env = environment()
    rows: list[Row] = []
    for suite in args.suite or SUITES:
        if suite == "rust":
            rows += run_rust_suite(args.mode)
        else:
            rows += run_pyperf_suite(suite, args.mode, LAST_RUN)
    if args.pgo:
        rows += run_rust_suite(args.mode, pgo=True)

    results = {"date": today, "mode": args.mode, **asdict(env), "rows": [asdict(r) for r in rows]}
    (LAST_RUN / "results.json").write_text(
        json.dumps(results, indent=1, ensure_ascii=False) + "\n", encoding="utf-8", newline="\n"
    )
    (LAST_RUN / "summary.md").write_text(
        summary(title, args.mode, env, rows), encoding="utf-8", newline="\n"
    )
    out = LAST_RUN
    if args.record:
        RESULTS.mkdir(exist_ok=True)
        out = LAST_RUN.rename(record)
    print(f"results written to {out}")


if __name__ == "__main__":
    main()
