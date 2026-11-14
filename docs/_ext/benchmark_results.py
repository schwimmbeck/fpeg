# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The directive `benchmark-results`: the newest recorded benchmark run of each platform.

`benchmarks/run.py --record` writes each run to `benchmarks/results/<date>-<platform>`, with
its tables in `summary.md`. The directive reads the newest folder of each platform and puts
its tables into the page as they stand, a level of heading lower, so that the page shows what
was measured and never a number typed by hand.
"""

from pathlib import Path
from typing import Any

from docutils import nodes
from docutils.statemachine import StringList
from sphinx.application import Sphinx
from sphinx.util.docutils import SphinxDirective

RESULTS = Path(__file__).resolve().parents[2] / "benchmarks" / "results"


def newest_runs() -> dict[str, Path]:
    """Find the summary of the newest run of each platform, by platform name."""
    newest: dict[str, Path] = {}
    # A folder is named `<date>-<platform>`, so in order of names the last is the newest.
    for folder in sorted(RESULTS.iterdir()):
        summary = folder / "summary.md"
        if summary.is_file():
            newest[folder.name.split("-", 3)[3]] = summary
    return newest


class BenchmarkResults(SphinxDirective):
    """Put the tables of the newest run of each platform into the page."""

    def run(self) -> list[nodes.Node]:
        """Parse each summary as part of the page, its headings a level lower."""
        lines: list[str] = []
        for summary in newest_runs().values():
            self.env.note_dependency(str(summary))
            for line in summary.read_text(encoding="utf-8").splitlines():
                lines.append("#" + line if line.startswith("#") else line)
            lines.append("")
        container = nodes.container()
        self.state.nested_parse(StringList(lines), self.content_offset, container)
        return container.children


def setup(app: Sphinx) -> dict[str, Any]:
    """Register the directive."""
    app.add_directive("benchmark-results", BenchmarkResults)
    return {"parallel_read_safe": True, "parallel_write_safe": True}
