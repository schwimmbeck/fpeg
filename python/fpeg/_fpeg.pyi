# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Type stubs for the native module."""

from os import PathLike
from typing import Any, final

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
    def parse(
        self,
        text: str,
        start: str = "start",
        *,
        max_steps: int | None = None,
        max_depth: int | None = None,
    ) -> Tree | Token: ...

@final
class Meta:
    """Where a node of a parse spans."""

    def __init__(
        self,
        line: int | None = None,
        column: int | None = None,
        end_line: int | None = None,
        end_column: int | None = None,
        start_pos: int | None = None,
        end_pos: int | None = None,
        empty: bool = True,
    ) -> None: ...
    @property
    def line(self) -> int | None: ...
    @property
    def column(self) -> int | None: ...
    @property
    def end_line(self) -> int | None: ...
    @property
    def end_column(self) -> int | None: ...
    @property
    def start_pos(self) -> int | None: ...
    @property
    def end_pos(self) -> int | None: ...
    @property
    def empty(self) -> bool: ...

class Tree:
    """A node of a parse tree: `data`, `children` and `meta`, as Lark's `Tree`."""

    def __init__(self, data: str, children: list[Any], meta: Meta | None = None) -> None: ...
    @property
    def data(self) -> str: ...
    @property
    def children(self) -> list[Any]: ...
    @property
    def meta(self) -> Meta: ...
    @property
    def text(self) -> str | None: ...

class Token(str):
    """A token of a parse: a str with the name of its kind and its positions."""

    def __new__(
        cls,
        type: str,
        value: object,
        start_pos: int | None = None,
        line: int | None = None,
        column: int | None = None,
        end_line: int | None = None,
        end_column: int | None = None,
        end_pos: int | None = None,
    ) -> Token: ...
    @property
    def type(self) -> str: ...
    @property
    def value(self) -> str: ...
    @property
    def start_pos(self) -> int | None: ...
    @property
    def end_pos(self) -> int | None: ...
    @property
    def line(self) -> int | None: ...
    @property
    def column(self) -> int | None: ...
    @property
    def end_line(self) -> int | None: ...
    @property
    def end_column(self) -> int | None: ...
    def update(self, type: str | None = None, value: object | None = None) -> Token: ...
    @classmethod
    def new_borrow_pos(cls, type: str, value: object, borrow_t: Token) -> Token: ...
