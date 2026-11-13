# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

import fpeg

CALLS = r"""
start: call ("," call)*
call: name=NAME "(" args=(NAME ** ",") ")" kw=kw?
kw: "!" NAME
NAME: [a-z\u{e4}]+
%ignore " "+
"""


def parse(text: str) -> fpeg.Tree:
    tree = fpeg.Grammar(CALLS).parse(text)
    assert isinstance(tree, fpeg.Tree)
    return tree


def first_call(text: str) -> fpeg.Tree:
    call = parse(text).children[0]
    assert isinstance(call, fpeg.Tree)
    return call


def test_a_view_shows_its_data_and_visible_children() -> None:
    call = first_call("f(a, b)")
    assert call.data == "call"
    assert call.children == ["f", "a", "b"]
    assert [token.type for token in call.children] == ["NAME", "NAME", "NAME"]


def test_the_children_of_a_view_keep_their_identity() -> None:
    tree = parse("f(a), g(b)")
    assert tree.children is tree.children
    assert tree.children[0] is tree.children[0]


def test_meta_and_token_positions_count_code_points() -> None:
    call = first_call("ä(b) ")
    meta = call.meta
    assert (meta.line, meta.column, meta.end_line, meta.end_column) == (1, 1, 1, 5)
    assert (meta.start_pos, meta.end_pos, meta.empty) == (0, 4, False)
    b = call.children[1]
    assert isinstance(b, fpeg.Token)
    assert (b.start_pos, b.end_pos, b.line, b.column, b.end_column) == (2, 3, 1, 3, 4)


def test_text_is_the_input_a_node_spans() -> None:
    assert first_call("f( a ,b )").text == "f( a ,b )"
    assert fpeg.Tree("x", []).text is None
