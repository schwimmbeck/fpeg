# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

from typing import Any

import fpeg
import pytest

CALLS = fpeg.Grammar(r"""
start: call ("," call)*
call: name=NAME "(" args=(arg ** ",") ")" kw=kw?
?arg: NAME | NUMBER | "[" (arg ** ",") "]" -> group
kw: "!" NAME
NAME: [a-z]+
NUMBER: [0-9]+
%ignore " "+
""")


def parse(text: str) -> fpeg.Tree:
    tree = CALLS.parse(text)
    assert isinstance(tree, fpeg.Tree)
    return tree


class Order(fpeg.Visitor):
    """Records the nodes in the order of their calls."""

    def __init__(self) -> None:
        self.seen: list[str] = []

    def start(self, node: fpeg.Tree) -> None:
        self.seen.append("start")

    def call(self, name: fpeg.Token, args: list[Any], kw: fpeg.Tree | None) -> None:
        self.seen.append(f"call {name}")

    def group(self, *children: Any) -> None:
        self.seen.append(f"group of {len(children)}")


def test_visit_calls_children_before_their_parents_and_topdown_the_reverse() -> None:
    tree = parse("f([a, [b]]), g()")
    bottom_up = Order()
    bottom_up.visit(tree)
    assert bottom_up.seen == ["group of 1", "group of 2", "call f", "call g", "start"]
    top_down = Order()
    top_down.visit_topdown(tree)
    assert top_down.seen == ["start", "call f", "group of 2", "group of 1", "call g"]


def test_a_method_gets_the_parts_of_its_node_as_parsed() -> None:
    seen: list[Any] = []

    class Parts(fpeg.Visitor):
        def call(self, name: Any, args: Any, kw: Any, *, meta: fpeg.Meta) -> None:
            seen.append((str(name), [str(arg) for arg in args], kw, meta.column))

        def kw(self, node: fpeg.Tree) -> None:
            seen.append(node.data)

        def start(self) -> None:
            seen.append("start")

    Parts().visit(parse("f(a, 1), g()!x"))
    assert seen[0] == ("f", ["a", "1"], None, 1)
    assert seen[1] == "kw"
    name, args, kw, column = seen[2]
    assert (name, args, kw.data, column) == ("g", [], "kw", 10)
    assert seen[3] == "start"


def test_the_tree_comes_back_as_it_is_whatever_the_methods_return() -> None:
    class Returning(fpeg.Visitor):
        def call(self, node: fpeg.Tree) -> str:
            return "dropped"

    tree = parse("f()")
    assert Returning().visit(tree) is tree
    assert Returning().visit_topdown(tree) is tree
    # A token has no nodes to call methods for.
    token = tree.children[0].children[0]
    assert Returning().visit(token) is token
    # An absent optional field and a field of several items are no trees.
    for wrong in [None, [tree]]:
        with pytest.raises(TypeError, match="visit takes a node or a token"):
            Returning().visit(wrong)
        with pytest.raises(TypeError, match="visit takes a node or a token"):
            Returning().visit_topdown(wrong)


def test_a_method_named_like_a_terminal_is_an_error() -> None:
    class Tokens(fpeg.Visitor):
        def NAME(self, token: fpeg.Token) -> None:
            pass

    with pytest.raises(TypeError, match=r"Tokens\.NAME is named like a terminal"):
        Tokens().visit(parse("f()"))


def test_an_error_gets_a_note_with_the_method_and_the_place_of_its_node() -> None:
    class Failing(fpeg.Visitor):
        def kw(self, node: fpeg.Tree) -> None:
            raise ValueError("no")

    with pytest.raises(ValueError, match="no") as raised:
        Failing().visit(parse("f(), g()!x"))
    (note,) = raised.value.__notes__
    assert note.endswith("Failing.kw, for the node kw at line 1, column 9")


def test_strict_makes_every_public_method_name_a_rule() -> None:
    class Typo(fpeg.Visitor, strict=True):
        def calls(self, node: fpeg.Tree) -> None:
            pass

    with pytest.raises(TypeError, match=r"Typo\.calls names no rule.*did you mean call"):
        Typo().visit(parse("f()"))


def test_a_tree_built_in_python_is_visited_by_the_names_of_its_data() -> None:
    seen: list[int] = []

    class Count(fpeg.Visitor):
        def add(self, *children: Any) -> None:
            seen.append(len(children))

    Count().visit(fpeg.Tree("add", [1, fpeg.Tree("add", [2, 3, 4])]))
    assert seen == [3, 2]


def test_a_tree_of_any_depth_is_visited_without_recursion() -> None:
    deep = fpeg.Grammar('start: item\n?item: NAME | "(" item ")" -> group\nNAME: [a-z]+')
    count = 0

    class Count(fpeg.Visitor):
        def group(self) -> None:
            nonlocal count
            count += 1

    Count().visit(deep.parse("(" * 5_000 + "a" + ")" * 5_000, max_depth=20_000))
    assert count == 5_000
