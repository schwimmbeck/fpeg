# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

from pathlib import Path

import fpeg
import pytest


def test_grammar_errors_carry_a_code_and_a_place() -> None:
    with pytest.raises(fpeg.GrammarError) as raised:
        fpeg.Grammar("start: missing")
    (diagnostic,) = raised.value.diagnostics
    assert (diagnostic.code, diagnostic.line, diagnostic.column) == ("E201", 1, 8)
    assert diagnostic.message == "unknown rule missing"
    assert diagnostic.path is None
    assert str(raised.value) == "<grammar>:1:8: error[E201]: unknown rule missing"
    assert isinstance(raised.value, ValueError)


def test_open_reads_a_file_with_a_byte_order_mark_and_names_it_in_messages(
    tmp_path: Path,
) -> None:
    good = tmp_path / "good.fpeg"
    good.write_bytes(b'\xef\xbb\xbfstart: "a"\n')
    assert isinstance(fpeg.Grammar.open(good), fpeg.Grammar)
    bad = tmp_path / "bad.fpeg"
    bad.write_text("start: missing\n", encoding="utf-8")
    with pytest.raises(fpeg.GrammarError) as raised:
        fpeg.Grammar.open(bad)
    assert raised.value.diagnostics[0].path == str(bad)
    assert str(raised.value).startswith(f"{bad}:1:8: error[E201]")
