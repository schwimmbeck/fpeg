# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

import gc
import pickle
import sys
import weakref
from collections.abc import Iterator
from typing import TYPE_CHECKING, Any

import fpeg
import pytest

if TYPE_CHECKING:
    from decimal import Decimal

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


def test_a_rule_named_like_a_method_of_type_is_looked_for_on_the_class_only() -> None:
    grammar = fpeg.Grammar("start: mro\nmro: NAME\nNAME: [a-z]+")
    tree = grammar.parse("a")
    assert fpeg.Transformer().transform(tree) == tree
    assert fpeg.Interpreter().visit(tree) == [["a"]]
    assert fpeg.Visitor().visit(tree) is tree


def test_an_error_whose_notes_are_no_list_is_raised_as_it_is() -> None:
    class Odd(Exception):
        pass

    def odd(*args: Any) -> None:
        error = Odd("mine")
        error.__notes__ = ("not a list",)  # type: ignore[assignment] # what the test is about
        raise error

    name = parse("f()").children[0].children[0]
    transformer = type("T", (fpeg.Transformer,), {"NAME": odd})
    interpreter = type("I", (fpeg.Interpreter,), {"NAME": odd})
    for run in [transformer().transform, interpreter().visit]:
        with pytest.raises(Odd) as raised:
            run(name)
        assert raised.value.__notes__ == ("not a list",)


def test_strict_takes_overrides_of_the_methods_of_the_base_class() -> None:
    class Calls(fpeg.Transformer, strict=True):
        def transform(self, tree: Any) -> Any:
            return ("transformed", super().transform(tree))

    class Names(fpeg.Visitor, strict=True):
        def __init__(self) -> None:
            self.names: list[str] = []

        def visit(self, tree: Any) -> list[str]:
            super().visit(tree)
            return self.names

        def call(self, name: fpeg.Token, args: Any, kw: Any) -> None:
            self.names.append(str(name))

    class Run(fpeg.Interpreter, strict=True):
        def visit_children(self, node: fpeg.Tree) -> list[Any]:
            return ["children", *super().visit_children(node)]

    tree = parse("f(), g()")
    assert Calls().transform(tree)[0] == "transformed"
    assert Names().visit(tree) == ["f", "g"]
    assert Run().visit(tree)[0] == "children"


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
    # More arguments than a call passes from the stack.
    many = tuple("abcdefghij")
    assert Inline().transform(parse(f"f({', '.join(many)})")) == [("f", *many)]

    # On a class v_args applies to every method, and it wraps a callable that keeps no options.
    @fpeg.v_args(inline=True)
    class Decorated(fpeg.Transformer):
        kw = fpeg.v_args(inline=True)(str.upper)

        def call(self, name: Any, *rest: Any) -> tuple[Any, ...]:
            return (name, *rest)

    assert Decorated().transform(parse("f(a) ! k")).children == [("f", "a", "K")]
    with pytest.raises(TypeError, match="takes no wrapper"):
        fpeg.v_args(wrapper=print)  # type: ignore[arg-type] # a wrapper is refused


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


def test_a_callable_class_attribute_gets_the_list() -> None:
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

    class Shapes(fpeg.Transformer):
        def NAME(self) -> str:
            return "name"

        @staticmethod
        def NUMBER(*token: fpeg.Token) -> tuple[fpeg.Token, ...]:
            return token

    assert Shapes().transform(parse("f(7)")).children[0].children == ["name", ("7",)]

    class TwoTokens(fpeg.Transformer):
        def NAME(self, token: fpeg.Token, other: fpeg.Token) -> None: ...

    with pytest.raises(TypeError, match="takes 2 arguments, and a terminal's method takes"):
        TwoTokens().transform(parse("f()"))


def test_keyword_only_extras_give_meta_node_and_children_beside_fields() -> None:
    seen: list[Any] = []

    class Extras(fpeg.Transformer):
        def call(self, name: Any, *, meta: fpeg.Meta, node: fpeg.Tree, children: list[Any]) -> str:
            seen.append((meta.start_pos, node.data, node.text, children))
            return str(name)

    Extras().transform(parse("x, f(a)"[3:]))
    assert seen == [(0, "call", "f(a)", ["f", "a"])]


@pytest.mark.skipif(sys.version_info < (3, 14), reason="annotations are lazy from 3.14 on")
def test_an_annotation_that_only_a_type_checker_can_read_does_not_matter() -> None:
    class Typed(fpeg.Transformer):
        def call(self, name: Decimal) -> str:
            return f"<{name}>"

    assert Typed().transform(parse("f()")).children == ["<f>"]


def test_a_signature_that_fits_no_rule_fails_at_the_first_transform_naming_the_fields() -> None:
    class Wrong(fpeg.Transformer):
        def call(self, name: Any, values: Any) -> None: ...

    with pytest.raises(TypeError, match="the fields of call are name, args, kw"):
        Wrong().transform(parse("f()"))

    class Keyword(fpeg.Transformer):
        def call(self, *, position: Any) -> None: ...

    with pytest.raises(TypeError, match="takes only meta, node, children by keyword"):
        Keyword().transform(parse("f()"))

    class Before(fpeg.Transformer):
        def call(self, name: Any, *rest: Any) -> None: ...

    with pytest.raises(TypeError, match=r"takes parameters before its \*parameter"):
        Before().transform(parse("f()"))

    class Unnamed(fpeg.Transformer):
        def kw(self, bang: Any, name: Any) -> None: ...

    with pytest.raises(TypeError, match="kw has no fields"):
        Unnamed().transform(parse("f() ! k"))


def test_discard_removes_a_node_from_its_parent_and_from_its_field() -> None:
    class Drop(fpeg.Transformer):
        def kw(self) -> Any:
            return fpeg.Discard

        def call(self, name: Any, kw: Any) -> tuple[Any, Any]:
            return (name, kw)

    assert Drop().transform(parse("f() ! k")).children == [("f", None)]
    assert Drop().transform(parse("f() ! k").children[0].field("kw")) == []

    # Also when no method rebuilds the parent, on a parse and on a tree built in Python.
    class DropOnly(fpeg.Transformer):
        def kw(self) -> Any:
            return fpeg.Discard

    tree = parse("f() ! k")
    expected = fpeg.Tree("start", [fpeg.Tree("call", ["f"])])
    assert DropOnly().transform(tree) == expected
    assert DropOnly().transform(pickle.loads(pickle.dumps(tree))) == expected


def test_a_generator_puts_its_values_into_the_parent_in_the_node_s_place() -> None:
    class Splice(fpeg.Transformer):
        def group(self, *items: Any) -> Iterator[Any]:
            yield from items

        def call(self, name: Any, args: list[Any]) -> list[Any]:
            return [name, *args]

    assert Splice().transform(parse("f(a, [b, [c]], d)")).children == [["f", "a", "b", "c", "d"]]

    # A yielded Discard puts nothing, on a parse and on a tree built in Python.
    class Some(fpeg.Transformer):
        def group(self, *items: Any) -> Iterator[Any]:
            yield from (fpeg.Discard if item == "b" else item for item in items)

        def call(self, children: list[Any]) -> list[Any]:
            return children

    tree = parse("f(a, [b, c])")
    assert Some().transform(tree).children == [["f", "a", "c"]]
    assert Some().transform(pickle.loads(pickle.dumps(tree))).children == [["f", "a", "c"]]
    assert Splice().transform(parse("f([a, b])").children[0].field("args")[0]) == ["a", "b"]


def test_at_the_top_what_a_method_yields_or_discards_comes_back_as_a_list() -> None:
    class One(fpeg.Transformer):
        def start(self, *calls: Any) -> Iterator[Any]:
            yield "only"

    class Gone(fpeg.Transformer):
        def start(self, *calls: Any) -> Any:
            return fpeg.Discard

    class Name(fpeg.Transformer):
        def NAME(self, token: fpeg.Token) -> Iterator[Any]:
            yield str(token)

    tree = parse("f()")
    for item in (tree, pickle.loads(pickle.dumps(tree))):
        assert One().transform(item) == ["only"]
        assert Gone().transform(item) == []
    assert Name().transform(tree.children[0].children[0]) == ["f"]


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

    # Neither does the default, on a parse, a tree built in Python or a token alone.
    class Default(fpeg.Transformer):
        def __default_token__(self, token: fpeg.Token) -> str:
            return "default"

    copy = pickle.loads(pickle.dumps(tree))
    token = tree.children[0].children[0]
    for item in (tree, copy, token):
        assert Default(visit_tokens=False).transform(item) == item


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


def test_an_error_while_a_method_or_builder_yields_gets_its_note_too() -> None:
    class Late(fpeg.Transformer):
        def group(self, *items: Any) -> Iterator[Any]:
            yield from items
            raise ValueError("late")

    class LateToken(fpeg.Transformer):
        def NUMBER(self, token: fpeg.Token) -> Iterator[Any]:
            yield int(token)
            raise ValueError("late")

    class LateDefaults(fpeg.Transformer):
        def __default__(self, data: Any, children: list[Any], meta: Any) -> Iterator[Any]:
            yield from children
            raise ValueError("late")

        def __default_token__(self, token: fpeg.Token) -> Iterator[Any]:
            yield token
            raise ValueError("late")

    def late_values(text: str) -> Iterator[str]:
        yield text
        raise ValueError("late")

    built = fpeg.Grammar("?start: NAME\nNAME: [a-z]+ => Late", functions={"Late": late_values})
    tree = parse("f([a, b])")
    copy = pickle.loads(pickle.dumps(tree))
    for transformer, item, where in [
        (Late(), tree, "Late.group, for the node group at line 1, column 3"),
        (Late(), copy, "Late.group, for the tree group"),
        (LateToken(), parse("f(1)"), "LateToken.NUMBER, for the token NUMBER"),
        (LateDefaults(), tree, "LateDefaults.__default_token__, for the token NAME at line 1"),
        (LateDefaults(), copy, "LateDefaults.__default_token__, for the token NAME"),
        (LateDefaults(visit_tokens=False), copy, "LateDefaults.__default__, for the tree group"),
        (fpeg.Transformer(), built.parse("x"), "in the builder of NAME, for a token"),
    ]:
        with pytest.raises(ValueError, match="late") as raised:
            transformer.transform(item)
        assert where in raised.value.__notes__[-1]


def test_strict_transformers_name_only_rules_and_terminals() -> None:
    class Typo(fpeg.Transformer, strict=True):
        def calls(self, children: list[Any]) -> None: ...

    with pytest.raises(
        TypeError, match=r"Typo\.calls names no rule or terminal of the grammar, did you mean call"
    ):
        Typo().transform(parse("f()"))

    class Static(fpeg.Transformer, strict=True):
        @classmethod
        def kws(cls, children: list[Any]) -> None: ...

    with pytest.raises(TypeError, match=r"Static\.kws names no rule or terminal"):
        Static().transform(parse("f()"))

    class Helper(fpeg.Transformer, strict=True):
        def call(self, name: Any) -> str:
            return self._shout(name)

        def _shout(self, text: str) -> str:
            return text.upper()

    assert Helper().transform(parse("f()")).children == ["F"]


def test_a_rule_named_like_a_keyword_or_a_method_of_the_class_takes_an_underscore() -> None:
    grammar = fpeg.Grammar('start: if transform\nif: None\ntransform: "t"\nNone: "n"\n')

    class Names(fpeg.Transformer, strict=True):
        def if_(self, *children: str) -> str:
            return f"if({', '.join(children)})"

        def transform_(self) -> str:
            return "transform"

        def None_(self, token: fpeg.Token) -> str:
            return "none"

        def start(self, *children: str) -> list[str]:
            return list(children)

    assert Names().transform(grammar.parse("nt")) == ["if(none)", "transform"]
    # Also in a tree built in Python, which has no grammar.
    assert Names().transform(fpeg.Tree("if", [fpeg.Token("None", "n")])) == "if(none)"


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

    loop = fpeg.Tree("start", [])
    loop.children.append(loop)
    with pytest.raises(ValueError, match="the tree contains itself"):
        Built().transform(loop)


def test_a_transformer_class_goes_when_it_is_no_longer_used() -> None:
    class Once(fpeg.Transformer):
        def call(self, children: list[Any]) -> int:
            return len(children)

    # A tree built in Python, and a parse of a grammar that outlives the class.
    tree = fpeg.Tree("start", [fpeg.Tree("call", ["a"])])
    assert Once().transform(tree) == fpeg.Tree("start", [1])
    assert Once().transform(parse("f(a)")).children == [2]
    gone = weakref.ref(Once)
    del Once
    gc.collect()
    assert gone() is None


def test_a_tree_built_in_python_transforms_as_the_parse_it_copies() -> None:
    class Modes(fpeg.Transformer):
        def start(self, *calls: Any, meta: fpeg.Meta, children: list[Any]) -> tuple[Any, ...]:
            return (meta.end_column, children, *calls)

        def call(self, children: list[Any], *, node: fpeg.Tree) -> tuple[Any, ...]:
            return (node.data, children)

        def group(self, *items: Any) -> Iterator[Any]:
            yield from items

        def kw(self) -> Any:
            return fpeg.Discard

        NUMBER = int

    class Options(fpeg.Transformer):
        @fpeg.v_args(meta=True)
        def call(self, meta: fpeg.Meta, children: list[Any]) -> tuple[Any, ...]:
            return (meta.column, *children)

        @fpeg.v_args(inline=True, meta=True)
        def group(self, meta: fpeg.Meta, *items: Any) -> tuple[Any, ...]:
            return (meta.end_column, *items)

        @fpeg.v_args(tree=True)
        def kw(self, tree: fpeg.Tree) -> fpeg.Tree:
            return tree

    class Defaults(fpeg.Transformer):
        def __default__(self, data: Any, children: list[Any], meta: fpeg.Meta) -> Any:
            return (data, meta.start_pos, children)

    tree = parse("f(1, [a, 2]) ! k, g()")
    # A pickled tree comes back built in Python, without the parse behind it.
    copy = pickle.loads(pickle.dumps(tree))
    for transformer in (Modes(), Options(), Defaults()):
        assert transformer.transform(copy) == transformer.transform(tree)
    assert fpeg.Transformer().transform(copy) is copy

    class Failing(fpeg.Transformer):
        def call(self, children: list[Any]) -> None:
            raise ValueError("no")

    with pytest.raises(ValueError, match="no") as raised:
        Failing().transform(copy)
    assert raised.value.__notes__[-1].endswith("Failing.call, for the tree call")


def test_a_lone_token_and_any_other_value_transform_too() -> None:
    class Tokens(fpeg.Transformer):
        NUMBER = int

    number = parse("f(7)").children[0].children[1]
    assert Tokens().transform(number) == 7
    assert Tokens().transform("plain") == "plain"
    odd = fpeg.Token(None, "x")  # type: ignore[arg-type] # a type no str, which Python allows
    assert Tokens().transform(odd) is odd

    class Default(fpeg.Transformer):
        def __default_token__(self, token: fpeg.Token) -> str:
            return f"<{token}>"

    assert Default().transform(number) == "<7>"


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
    assert "    def call(self, name: Any, args: list[Any], kw: Any | None) -> Any:" in source
    assert "        name holds one item." in source
    assert "        args holds a list of items." in source
    assert "        kw holds one item or None." in source
    assert "    def NUMBER(self, token: fpeg.Token) -> Any:" in source
    # `?arg` always gives way to its one child, and only its alias makes nodes.
    assert "def arg(" not in source
    assert "    def group(self, *children: Any) -> Any:" in source
    namespace: dict[str, Any] = {}
    exec(compile(source, "<template>", "exec"), namespace)
    assert namespace["Calls"]().transform(parse("f(a, [1]) ! k")) is None
    keyword = fpeg.Grammar('start: if\nif: "x"').transformer_template()
    assert "    def if_(self, *children: Any) -> Any:" in keyword
    # Builders give the nodes of one name kinds apart, and a method meets them all.
    shared = fpeg.Grammar(
        "start: x | y\nx: a=NAME -> item => list\ny: b=NUM -> item => tuple\n"
        "NAME: [a-z]+\nNUM: [0-9]+"
    ).transformer_template()
    assert "    def item(self, a: Any | None, b: Any | None) -> Any:" in shared
    # A field self cannot be a parameter, and builder text keeps its escapes.
    odd = fpeg.Grammar(
        r'start: self=NAME => "q\"q\u{7}\n"' + "\nNAME: [a-z]+"
    ).transformer_template()
    namespace = {}
    exec(compile(odd, "<template>", "exec"), namespace)
    start = namespace["Transformer"].start
    assert '=> "q\\"q\\u{7}\\n" applies.' in start.__doc__
    assert "def start(self, *children: Any)" in odd
    # A field __x would be mangled in the class, and a method fpeg hides the module from the
    # annotations after it, which Python evaluates at once before 3.14.
    hidden = fpeg.Grammar('start: fpeg other\nfpeg: __x=A\nother: B\nA: "a"\nB: "b"')
    source = hidden.transformer_template()
    assert "    def fpeg(self, *children: Any) -> Any:" in source
    namespace = {}
    exec(compile(source, "<template>", "exec"), namespace)
    assert namespace["Transformer"]().transform(hidden.parse("ab")) is None


def test_deep_trees_transform_without_recursion() -> None:
    deep = fpeg.Grammar('start: "(" start? ")"')
    tree = deep.parse("(" * 1500 + ")" * 1500, max_depth=4000)

    class Depth(fpeg.Transformer):
        def start(self, children: list[Any]) -> int:
            return 1 + (children[0] if children else 0)

    assert Depth().transform(tree) == 1500
