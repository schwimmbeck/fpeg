# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

import gc
import random
from dataclasses import dataclass
from decimal import Decimal
from typing import Any

import fpeg
import pytest


def values(grammar: fpeg.Grammar, text: str) -> Any:
    return fpeg.Transformer().transform(grammar.parse(text))


def test_int_and_float_give_python_s_numbers() -> None:
    grammar = fpeg.Grammar(r"""
start: (number ** ",") => list
?number: FLOAT | UNDERSCORED | INT
INT: [+-]? [0-9]+ => int
FLOAT: [+-]? [0-9]+ "." [0-9]* ([eE] [+-]? [0-9]+)? => float
UNDERSCORED: [0-9]+ ("_" [0-9]+)+ "u" => int
%ignore " "+
""")
    numbers = values(grammar, "7, -12, +3, 123456789012345678901234567890, 2.5, -0.0, 1.e400")
    assert numbers == [7, -12, 3, 123456789012345678901234567890, 2.5, -0.0, float("inf")]
    assert [type(n) for n in numbers[:4]] == [int] * 4
    with pytest.raises(ValueError, match="invalid literal for int"):
        values(grammar, "1_000u")


# Numbers that the next test reads.
FLOAT_TEXTS = 20_000
# Digits of a number at most, more than a double holds, so that many round.
MAX_FLOAT_DIGITS = 25
# The least exponent of a number, low enough that some round to zero.
MIN_FLOAT_EXPONENT = -330
# The greatest exponent of a number, high enough that some overflow to infinity.
MAX_FLOAT_EXPONENT = 310


def test_the_fast_float_path_agrees_with_python_on_many_numbers() -> None:
    rng = random.Random(20261113)
    texts = []
    for _ in range(FLOAT_TEXTS):
        digits = "".join(rng.choice("0123456789") for _ in range(rng.randint(1, MAX_FLOAT_DIGITS)))
        point = rng.randint(0, len(digits))
        text = f"{digits[:point]}.{digits[point:]}" if point < len(digits) else digits + ".0"
        if rng.random() < 0.5:
            text += f"e{rng.randint(MIN_FLOAT_EXPONENT, MAX_FLOAT_EXPONENT)}"
        texts.append(text)
    grammar = fpeg.Grammar(r"""
start: (NUMBER ** ",") => list
NUMBER: [0-9]* "." [0-9]* ([eE] [+-]? [0-9]+)? => float
""")
    assert values(grammar, ",".join(texts)) == [float(t) for t in texts]


def test_text_gives_a_plain_str_and_unquote_decodes_json_escapes() -> None:
    grammar = fpeg.Grammar(r"""
start: (item ** ",") => list
?item: NAME | STRING
NAME: [a-z]+ => text
STRING: "\"" ([^"\\] | "\\" .)* "\"" => unquote
%ignore " "+
""")
    name, plain, escaped, pair, lone = values(
        grammar, r'abc, "x", "\" \\ \/ \' \b\f\n\r\t", "😀", "\udc00 \ud800x"'
    )
    assert (name, type(name)) == ("abc", str)
    assert plain == "x"
    assert escaped == "\" \\ / ' \b\f\n\r\t"
    assert pair == "\U0001f600"
    assert lone == "\udc00 \ud800x"
    with pytest.raises(ValueError, match="does not know the escape") as raised:
        values(grammar, r'"\q"')
    assert raised.value.__notes__ == [
        "fpeg: in the builder unquote of STRING, for the token at line 1, column 1"
    ]
    # Four hex digits, without the sign that Rust's own parser would take, as in json.
    with pytest.raises(ValueError, match="four hex digits"):
        values(grammar, r'"\u+123"')


def test_list_tuple_dict_and_constants_make_json_s_values() -> None:
    grammar = fpeg.Grammar(r"""
?start: value
?value: object | array | STRING | INT
      | "true" => True | "false" => False | "null" => None
      | "pi" => 3.25 | "answer" => 42 | "name" => "fpeg"
object: "{" (member ** ",") "}" => dict
member: key=STRING ":" value=value => tuple
array: "[" (value ** ",") "]" => list
STRING: "\"" [a-z]* "\"" => unquote
INT: [0-9]+ => int
%ignore " "+
""")
    text = '{"a": [1, true, false, null], "b": {}, "c": [pi, answer, name]}'
    assert values(grammar, text) == {
        "a": [1, True, False, None],
        "b": {},
        "c": [3.25, 42, "fpeg"],
    }


@dataclass
class Point:
    x: Any
    y: Any
    label: str
    rest: list[Any]


def test_a_function_gets_fields_constants_keywords_and_all_children() -> None:
    grammar = fpeg.Grammar(
        r"""
start: x=INT "," y=INT? tag=TAG* => Point(x, y, label="p", rest=*)
INT: [0-9]+ => int
TAG: "#" [a-z]+
%ignore " "+
""",
        functions={"Point": Point},
    )
    point = values(grammar, "1, 2 #a #b")
    assert point == Point(1, 2, "p", [1, 2, "#a", "#b"])
    assert values(grammar, "1,").y is None


def test_a_bare_function_gets_a_token_s_text_or_a_node_s_children() -> None:
    grammar = fpeg.Grammar(
        r"""
start: (NUMBER ** ",") => frozenset
NUMBER: [0-9]+ "." [0-9]+ => Decimal
%ignore " "+
""",
        functions={"Decimal": Decimal, "frozenset": frozenset},
    )
    assert values(grammar, "1.10, 2.5") == frozenset({Decimal("1.10"), Decimal("2.5")})


def test_functions_the_builders_call_must_be_given_and_callable() -> None:
    text = 'start: "a" => Make\nother: "b" => Make(1)'
    with pytest.raises(fpeg.GrammarError) as raised:
        fpeg.Grammar(text)
    assert [(d.code, d.line, d.column) for d in raised.value.diagnostics] == [
        ("E208", 1, 15),
        ("E208", 2, 15),
    ]
    not_callable: dict[str, Any] = {"Make": 3}
    with pytest.raises(TypeError, match="Make given with the grammar is not callable"):
        fpeg.Grammar(text, functions=not_callable)
    assert isinstance(fpeg.Grammar(text, functions={"Make": dict}), fpeg.Grammar)


def test_a_method_comes_before_a_builder_and_a_builder_before_default() -> None:
    grammar = fpeg.Grammar(r"""
start: pair+ => list
pair: NAME "=" NUMBER => tuple
NAME: [a-z]+ => text
NUMBER: [0-9]+ => int
%ignore " "+
""")

    class Override(fpeg.Transformer):
        def NUMBER(self, token: fpeg.Token) -> str:
            return f"#{token}"

        def __default__(self, data: Any, children: list[Any], meta: Any) -> str:
            return "default"

    assert Override().transform(grammar.parse("a = 1 b = 2")) == [("a", "#1"), ("b", "#2")]


def test_a_lone_token_gets_the_builder_of_its_grammar_while_the_grammar_lives() -> None:
    grammar = fpeg.Grammar('?start: NUMBER | "[" start "]"\nNUMBER: [0-9]+ => int')
    token = grammar.parse("7")
    assert isinstance(token, fpeg.Token)
    assert fpeg.Transformer().transform(token) == 7
    del grammar
    gc.collect()
    assert fpeg.Transformer().transform(token) is token
