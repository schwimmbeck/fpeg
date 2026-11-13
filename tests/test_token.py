# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

import pickle
import subprocess
import sys

import fpeg
import pytest

# Drops every reference that Python holds to the token type, then makes tokens of it.
DROP_THE_TYPE = """
import gc
import fpeg, fpeg._fpeg, fpeg._transformer
grammar = fpeg.Grammar('start: NAME+\\nNAME: [a-z]+\\n%ignore " "+')
del fpeg.Token, fpeg._fpeg.Token, fpeg._transformer.Token
gc.collect()
for _ in range(3):
    tokens = grammar.parse("abc de f").children
    assert [token.type for token in tokens] == ["NAME"] * 3, tokens
"""


def name_token() -> fpeg.Token:
    grammar = fpeg.Grammar('start: "(" NAME ")"\nNAME: [a-z]+')
    tree = grammar.parse("(abc)")
    assert isinstance(tree, fpeg.Tree)
    token = tree.children[0]
    assert isinstance(token, fpeg.Token)
    return token


def test_a_token_of_a_parse_is_a_str_with_its_kind_and_positions() -> None:
    token = name_token()
    assert isinstance(token, str)
    assert token == "abc"
    assert token.upper() == "ABC"
    assert token.type == "NAME"
    assert token.value == "abc"
    assert type(token.value) is str
    assert (token.start_pos, token.end_pos) == (1, 4)
    assert (token.line, token.column, token.end_line, token.end_column) == (1, 2, 1, 5)


def test_tokens_compare_and_hash_as_their_text() -> None:
    token = name_token()
    assert token == fpeg.Token("OTHER", "abc")
    assert hash(token) == hash("abc")
    by_text: dict[str, int] = {token: 1}
    assert by_text["abc"] == 1


def test_a_token_is_made_in_python_as_lark_makes_it() -> None:
    token = fpeg.Token("NAME", "x", 3, 1, 4, end_line=1, end_column=5, end_pos=4)
    assert (token.type, token, token.start_pos, token.line, token.column) == ("NAME", "x", 3, 1, 4)
    assert (token.end_line, token.end_column, token.end_pos) == (1, 5, 4)
    bare = fpeg.Token("NAME", "y")
    assert (bare.start_pos, bare.line, bare.end_column) == (None, None, None)
    with pytest.raises(ValueError, match="never negative"):
        fpeg.Token("NAME", "z", -1)


def test_update_and_new_borrow_pos_keep_the_positions() -> None:
    token = name_token()
    renamed = token.update(type="ID")
    assert (renamed.type, renamed, renamed.start_pos, renamed.column) == ("ID", "abc", 1, 2)
    retexted = token.update(value="xyz")
    assert (retexted.type, retexted, retexted.end_pos) == ("NAME", "xyz", 4)
    borrowed = fpeg.Token.new_borrow_pos("OTHER", "q", token)
    assert (borrowed.type, borrowed, borrowed.line, borrowed.end_column) == ("OTHER", "q", 1, 5)


def test_a_token_shows_and_pickles_as_lark_s_does() -> None:
    token = name_token()
    assert repr(token) == "Token('NAME', 'abc')"
    again = pickle.loads(pickle.dumps(token))
    assert (again.type, again, again.start_pos, again.end_column) == ("NAME", "abc", 1, 5)


def test_the_attributes_of_a_token_are_read_only() -> None:
    token = name_token()
    with pytest.raises(AttributeError):
        token.type = "OTHER"  # type: ignore[misc] # a read-only property, which this test sets
    with pytest.raises(AttributeError):
        token.line = 9  # type: ignore[misc] # a read-only property, which this test sets


def test_a_lone_surrogate_made_in_python_stays_in_value_and_repr() -> None:
    token = fpeg.Token("S", "\ud800")
    assert token.value == "\ud800"
    assert repr(token) == "Token('S', '\\ud800')"


def test_the_token_type_lives_when_python_drops_every_reference_to_it() -> None:
    result = subprocess.run(
        [sys.executable, "-I", "-c", DROP_THE_TYPE], capture_output=True, text=True, timeout=60
    )
    assert result.returncode == 0, result.stderr
