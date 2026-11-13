# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

from collections.abc import Iterator
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


def test_one_parameter_that_names_no_field_gets_the_list_of_children() -> None:
    class Lists(fpeg.Transformer):
        def call(self, children: list[Any]) -> list[Any]:
            return children

    assert Lists().transform(parse("f(a, 1)")).children == [["f", "a", "1"]]


def test_a_node_takes_exactly_its_children_also_right_after_a_visible_sibling() -> None:
    adjacent = fpeg.Grammar(r"""
start: NAME pair pair
pair: NAME NUMBER
NAME: [a-z]+
NUMBER: [0-9]+
%ignore " "+
""")

    class Lists(fpeg.Transformer):
        def start(self, children: list[Any]) -> list[Any]:
            return children

        def pair(self, children: list[Any]) -> list[Any]:
            return children

    assert Lists().transform(adjacent.parse("a b 1 c 2")) == ["a", ["b", "1"], ["c", "2"]]


def test_a_star_parameter_or_inline_v_args_gets_the_children_as_arguments() -> None:
    class Inline(fpeg.Transformer):
        def call(self, *children: Any) -> tuple[Any, ...]:
            return children

        @fpeg.v_args(inline=True)
        def start(self, *calls: Any) -> list[Any]:
            return list(calls)

    assert Inline().transform(parse("f(a), g()")) == [("f", "a"), ("g",)]


def test_parameters_named_after_fields_get_one_value_none_or_a_list() -> None:
    class Fields(fpeg.Transformer):
        def call(self, name: Any, args: list[Any], kw: Any) -> tuple[Any, ...]:
            return (name, args, kw)

        def kw(self, children: list[Any]) -> str:
            return f"!{children[0]}"

        def start(self, *calls: Any) -> list[Any]:
            return list(calls)

    assert Fields().transform(parse("f(a, 1) ! k, g()")) == [
        ("f", ["a", "1"], "!k"),
        ("g", [], None),
    ]


def test_a_method_without_parameters_is_called_with_nothing() -> None:
    class Constant(fpeg.Transformer):
        def kw(self) -> str:
            return "keyword"

    call = Constant().transform(parse("f() ! k")).children[0]
    assert call.children == ["f", "keyword"]


def test_v_args_passes_meta_or_a_tree_of_the_transformed_children() -> None:
    class Options(fpeg.Transformer):
        @fpeg.v_args(meta=True)
        def call(self, meta: fpeg.Meta, children: list[Any]) -> tuple[int | None, int]:
            return (meta.column, len(children))

        @fpeg.v_args(tree=True)
        def start(self, tree: fpeg.Tree) -> fpeg.Tree:
            return tree

    tree = Options().transform(parse("f(a),  g()"))
    assert tree == fpeg.Tree("start", [(1, 2), (8, 1)])


def test_a_callable_class_attribute_gets_the_list_as_in_lark() -> None:
    class Attributes(fpeg.Transformer):
        start = list
        call = tuple

    assert Attributes().transform(parse("f(a), g()")) == [("f", "a"), ("g",)]


def test_terminal_methods_get_their_token_and_any_callable_works() -> None:
    class Tokens(fpeg.Transformer):
        NUMBER = int

        def NAME(self, token: fpeg.Token) -> str:
            return f"{token.type}:{token}"

    call = Tokens().transform(parse("f(7)")).children[0]
    assert call.children == ["NAME:f", 7]


def test_keyword_only_extras_give_meta_node_and_children_beside_fields() -> None:
    seen: list[Any] = []

    class Extras(fpeg.Transformer):
        def call(self, name: Any, *, meta: fpeg.Meta, node: fpeg.Tree, children: list[Any]) -> str:
            seen.append((meta.start_pos, node.data, node.text, children))
            return str(name)

    Extras().transform(parse("x, f(a)"[3:]))
    assert seen == [(0, "call", "f(a)", ["f", "a"])]


def test_a_signature_that_fits_no_rule_fails_at_the_first_transform_naming_the_fields() -> None:
    class Wrong(fpeg.Transformer):
        def call(self, name: Any, values: Any) -> None: ...

    with pytest.raises(TypeError, match="the fields of call are name, args, kw"):
        Wrong().transform(parse("f()"))

    class Keyword(fpeg.Transformer):
        def call(self, *, position: Any) -> None: ...

    with pytest.raises(TypeError, match="takes only meta, node, children by keyword"):
        Keyword().transform(parse("f()"))


def test_discard_removes_a_node_from_its_parent_and_from_its_field() -> None:
    class Drop(fpeg.Transformer):
        def kw(self) -> Any:
            return fpeg.Discard

        def call(self, name: Any, kw: Any) -> tuple[Any, Any]:
            return (name, kw)

    assert Drop().transform(parse("f() ! k")).children == [("f", None)]
    assert Drop().transform(parse("f() ! k").children[0].field("kw")) == []


def test_a_generator_puts_its_values_into_the_parent_in_the_node_s_place() -> None:
    class Splice(fpeg.Transformer):
        def group(self, *items: Any) -> Iterator[Any]:
            yield from items

        def call(self, name: Any, args: list[Any]) -> list[Any]:
            return [name, *args]

    assert Splice().transform(parse("f(a, [b, [c]], d)")).children == [["f", "a", "b", "c", "d"]]
    assert Splice().transform(parse("f([a, b])").children[0].field("args")[0]) == ["a", "b"]


def test_several_values_in_a_field_of_one_item_are_an_error() -> None:
    class Twice(fpeg.Transformer):
        def kw(self, children: list[Any]) -> Iterator[Any]:
            yield "one"
            yield "two"

        def call(self, kw: Any) -> Any:
            return kw

    with pytest.raises(TypeError, match="the field kw holds one item, and its children gave 2"):
        Twice().transform(parse("f() ! k"))


def test_a_subtree_no_method_reaches_stays_a_view_and_a_changed_one_a_new_tree() -> None:
    tree = parse("f(a), g(1)")
    assert fpeg.Transformer().transform(tree) is tree

    class Numbers(fpeg.Transformer):
        NUMBER = int

    result = Numbers().transform(tree)
    assert result.children[0] is not tree.children[0]
    assert result == fpeg.Tree("start", [tree.children[0], fpeg.Tree("call", ["g", 1])])
    assert result.children[1].meta.start_pos == 6


def test_default_methods_apply_when_a_subclass_overrides_them() -> None:
    class Defaults(fpeg.Transformer):
        def __default__(self, data: Any, children: list[Any], meta: fpeg.Meta) -> Any:
            return (data, children)

        def __default_token__(self, token: fpeg.Token) -> str:
            return token.upper()

    assert Defaults().transform(parse("f(a)")) == ("start", [("call", ["F", "A"])])


def test_terminal_methods_wait_without_visit_tokens() -> None:
    class Upper(fpeg.Transformer):
        def NAME(self, token: fpeg.Token) -> str:
            return token.upper()

    tree = parse("f()")
    assert Upper(visit_tokens=False).transform(tree) is tree
    assert Upper().transform(tree).children[0].children == ["F"]


def test_an_error_in_a_method_keeps_its_type_and_gets_a_note_with_the_place() -> None:
    class Failing(fpeg.Transformer):
        def call(self, name: Any) -> None:
            raise ValueError(f"no function {name}")

    with pytest.raises(ValueError, match="no function f") as raised:
        Failing().transform(parse("   f()"))
    assert raised.value.__notes__ == [
        "fpeg: in test_an_error_in_a_method_keeps_its_type_and_gets_a_note_with_the_place."
        "<locals>.Failing.call, for the node call at line 1, column 4"
    ]


def test_strict_transformers_name_only_rules_and_terminals() -> None:
    class Typo(fpeg.Transformer, strict=True):
        def calls(self, children: list[Any]) -> None: ...

    with pytest.raises(
        TypeError, match=r"Typo\.calls names no rule or terminal of the grammar, did you mean call"
    ):
        Typo().transform(parse("f()"))

    class Helper(fpeg.Transformer, strict=True):
        def call(self, name: Any) -> str:
            return self._shout(name)

        def _shout(self, text: str) -> str:
            return text.upper()

    assert Helper().transform(parse("f()")).children == ["F"]


def test_a_tree_built_in_python_is_transformed_by_its_data() -> None:
    class Built(fpeg.Transformer):
        def pair(self, children: list[Any]) -> tuple[Any, ...]:
            return tuple(children)

        def NAME(self, token: fpeg.Token) -> str:
            return token * 2

    built = fpeg.Tree("start", [fpeg.Tree("pair", [fpeg.Token("NAME", "a"), "b"]), parse("f()")])
    result = Built().transform(built)
    assert result == fpeg.Tree(
        "start", [("aa", "b"), fpeg.Tree("start", [fpeg.Tree("call", ["ff"])])]
    )

    class FieldsOnBuilt(fpeg.Transformer):
        def pair(self, key: Any, value: Any) -> None: ...

    with pytest.raises(TypeError, match="a tree built in Python has no fields"):
        FieldsOnBuilt().transform(built)


def test_a_lone_token_and_any_other_value_transform_too() -> None:
    class Tokens(fpeg.Transformer):
        NUMBER = int

    number = parse("f(7)").children[0].children[1]
    assert Tokens().transform(number) == 7
    assert Tokens().transform("plain") == "plain"


def test_methods_are_found_on_the_instance_at_each_transform() -> None:
    class Instance(fpeg.Transformer):
        def call(self, name: Any) -> str:
            return f"class {name}"

    transformer = Instance()
    tree = parse("f()")
    assert transformer.transform(tree).children == ["class f"]
    transformer.call = lambda name: f"instance {name}"  # type: ignore[method-assign] # the test replaces it
    assert transformer.transform(tree).children == ["instance f"]


def test_a_method_may_transform_a_subtree_itself() -> None:
    class Nested(fpeg.Transformer):
        def start(self, *calls: Any) -> list[Any]:
            return list(calls)

        def call(self, name: Any, *, node: fpeg.Tree) -> Any:
            return (name, Inner().transform(node))

    class Inner(fpeg.Transformer):
        def call(self, children: list[Any]) -> int:
            return len(children)

    assert Nested().transform(parse("f(a, b)")) == [("f", 3)]


def test_the_template_of_a_grammar_compiles_and_fits_its_plan() -> None:
    source = CALLS.transformer_template("Calls")
    assert "    def call(self, name: Any, args: list[Any], kw: Any) -> Any:" in source
    assert "        args holds a list of items." in source
    assert "    def NUMBER(self, token: fpeg.Token) -> Any:" in source
    namespace: dict[str, Any] = {}
    exec(compile(source, "<template>", "exec"), namespace)
    assert namespace["Calls"]().transform(parse("f(a, [1]) ! k")) is None
    keyword = fpeg.Grammar('start: if\nif: "x"').transformer_template()
    assert "    # if cannot be a method of the class: Python reserves the name." in keyword


def test_deep_trees_transform_without_recursion() -> None:
    deep = fpeg.Grammar('start: "(" start? ")"')
    tree = deep.parse("(" * 1500 + ")" * 1500, max_depth=4000)

    class Depth(fpeg.Transformer):
        def start(self, children: list[Any]) -> int:
            return 1 + (children[0] if children else 0)

    assert Depth().transform(tree) == 1500
