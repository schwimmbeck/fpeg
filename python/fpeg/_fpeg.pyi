# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Type stubs for the native module."""

from os import PathLike
from typing import final

__version__: str

@final
class Diagnostic:
    """A problem of a grammar: its stable code, what exactly is wrong, and where."""

    @property
    def code(self) -> str: ...
    @property
    def message(self) -> str: ...
    @property
    def line(self) -> int: ...
    @property
    def column(self) -> int: ...
    @property
    def end_line(self) -> int: ...
    @property
    def end_column(self) -> int: ...
    @property
    def path(self) -> str | None: ...

class GrammarError(ValueError):
    """A grammar has problems that keep it from compiling."""

    diagnostics: tuple[Diagnostic, ...]

@final
class Grammar:
    """A grammar compiled for fpeg's parsing machine."""

    def __init__(self, text: str) -> None: ...
    @classmethod
    def open(cls, path: str | PathLike[str]) -> Grammar: ...
