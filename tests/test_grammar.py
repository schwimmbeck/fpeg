# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

import pickle
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


def test_a_grammar_error_pickles_with_its_diagnostics() -> None:
    with pytest.raises(fpeg.GrammarError) as raised:
        fpeg.Grammar("start: missing")
    again = pickle.loads(pickle.dumps(raised.value))
    assert str(again) == str(raised.value)
    (diagnostic,) = again.diagnostics
    assert repr(diagnostic) == repr(raised.value.diagnostics[0])
    made = fpeg.Diagnostic("E201", "unknown rule x", 2, 3, 2, 4, "g.fpeg")
    assert str(pickle.loads(pickle.dumps(made))) == "g.fpeg:2:3: error[E201]: unknown rule x"


def test_check_returns_the_errors_of_a_grammar_instead_of_raising() -> None:
    assert [(found.code, found.severity) for found in fpeg.check("start: missing")] == [
        ("E201", "error")
    ]
    assert [found.code for found in fpeg.check("start: (")] == ["E101"]
    assert fpeg.check(PAIRS) == []


def test_check_returns_the_warnings_of_a_grammar_that_compiles() -> None:
    text = 'start: "a"? | "b"'
    (found,) = fpeg.check(text)
    assert (found.code, found.severity, found.line, found.column) == ("W101", "warning", 1, 15)
    assert str(found).startswith("<grammar>:1:15: warning[W101]: this alternative is never tried")
    assert isinstance(fpeg.Grammar(text).parse("a"), fpeg.Tree)


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


def test_parse_errors_place_the_failure_in_code_points_and_name_what_was_expected() -> None:
    grammar = fpeg.Grammar(PAIRS)
    with pytest.raises(fpeg.ParseError) as raised:
        grammar.parse("ää = 1, b = ,")
    error = raised.value
    assert (error.pos, error.line, error.column) == (12, 1, 13)
    assert error.expected == ("NAME", "NUMBER")
    assert error.label is None
    assert error.message == "expected NAME, NUMBER"
    assert str(error) == "line 1, column 13: expected NAME, NUMBER"
    assert isinstance(error, ValueError)
    assert not isinstance(error, fpeg.LimitError)


def test_a_labelled_error_carries_its_label() -> None:
    grammar = fpeg.Grammar('start: "a" ("b" ^ "a b must follow")')
    with pytest.raises(fpeg.ParseError) as raised:
        grammar.parse("ac")
    assert raised.value.label == "a b must follow"
    assert raised.value.expected == ()
    assert (raised.value.line, raised.value.column) == (1, 2)


def test_limits_end_a_parse_with_a_limit_error() -> None:
    grammar = fpeg.Grammar(PAIRS)
    with pytest.raises(fpeg.LimitError) as raised:
        grammar.parse(", ".join(["x = 1"] * 100), max_steps=50)
    assert raised.value.limit == "steps"
    nested = fpeg.Grammar('start: "(" start? ")"')
    with pytest.raises(fpeg.LimitError) as raised:
        nested.parse("(" * 40 + ")" * 40, max_depth=10)
    assert raised.value.limit == "depth"


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
