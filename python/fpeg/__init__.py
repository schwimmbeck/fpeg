# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""A PEG parsing library with a native engine and a Transformer for parse trees.

A `Grammar` is compiled from text at runtime.
"""

from fpeg._fpeg import (
    Diagnostic,
    Grammar,
    GrammarError,
    __version__,
)

__all__ = [
    "Diagnostic",
    "Grammar",
    "GrammarError",
    "__version__",
]
