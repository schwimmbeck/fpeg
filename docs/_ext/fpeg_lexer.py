# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""A Pygments lexer for fastpeg grammars, so that grammar blocks in the docs are highlighted."""

from typing import Any, ClassVar

from pygments.lexer import RegexLexer
from pygments.token import (
    Comment,
    Keyword,
    Name,
    Number,
    Operator,
    Punctuation,
    String,
    Whitespace,
)
from sphinx.application import Sphinx


class FpegLexer(RegexLexer):
    """Highlight rules, terminals, literals, classes, directives and operators."""

    name = "fastpeg"
    aliases = ("fpeg",)
    filenames = ("*.fpeg",)

    tokens: ClassVar[dict[str, Any]] = {
        "root": [
            (r"//[^\n]*", Comment.Single),
            (r"\s+", Whitespace),
            (r"%[a-z]+", Keyword),
            (r"@[a-z]+", Name.Decorator),
            (r'"(\\.|[^"\\\n])*"i?', String),
            (r"'(\\.|[^'\\\n])*'i?", String),
            (r"\[(\\.|[^\]\\\n])+\]", String.Regex),
            (r"\\[pP]\{\w+\}", String.Regex),
            (r"->|=>|\*\*|\+\+|\.\.\.", Operator),
            (r"[|&!~^?*+.=]", Operator),
            (r"_*[A-Z][A-Za-z0-9_]*", Name.Constant),
            (r"_*[a-z][A-Za-z0-9_]*", Name),
            (r"-?\d+", Number),
            (r"[(){}<>:,]", Punctuation),
        ]
    }


def setup(app: Sphinx) -> dict[str, Any]:
    """Register the lexer under the name used by grammar blocks."""
    app.add_lexer("fpeg", FpegLexer)
    return {"parallel_read_safe": True, "parallel_write_safe": True}
