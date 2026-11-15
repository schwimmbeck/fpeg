# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The benchmark chart of the README against the record it names."""

import re
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "benchmarks"))

import chart


def test_the_chart_is_what_its_record_draws() -> None:
    svg = chart.CHART.read_text(encoding="utf-8")
    source = re.search(r"<desc>Drawn by benchmarks/chart\.py from (\S+)\.</desc>", svg)
    assert source is not None
    assert chart.draw(chart.ROOT / source[1]) == svg
