# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

import threading
from pathlib import Path

import fpeg
import pytest

PAIRS = r"""
start: pair ("," pair)*
pair: key=NAME "=" value=(NAME | NUMBER)
NAME: [a-z\u{e4}]+
NUMBER: [0-9]+
%ignore " "+
"""


def test_a_grammar_parses_from_its_start_rule() -> None:
    tree = fpeg.Grammar(PAIRS).parse("x = 1, y = z")
    assert isinstance(tree, fpeg.Tree)
    assert tree.data == "start"
    assert [pair.data for pair in tree.children] == ["pair", "pair"]


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
    tree = fpeg.Grammar.open(good).parse("a")
    assert isinstance(tree, fpeg.Tree)
    assert tree.data == "start"
    bad = tmp_path / "bad.fpeg"
    bad.write_text("start: missing\n", encoding="utf-8")
    with pytest.raises(fpeg.GrammarError) as raised:
        fpeg.Grammar.open(bad)
    assert raised.value.diagnostics[0].path == str(bad)
    assert str(raised.value).startswith(f"{bad}:1:8: error[E201]")


def test_a_parse_returns_its_one_visible_item_a_token_or_a_tree() -> None:
    grammar = fpeg.Grammar('?start: value\n?value: NUMBER | "[" value "]" -> list\nNUMBER: [0-9]+')
    number = grammar.parse("7")
    assert isinstance(number, fpeg.Token)
    assert (number.type, number) == ("NUMBER", "7")
    nested = grammar.parse("[7]")
    assert isinstance(nested, fpeg.Tree)
    assert nested.data == "list"


def test_an_unknown_start_rule_is_a_value_error() -> None:
    with pytest.raises(ValueError, match="no rule nothing"):
        fpeg.Grammar(PAIRS).parse("x = 1", "nothing")


def test_a_start_rule_that_may_leave_several_items_is_refused() -> None:
    grammar = fpeg.Grammar('start: _pair\n_pair: A A\nA: "a"')
    assert isinstance(grammar.parse("aa"), fpeg.Tree)
    with pytest.raises(ValueError, match="no visible item or several"):
        grammar.parse("aa", "_pair")


def test_a_failed_parse_is_a_value_error_saying_what_was_expected() -> None:
    with pytest.raises(ValueError, match="expected NAME, NUMBER"):
        fpeg.Grammar(PAIRS).parse("x = ")


def test_an_input_with_a_lone_surrogate_cannot_be_parsed() -> None:
    with pytest.raises(UnicodeEncodeError):
        fpeg.Grammar(PAIRS).parse("x = \ud800")


def test_one_grammar_parses_in_many_threads_at_once() -> None:
    grammar = fpeg.Grammar(PAIRS)
    # Long enough that the parse releases the interpreter.
    text = ", ".join(f"{chr(97 + i % 26)} = {i}" for i in range(2000))
    counts: list[int] = []

    def parse() -> None:
        for _ in range(5):
            tree = grammar.parse(text)
            assert isinstance(tree, fpeg.Tree)
            counts.append(len(tree.children))

    threads = [threading.Thread(target=parse) for _ in range(8)]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    assert counts == [2000] * 40
