# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

import pickle
import threading
import time
from pathlib import Path

import fpeg
import pytest
from helpers import RECOVERING

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
    # Equal by their parts, as a copy is.
    assert again.diagnostics == raised.value.diagnostics
    assert pickle.loads(pickle.dumps(made)) == made
    assert made != fpeg.Diagnostic("E201", "unknown rule x", 2, 3, 2, 4, None)
    assert len({made, pickle.loads(pickle.dumps(made))}) == 1


def test_check_returns_the_errors_of_a_grammar_instead_of_raising() -> None:
    assert [(found.code, found.severity) for found in fpeg.check("start: missing")] == [
        ("E201", "error")
    ]
    assert [found.code for found in fpeg.check("start: (")] == ["E101"]
    assert fpeg.check(PAIRS) == []
    # In the order of the text, as `Grammar` raises them.
    text = 'start: b a\nstart: "x"\nc: zz\n'
    found = fpeg.check(text)
    assert [(d.code, d.line, d.column) for d in found] == [
        ("E201", 1, 8),
        ("E201", 1, 10),
        ("E204", 2, 1),
        ("E201", 3, 4),
    ]
    with pytest.raises(fpeg.GrammarError) as raised:
        fpeg.Grammar(text)
    assert list(raised.value.diagnostics) == found


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
    missing = tmp_path / "missing.fpeg"
    with pytest.raises(FileNotFoundError) as gone:
        fpeg.Grammar.open(missing)
    assert gone.value.filename == str(missing)


def test_a_grammar_takes_its_modules_from_those_given_then_from_the_files_beside_it(
    tmp_path: Path,
) -> None:
    base = 'start: item+\nitem: WORD\nWORD: [a-z]+\n%ignore " "+\n'
    (tmp_path / "base.fpeg").write_text(base, encoding="utf-8")
    main = tmp_path / "main.fpeg"
    main.write_text('%extends "base"\nitem: ... | N\nN: [0-9]+\n', encoding="utf-8")
    tree = fpeg.Grammar.open(main).parse("ab 12")
    assert isinstance(tree, fpeg.Tree)
    assert tree.children[1] == fpeg.Tree("item", [fpeg.Token("N", "12")])
    # A module given comes before the file of its name.
    given = {"base": 'start: item\nitem: "x"\n'}
    tree = fpeg.Grammar.open(main, modules=given).parse("7")
    assert tree == fpeg.Tree("start", [fpeg.Tree("item", [fpeg.Token("N", "7")])])
    text = '%extends "base"\nstart: item\nitem: "y" => Missing\n'
    assert [d.code for d in fpeg.check(text, modules=given)] == []
    # A function that a module's builder calls, missing, stands at the directive.
    with pytest.raises(fpeg.GrammarError) as raised:
        fpeg.Grammar('%extends "base"', modules={"base": 'start: "x" => Missing\n'})
    (problem,) = raised.value.diagnostics
    assert (problem.code, problem.line, problem.column) == ("E208", 1, 10)
    assert problem.message.startswith("in the grammar base, line 1, column 15: ")


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


def test_a_named_error_carries_its_name_and_what_was_expected() -> None:
    grammar = fpeg.Grammar('start: "a" ("b" ^ "a b must follow")')
    with pytest.raises(fpeg.ParseError) as raised:
        grammar.parse("ac")
    assert raised.value.label is None
    assert raised.value.message == "a b must follow"
    assert raised.value.expected == ('"b"',)
    assert (raised.value.line, raised.value.column) == (1, 2)
    named = fpeg.Grammar('start: "a" (("b" | "c" "d") ^ letter)')
    with pytest.raises(fpeg.ParseError) as raised:
        named.parse("ae")
    assert raised.value.label == "letter"
    assert raised.value.message == 'letter: expected "b", "c"'


def test_a_recovering_parse_raises_with_its_tree_and_every_error() -> None:
    grammar = fpeg.Grammar(RECOVERING)
    tree = grammar.parse("[1, 2]", recover=True)
    assert isinstance(tree, fpeg.Tree) and tree.data == "start"
    with pytest.raises(fpeg.ParseError) as raised:
        grammar.parse("[1, x, 3", recover=True)
    error = raised.value
    # The first error describes the whole, which says each on a line.
    assert (error.label, error.line, error.column) == ("item", 1, 5)
    assert str(error) == (
        'line 1, column 5: item: expected NUMBER\nline 1, column 9: close: expected ",", "]"'
    )
    spans = [(e.label, e.pos, e.end_pos, e.tree, e.errors) for e in error.errors]
    assert spans == [("item", 4, 5, None, ()), ("close", 8, 8, None, ())]
    assert isinstance(error.tree, fpeg.Tree)
    kinds = [getattr(c, "data", getattr(c, "type", None)) for c in error.tree.children]
    assert kinds == ["item", "<error>", "item", "<missing>"]
    # Of many errors, the text lists the first ten and counts the rest.
    with pytest.raises(fpeg.ParseError) as raised:
        grammar.parse("[1, " + ", ".join(["x"] * 12) + "]", recover=True)
    assert len(raised.value.errors) == 12
    lines = str(raised.value).split("\n")
    assert lines == [str(e) for e in raised.value.errors[:10]] + ["and 2 more errors"]
    # A parse that fails even so has no tree, its error after those it recovered from.
    with pytest.raises(fpeg.ParseError) as raised:
        grammar.parse("[1, x] 4", recover=True)
    assert raised.value.tree is None
    messages = [e.message for e in raised.value.errors]
    assert messages == ["item: expected NUMBER", "expected end of input"]
    assert raised.value.message == "expected end of input"
    # Without recover, the first error ends the parse and is the one error.
    with pytest.raises(fpeg.ParseError) as raised:
        grammar.parse("[1, x, 3")
    assert raised.value.tree is None
    assert [str(e) for e in raised.value.errors] == ["line 1, column 5: item: expected NUMBER"]


def test_render_shows_each_error_with_its_line_and_a_caret() -> None:
    grammar = fpeg.Grammar(RECOVERING)
    with pytest.raises(fpeg.ParseError) as raised:
        grammar.parse("[1, x\u00e4, 3", recover=True)
    error = raised.value
    line = "  |\n1 | [1, x\u00e4, 3\n  |"
    item = f"error[item]: expected NUMBER\n --> data.txt:1:5\n{line}     ^^"
    close = f'error[close]: expected ",", "]"\n --> data.txt:1:10\n{line}          ^'
    assert error.render("data.txt") == f"{item}\n\n{close}"
    # An error of `errors` shows itself, and without a path its place alone.
    assert error.errors[1].render() == close.replace("data.txt:", "")
    # The text travels with the error, so a pickled one shows the same.
    assert pickle.loads(pickle.dumps(error)).render("data.txt") == error.render("data.txt")
    # An error that no parse raised has no text to show, only its message.
    assert fpeg.ParseError("bad \x1b[2J value").render() == "error: bad \u241b[2J value"


def test_str_shows_the_controls_that_a_grammar_wrote_as_marks() -> None:
    with pytest.raises(fpeg.ParseError) as raised:
        fpeg.Grammar(r'start: "a" ("b" ^ "bad\x1b[2J")').parse("ac")
    assert raised.value.message == "bad\x1b[2J"
    assert str(raised.value) == "line 1, column 2: bad\u241b[2J"
    with pytest.raises(fpeg.ParseError) as raised:
        fpeg.Grammar('start: A\nA "x\\u{202e}y": "a"').parse("b")
    assert str(raised.value) == "line 1, column 1: expected x\ufffdy"
    # A diagnostic may quote the grammar.
    diagnostic = fpeg.Diagnostic("E101", "bad\x1b[2J", 1, 1, 1, 1)
    assert diagnostic.message == "bad\x1b[2J"
    assert str(diagnostic) == "<grammar>:1:1: error[E101]: bad\u241b[2J"


# Errors of a parse shown one at a time.
ERRORS_SHOWN = 1_000
# Code points of the text after those errors, too many to index again for each.
LONG_TEXT_CHARS = 4_000_000
# Time that showing the errors stays well within where they share the index of the text, which
# indexing the text for each takes many times.
SHOW_SECONDS = 1.0


def test_showing_the_errors_of_a_parse_one_at_a_time_costs_what_they_show() -> None:
    grammar = fpeg.Grammar(RECOVERING)
    text = "[1" + ", x" * (ERRORS_SHOWN - 1) + ", " + "\u00e9" * LONG_TEXT_CHARS + "]"
    with pytest.raises(fpeg.ParseError) as raised:
        grammar.parse(text, recover=True)
    errors = raised.value.errors
    assert len(errors) == ERRORS_SHOWN
    started = time.perf_counter()
    shown = [error.render() for error in errors]
    assert time.perf_counter() - started < SHOW_SECONDS
    assert shown[-1] == raised.value.render().split("\n\n")[-1]


def test_limits_end_a_parse_with_a_limit_error() -> None:
    grammar = fpeg.Grammar(PAIRS)
    with pytest.raises(fpeg.LimitError) as raised:
        grammar.parse(", ".join(["x = 1"] * 100), max_steps=50)
    assert raised.value.limit == "steps"
    nested = fpeg.Grammar('start: "(" start? ")"')
    with pytest.raises(fpeg.LimitError) as raised:
        nested.parse("(" * 40 + ")" * 40, max_depth=10)
    assert raised.value.limit == "depth"
    with pytest.raises(fpeg.LimitError) as raised:
        grammar.parse(", ".join(["x = 1"] * 1_000), max_memory=1_000)
    assert raised.value.limit == "memory"


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
