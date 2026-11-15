# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Draw the benchmark chart of the README from a recorded run.

Three groups of bars compare fpeg with other parsers, time per input, from the ``results.json``
of a run in ``benchmarks/results``. The chart is one SVG on a card of its own colour, so that it
reads on light and dark pages and on PyPI, which cannot switch an image by theme. ``run.py
--record`` draws it from each new record, and a test holds the committed chart to the record it
names in its description.
"""

import argparse
import json
from dataclasses import dataclass
from enum import Enum
from pathlib import Path
from xml.sax.saxutils import escape

BENCHMARKS = Path(__file__).resolve().parent
ROOT = BENCHMARKS.parent
CHART = ROOT / "docs" / "_static" / "benchmarks.svg"


class Kind(Enum):
    """What a bar measures, with its legend and colour."""

    FPEG = ("fpeg", "#E5432A")
    RUNTIME = ("other parsers of a grammar loaded at runtime", "#8C939C")
    NATIVE = ("parsers built for one format", "#4F5864")

    def __init__(self, legend: str, colour: str) -> None:
        """Take the legend and the colour from the value of the member."""
        self.legend = legend
        self.colour = colour


@dataclass(frozen=True)
class Bar:
    """A benchmark of a recorded run, by its suite and name, and how the chart labels it."""

    suite: str
    name: str
    label: str
    kind: Kind


@dataclass(frozen=True)
class Group:
    """The bars over one input, drawn from the fastest to the slowest."""

    title: str
    input: str
    # What the input is, with {lines} and {megabytes} taken from the record.
    about: str
    bars: tuple[Bar, ...]


GROUPS = (
    Group(
        "JSON to Python objects",
        "records.json",
        "records.json, {megabytes} MB",
        (
            Bar("json", "json.loads", "json.loads", Kind.NATIVE),
            Bar(
                "json",
                "fpeg parse and grammar builders, Python objects",
                "fpeg, builders",
                Kind.FPEG,
            ),
            Bar(
                "json",
                "fpeg parse and Transformer methods, Python objects",
                "fpeg, Transformer",
                Kind.FPEG,
            ),
            Bar("json", "pe machine, Python objects", "pe, machine", Kind.RUNTIME),
            Bar("json", "pe packrat, Python objects", "pe, packrat", Kind.RUNTIME),
            Bar(
                "json",
                "Lark LALR with inlined Transformer, Python objects",
                "Lark, LALR",
                Kind.RUNTIME,
            ),
        ),
    ),
    Group(
        "Python source to a tree",
        "python_transformer.py",
        "a file of {lines} lines",
        (
            Bar(
                "python",
                "CPython parser and symbol table, no AST objects",
                "CPython, no objects",
                Kind.NATIVE,
            ),
            Bar(
                "python", "tree-sitter parse, native tree", "tree-sitter, native tree", Kind.NATIVE
            ),
            Bar("python", "ast.parse", "ast.parse", Kind.NATIVE),
            Bar("python", "fpeg parse, tree", "fpeg, native tree", Kind.FPEG),
            Bar(
                "python",
                "fpeg parse and transform to ast, positions stamped natively",
                "fpeg, to ast",
                Kind.FPEG,
            ),
            Bar("python", "LibCST parse_module", "LibCST", Kind.NATIVE),
            Bar("python", "Lark LALR, bundled Python grammar, tree", "Lark, LALR", Kind.RUNTIME),
        ),
    ),
    Group(
        "A calculator, parsed and computed",
        "calculator.txt",
        "{lines} lines of arithmetic",
        (
            Bar("calculator", "fpeg parse and Transformer, numbers", "fpeg", Kind.FPEG),
            Bar("calculator", "eval, a line at a time", "Python's eval", Kind.NATIVE),
            Bar("calculator", "pe machine, numbers", "pe, machine", Kind.RUNTIME),
            Bar(
                "calculator",
                "Lark LALR with inlined Transformer, numbers",
                "Lark, LALR",
                Kind.RUNTIME,
            ),
            Bar("calculator", "pe packrat, numbers", "pe, packrat", Kind.RUNTIME),
        ),
    ),
)

WIDTH_PX = 800
MARGIN_PX = 24
CARD_CORNER_PX = 10
# Labels end left of this column and bars start at it.
LABEL_COLUMN_PX = 200
# Room right of the longest bar for its value.
VALUE_COLUMN_PX = 90
BAR_PX = 18
BAR_CORNER_PX = 3
# A bar for the shortest time stays visible.
MIN_BAR_PX = 2
BAR_GAP_PX = 8
# From the top of a bar to the baseline of its label and value.
BAR_BASELINE_PX = 13
# Between a bar and its label or value.
TEXT_GAP_PX = 8
HEADING_GAP_PX = 14
GROUP_GAP_PX = 26
LINE_PX = 17
NOTES_GAP_PX = 22
SWATCH_PX = 12
SWATCH_CORNER_PX = 2
# From the top of a swatch to the baseline of its text.
SWATCH_BASELINE_PX = 10
LEGEND_GAP_PX = 28
# About the advance of a character of the legend's 12 px text, to place its entries.
LEGEND_CHAR_PX = 5.9
# Times from here on show without a decimal.
WHOLE_MS_FROM_S = 0.1
TEXT = "#1F2328"
SUBTLE = "#59636E"
CARD = "#FFFFFF"
BORDER = "#D1D9E0"
STYLE = (
    f".h{{font-size:15px;font-weight:600;fill:{TEXT}}}"
    f".l{{font-size:13px;fill:{TEXT}}}"
    f".v{{font-size:12px;fill:{TEXT};font-variant-numeric:tabular-nums}}"
    f".s{{font-size:12px;fill:{SUBTLE}}}"
)
SCRIPTS = "github.com/schwimmbeck/fpeg/tree/main/benchmarks"


def shown(seconds: float) -> str:
    """Return a time in milliseconds as the chart writes it."""
    milliseconds = seconds * 1e3
    if seconds >= WHOLE_MS_FROM_S:
        return f"{milliseconds:,.0f} ms"
    return f"{milliseconds:.1f} ms"


def system(platform: str) -> str:
    """Return the name of an operating system from Python's platform string."""
    name, _, rest = platform.partition("-")
    return f"{name} {rest.split('-')[0]}" if name == "Windows" else name


def text(x: float, y: float, css: str, content: str, anchor: str = "start") -> str:
    """Return an SVG text element."""
    end = "" if anchor == "start" else f' text-anchor="{anchor}"'
    return f'<text x="{x:g}" y="{y:g}" class="{css}"{end}>{escape(content)}</text>'


def bars(group: Group, means: dict[tuple[str, str], float], top: float) -> tuple[list[str], float]:
    """Return the elements of a group's bars from a height on, and the height after them."""
    parts: list[str] = []
    span = WIDTH_PX - LABEL_COLUMN_PX - VALUE_COLUMN_PX
    ordered = sorted(group.bars, key=lambda bar: means[(bar.suite, bar.name)])
    slowest = means[(ordered[-1].suite, ordered[-1].name)]
    y = top
    for bar in ordered:
        seconds = means[(bar.suite, bar.name)]
        length = round(max(span * seconds / slowest, MIN_BAR_PX), 1)
        baseline = y + BAR_BASELINE_PX
        parts.append(text(LABEL_COLUMN_PX - TEXT_GAP_PX, baseline, "l", bar.label, "end"))
        parts.append(
            f'<rect x="{LABEL_COLUMN_PX}" y="{y:g}" width="{length:g}" height="{BAR_PX}" '
            f'rx="{BAR_CORNER_PX}" fill="{bar.kind.colour}"/>'
        )
        value_x = round(LABEL_COLUMN_PX + length + TEXT_GAP_PX, 1)
        parts.append(text(value_x, baseline, "v", shown(seconds)))
        y += BAR_PX + BAR_GAP_PX
    return parts, y


def draw(run: Path) -> str:
    """Return the chart of a recorded run as SVG text."""
    record = json.loads((run / "results.json").read_text(encoding="utf-8"))
    means = {(row["suite"], row["name"]): row["mean_s"] for row in record["rows"]}
    missing = [b.name for g in GROUPS for b in g.bars if (b.suite, b.name) not in means]
    if missing:
        raise SystemExit(f"{run.name} has no result for: {', '.join(missing)}")
    parts: list[str] = []
    y = float(MARGIN_PX)
    for group in GROUPS:
        size = record["inputs"][group.input]
        about = group.about.format(
            lines=f"{size['lines']:,}", megabytes=f"{size['bytes'] / 1e6:.1f}"
        )
        parts.append(text(MARGIN_PX, y, "h", group.title))
        parts.append(text(WIDTH_PX - MARGIN_PX, y, "s", about, "end"))
        group_parts, y = bars(group, means, y + HEADING_GAP_PX)
        parts += group_parts
        y += GROUP_GAP_PX
    x = float(MARGIN_PX)
    for kind in Kind:
        parts.append(
            f'<rect x="{x:g}" y="{y - SWATCH_BASELINE_PX:g}" width="{SWATCH_PX}" '
            f'height="{SWATCH_PX}" rx="{SWATCH_CORNER_PX}" fill="{kind.colour}"/>'
        )
        parts.append(text(x + SWATCH_PX + TEXT_GAP_PX, y, "s", kind.legend))
        x = round(
            x + SWATCH_PX + TEXT_GAP_PX + LEGEND_CHAR_PX * len(kind.legend) + LEGEND_GAP_PX, 1
        )
    packages = ", ".join(f"{name} {version}" for name, version in record["packages"].items())
    notes = [
        "Time per input, the mean of pyperf's runs, shorter is faster. Each parser runs in its "
        "fastest setting.",
        "Rows marked native tree or no objects make no Python objects. The others make them.",
        f"Recorded {record['date']}: {record['cpu']}, {system(record['platform'])}, "
        f"{record['python']}.",
        f"fpeg at {record['commit']}, {packages}.",
        f"Scripts and raw results: {SCRIPTS}",
    ]
    y += NOTES_GAP_PX
    for note in notes:
        parts.append(text(MARGIN_PX, y, "s", note))
        y += LINE_PX
    height = f"{y:g}"
    source = run.resolve().relative_to(ROOT).as_posix()
    lines = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH_PX}" height="{height}" '
        f'viewBox="0 0 {WIDTH_PX} {height}" role="img" '
        'font-family="system-ui, -apple-system, Segoe UI, sans-serif">',
        "<title>fpeg and other parsers, time per input</title>",
        f"<desc>Drawn by benchmarks/chart.py from {source}.</desc>",
        f"<style>{STYLE}</style>",
        f'<rect width="{WIDTH_PX}" height="{height}" rx="{CARD_CORNER_PX}" fill="{CARD}" '
        f'stroke="{BORDER}"/>',
        *parts,
        "</svg>",
    ]
    return "\n".join(lines) + "\n"


def write(run: Path) -> None:
    """Draw the chart of a recorded run into the docs."""
    CHART.write_text(draw(run), encoding="utf-8", newline="\n")


def main() -> None:
    """Parse the command line and draw the chart."""
    parser = argparse.ArgumentParser(description="Draw the README's chart from a recorded run.")
    parser.add_argument("run", type=Path, help="a folder in benchmarks/results")
    args = parser.parse_args()
    write(args.run)
    print(f"drew {CHART.relative_to(ROOT).as_posix()} from {args.run.name}")


if __name__ == "__main__":
    main()
