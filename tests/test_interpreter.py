# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

from typing import Any

import fpeg
import pytest

# A small language: assignments, and an `if` that must not run the branch it does not take.
PROGRAMS = fpeg.Grammar(r"""
start: statement*
?statement: assign | if_ | block
assign: target=NAME "=" value=expr ";"
if_: "if" test=expr then=block ("else" otherwise=block)?
block: "{" statement* "}"
?expr: name=NAME -> load | NUMBER | "[" items=(expr ** ",") "]" -> list_
NAME: [a-z]+
NUMBER: [0-9]+ => int
%ignore [ \n]+
%word [a-z]
""")


def parse(text: str) -> fpeg.Tree:
    tree = PROGRAMS.parse(text)
    assert isinstance(tree, fpeg.Tree)
    return tree


class Run(fpeg.Interpreter):
    """Runs a program and returns its variables."""

    def __init__(self) -> None:
        self.variables: dict[str, Any] = {}

    def start(self, *statements: fpeg.Tree) -> dict[str, Any]:
        for statement in statements:
            self.visit(statement)
        return self.variables

    def assign(self, target: fpeg.Token, value: Any) -> None:
        self.variables[str(target)] = self.visit(value)

    def if_(self, test: Any, then: fpeg.Tree, otherwise: fpeg.Tree | None) -> None:
        if self.visit(test):
            self.visit(then)
        elif otherwise is not None:
            self.visit(otherwise)

    def load(self, name: fpeg.Token) -> Any:
        return self.variables[str(name)]

    def list_(self, items: list[Any]) -> list[Any]:
        return [self.visit(item) for item in items]


def test_a_method_visits_what_it_asks_for_and_nothing_else() -> None:
    variables = Run().visit(parse("x = 1; if x { y = 2; } else { y = z; } if 0 { y = z; }"))
    # The branches not taken read `z`, which no statement sets.
    assert variables == {"x": 1, "y": 2}


def test_parameters_named_like_fields_get_the_fields_as_parsed() -> None:
    seen: list[Any] = []

    class Fields(fpeg.Interpreter):
        def if_(self, test: Any, then: Any, otherwise: Any) -> None:
            seen.append((test, then.data, otherwise))

        def list_(self, items: Any) -> None:
            seen.append([str(item) for item in items])

    Fields().visit(parse("if 7 { } x = [1, 2];"))
    token, then, otherwise = seen[0]
    # One item, as a view or a token, `None` for an absent field, a list for a field of several.
    assert (token, token.type, then, otherwise) == ("7", "NUMBER", "block", None)
    assert seen[1] == ["1", "2"]


def test_one_parameter_that_names_no_field_gets_the_node() -> None:
    class Nodes(fpeg.Interpreter):
        def assign(self, node: fpeg.Tree) -> tuple[str, str]:
            return (node.data, str(node.field("target")))

    assert Nodes().visit(parse("x = 1;")) == [("assign", "x")]


def test_a_starred_parameter_gets_the_children_as_parsed_and_none_nothing() -> None:
    class Children(fpeg.Interpreter):
        def assign(self, *children: Any) -> list[str]:
            return [str(child) for child in children]

        def block(self) -> str:
            return "block"

    assert Children().visit(parse("x = 1; { y = 2; }")) == [["x", "1"], "block"]


def test_keyword_only_parameters_give_the_node_its_place_and_its_children() -> None:
    class Extras(fpeg.Interpreter):
        def assign(self, target: Any, *, meta: fpeg.Meta, node: fpeg.Tree, children: Any) -> Any:
            return (str(target), meta.column, node.data, len(children))

    assert Extras().visit(parse("  x = 1;")) == [("x", 3, "assign", 2)]


def test_a_node_without_a_method_gives_the_list_of_its_visited_children() -> None:
    class Names(fpeg.Interpreter):
        def NAME(self, token: fpeg.Token) -> str:
            return token.upper()

    assert Names().visit(parse("x = [y, 1];")) == [["X", [["Y"], 1]]]


def test_a_token_gives_its_method_else_its_builder_else_itself() -> None:
    class Words(fpeg.Interpreter):
        def NAME(self) -> str:
            return "name"

    tree = parse("x = 1;")
    name, number = tree.children[0].children
    assert Words().visit(name) == "name"
    # The builder of NUMBER is `int`. NAME has none.
    assert fpeg.Interpreter().visit(number) == 1
    assert fpeg.Interpreter().visit(name) is name


def test_a_subclass_is_called_by_its_own_methods_after_its_base_was_used() -> None:
    class Base(fpeg.Interpreter):
        pass

    name = parse("x = 1;").children[0].children[0]
    assert Base().visit(name) is name
    assert Base().visit(fpeg.Tree("made", [name])) == [name]

    class Sub(Base):
        def NAME(self) -> str:
            return "name"

        def made(self) -> str:
            return "made"

    assert Sub().visit(name) == "name"
    assert Sub().visit(fpeg.Tree("made", [name])) == "made"


def test_the_builder_of_a_rule_is_not_applied() -> None:
    built = fpeg.Grammar('start: pair\npair: NAME NAME => tuple\nNAME: [a-z]+\n%ignore " "+')
    assert fpeg.Interpreter().visit(built.parse("a b")) == [["a", "b"]]


def test_visiting_none_or_a_list_is_an_error() -> None:
    class Careless(fpeg.Interpreter):
        def if_(self, test: Any, then: Any, otherwise: Any) -> Any:
            return self.visit(otherwise)

        def list_(self, items: Any) -> Any:
            return self.visit(items)

    with pytest.raises(TypeError, match="got None, as an absent optional field is") as raised:
        Careless().visit(parse("if 1 { }"))
    (note,) = raised.value.__notes__
    assert note.endswith("Careless.if_, for the node if_ at line 1, column 1")
    with pytest.raises(TypeError, match="got a list"):
        Careless().visit(parse("x = [1];"))


def test_an_error_gets_one_note_from_the_innermost_method() -> None:
    with pytest.raises(KeyError) as raised:
        Run().visit(parse("x = 1;\nif x { y = [x, zz]; }"))
    assert raised.value.__notes__ == ["fpeg: in Run.load, for the node load at line 2, column 16"]

    class Tokens(fpeg.Interpreter):
        def NUMBER(self, token: fpeg.Token) -> int:
            raise ValueError(str(token))

    with pytest.raises(ValueError, match="7") as failed:
        Tokens().visit(parse("x = 7;"))
    (note,) = failed.value.__notes__
    assert note.endswith("Tokens.NUMBER, for the token NUMBER at line 1, column 5")

    class Defaults(fpeg.Interpreter):
        def start(self, node: fpeg.Tree) -> list[Any]:
            return [self.visit(child) for child in node.children]

        def __default__(self, node: fpeg.Tree) -> Any:
            raise ValueError(node.data)

    # Below a method that visits the node, and at the top.
    tree = parse("x = 7;")
    for top in [tree, tree.children[0]]:
        with pytest.raises(ValueError) as defaulted:
            Defaults().visit(top)
        (note,) = defaulted.value.__notes__
        assert note.endswith("Defaults.__default__, for the node assign at line 1, column 1")


def test_a_signature_that_fits_no_convention_fails_with_the_fields_of_the_node() -> None:
    class Wrong(fpeg.Interpreter):
        def assign(self, target: Any, expr: Any) -> None:
            pass

    with pytest.raises(TypeError, match="the fields of assign are target, value"):
        Wrong().visit(parse("x = 1;"))

    class Decorated(fpeg.Interpreter):
        @fpeg.v_args(inline=True)
        def assign(self, target: Any, value: Any) -> None:
            pass

    with pytest.raises(TypeError, match=r"Decorated\.assign has v_args"):
        Decorated().visit(parse("x = 1;"))

    class Terminal(fpeg.Interpreter):
        @fpeg.v_args(inline=True)
        def NUMBER(self, token: fpeg.Token) -> int:
            return int(token)

    with pytest.raises(TypeError, match=r"Terminal\.NUMBER has v_args"):
        Terminal().visit(parse("x = 1;"))


def test_strict_makes_every_public_method_name_a_rule_or_terminal() -> None:
    class Typo(fpeg.Interpreter, strict=True):
        def asign(self, node: fpeg.Tree) -> None:
            pass

        def helper_(self) -> None:
            pass

    with pytest.raises(
        TypeError, match=r"Typo\.asign names no rule or terminal.*did you mean assign"
    ):
        Typo().visit(parse("x = 1;"))


def test_a_rule_named_like_a_method_of_the_base_class_is_no_handler() -> None:
    visits = fpeg.Grammar("start: visit\nvisit: NAME\nNAME: [a-z]+")
    assert fpeg.Interpreter().visit(visits.parse("a")) == [["a"]]


def test_a_tree_built_in_python_is_walked_by_the_names_of_its_data() -> None:
    class Sum(fpeg.Interpreter):
        def add(self, *children: Any) -> int:
            return sum(self.visit(child) for child in children)

        def NUMBER(self, token: fpeg.Token) -> int:
            return int(token)

    tree = fpeg.Tree(
        "add", [fpeg.Token("NUMBER", "1"), fpeg.Tree("add", [fpeg.Token("NUMBER", "2")])]
    )
    assert Sum().visit(tree) == 3
    # Anything else in a tree built in Python comes back as it is.
    assert fpeg.Interpreter().visit(fpeg.Tree("other", [3, None])) == [3, None]


def test_a_tree_nested_deeper_than_the_recursion_limit_ends_the_walk() -> None:
    deep = fpeg.Grammar('start: item\n?item: NAME | "(" item ")" -> group\nNAME: [a-z]+')
    tree = deep.parse("(" * 5_000 + "a" + ")" * 5_000, max_depth=20_000)
    with pytest.raises(RecursionError):
        fpeg.Interpreter().visit(tree)
