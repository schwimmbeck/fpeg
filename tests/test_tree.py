# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

import copy
import pickle
from collections.abc import Callable
from typing import Any

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


def test_the_children_of_a_view_read_as_a_list_and_refuse_changes() -> None:
    call = first_call("f(a, b)")
    children = call.children
    assert isinstance(children, list)
    assert children == ["f", "a", "b"]
    changes: list[Callable[[list[Any]], object]] = [
        lambda c: c.append("x"),
        lambda c: c.extend(["x"]),
        lambda c: c.insert(0, "x"),
        lambda c: c.remove("a"),
        lambda c: c.pop(),
        lambda c: c.clear(),
        lambda c: c.sort(),
        lambda c: c.reverse(),
        lambda c: c.__setitem__(0, "x"),
        lambda c: c.__delitem__(0),
        lambda c: c.__iadd__(["x"]),
        lambda c: c.__imul__(2),
    ]
    for change in changes:
        with pytest.raises(TypeError, match="cannot change"):
            change(children)
    assert call.children == ["f", "a", "b"]
    # A copy is a tree built in Python, whose children change freely.
    shallow = copy.copy(call)
    shallow.children.append("x")
    assert shallow.children == ["f", "a", "b", "x"]


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
    # An object equals itself, as in Python's lists.
    odd = fpeg.Tree("x", [float("nan")])
    assert odd == odd


def test_pretty_writes_lark_s_layout_with_fields() -> None:
    tree = parse("f(a) ! k")
    assert tree.pretty() == "start\n  call\n    name=f\n    args=a\n    kw=kw\tk\n"


def test_subtrees_come_children_first_or_parents_first() -> None:
    tree = parse("f(a) ! k, g(b)")
    assert [t.data for t in tree.iter_subtrees()] == ["kw", "call", "call", "start"]
    assert [t.data for t in tree.iter_subtrees_topdown()] == ["start", "call", "kw", "call"]
    assert [t.data for t in tree.find_data("kw")] == ["kw"]
    assert len(list(tree.find_pred(lambda t: t.data == "call"))) == 2
    assert list(tree.scan_values(lambda v: v != "f")) == ["a", "k", "g", "b"]


def test_a_tree_pickles_and_comes_back_equal() -> None:
    tree = parse("f(a), g(b) ! k")
    again = pickle.loads(pickle.dumps(tree))
    assert again == tree
    assert again.children[0].meta.end_column == 5
    assert isinstance(again.children[0].children[0], fpeg.Token)


def test_deep_trees_print_compare_hash_and_pickle_without_recursion() -> None:
    deep = fpeg.Tree("leaf", ["x"])
    for _ in range(5000):
        deep = fpeg.Tree("node", [deep])
    other = pickle.loads(pickle.dumps(deep))
    assert other == deep
    assert hash(other) == hash(deep)
    assert repr(deep).count("Tree(") == 5001
    assert deep.pretty().count("\n") == 5001


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
    assert copy.deepcopy(tree) == tree
