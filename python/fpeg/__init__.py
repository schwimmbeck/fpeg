# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""A PEG parsing library with a native engine and a Transformer for parse trees.

A `Grammar` is compiled from text at runtime and parses inputs into trees of `Tree` and
`Token` items, views of a tree that stays in native memory. A `Transformer` turns them into
values with one method per rule or terminal, children first, an `Interpreter` walks them
from the top, each method visiting what it asks for, and a `Visitor` calls a method for
every node.
"""

from fpeg._fpeg import (
    Diagnostic,
    Discard,
    Grammar,
    GrammarError,
    LimitError,
    Meta,
    ParseError,
    Token,
    Tree,
    __version__,
    check,
)
from fpeg._transformer import Transformer, v_args
from fpeg._visitors import Interpreter, Visitor

__all__ = [
    "Diagnostic",
    "Discard",
    "Grammar",
    "GrammarError",
    "Interpreter",
    "LimitError",
    "Meta",
    "ParseError",
    "Token",
    "Transformer",
    "Tree",
    "Visitor",
    "__version__",
    "check",
    "v_args",
]
