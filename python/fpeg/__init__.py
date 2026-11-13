# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""A PEG parsing library with a native engine and a Transformer for parse trees.

A `Grammar` is compiled from text at runtime and parses inputs into trees of `Tree` and
`Token` items, views of a tree that stays in native memory. A `Transformer` turns them into
values with one method per rule or terminal.
"""

from fpeg._fpeg import (
    Diagnostic,
    Grammar,
    GrammarError,
    LimitError,
    Meta,
    ParseError,
    Token,
    Tree,
    __version__,
)
from fpeg._transformer import Transformer, v_args

__all__ = [
    "Diagnostic",
    "Grammar",
    "GrammarError",
    "LimitError",
    "Meta",
    "ParseError",
    "Token",
    "Transformer",
    "Tree",
    "__version__",
    "v_args",
]
