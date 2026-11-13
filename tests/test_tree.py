# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

import copy

import fpeg
import pytest

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


def test_a_field_gives_one_item_or_a_list_by_how_many_it_can_hold() -> None:
    call = first_call("f(a, b) ! k")
    assert call.field("name") == "f"
    assert call.field("args") == ["a", "b"]
    kw = call.field("kw")
    assert isinstance(kw, fpeg.Tree)
    assert kw.data == "kw"
    bare = first_call("g()")
    assert bare.field("args") == []
    assert bare.field("kw") is None
    with pytest.raises(KeyError, match="call has no field nope, its fields are"):
        bare.field("nope")


def test_text_is_the_input_a_node_spans() -> None:
    assert first_call("f( a ,b )").text == "f( a ,b )"
    assert fpeg.Tree("x", []).text is None


def test_trees_built_in_python_equal_views_of_the_same_shape() -> None:
    call = first_call("f(a)")
    built = fpeg.Tree("call", [fpeg.Token("NAME", "f"), fpeg.Token("NAME", "a")])
    assert call == built
    assert hash(call) == hash(built)
    assert call != fpeg.Tree("call", ["f"])
    assert call != "call"


def test_pretty_writes_lark_s_layout_with_fields() -> None:
    tree = parse("f(a) ! k")
    assert tree.pretty() == "start\n  call\n    name=f\n    args=a\n    kw=kw\tk\n"


def test_a_tree_that_contains_itself_is_refused() -> None:
    tree = fpeg.Tree("loop", [])
    tree.children.append(tree)
    with pytest.raises(ValueError, match="contains itself"):
        repr(tree)
    with pytest.raises(ValueError, match="contains itself"):
        hash(tree)


def test_a_copy_has_a_new_list_of_the_same_children() -> None:
    tree = parse("f(a), g(b)")
    shallow = copy.copy(tree)
    assert shallow == tree
    assert shallow.children is not tree.children
    assert shallow.children[0] is tree.children[0]


def test_deep_trees_print_compare_and_hash_without_recursion() -> None:
    def deep() -> fpeg.Tree:
        tree = fpeg.Tree("leaf", ["x"])
        for _ in range(5000):
            tree = fpeg.Tree("node", [tree])
        return tree

    one, other = deep(), deep()
    assert other == one
    assert hash(other) == hash(one)
    assert repr(one).count("Tree(") == 5001
    assert one.pretty().count("\n") == 5001
