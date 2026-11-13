# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Type stubs for the native module."""

from collections.abc import Callable, Iterator, Mapping
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

class ParseError(ValueError):
    """The input does not match the grammar."""

    message: str
    pos: int
    line: int
    column: int
    expected: tuple[str, ...]
    label: str | None

class LimitError(ParseError):
    """A parse stopped at a limit on its depth, its steps or the size of its input."""

    limit: str

@final
class Grammar:
    """A grammar compiled for fpeg's parsing machine."""

    def __init__(
        self, text: str, *, functions: Mapping[str, Callable[..., Any]] | None = None
    ) -> None: ...
    @classmethod
    def open(
        cls,
        path: str | PathLike[str],
        *,
        functions: Mapping[str, Callable[..., Any]] | None = None,
    ) -> Grammar: ...
    def parse(
        self,
        text: str,
        start: str = "start",
        *,
        max_steps: int | None = None,
        max_depth: int | None = None,
    ) -> Tree | Token: ...
    def transformer_template(self, name: str = "Transformer") -> str: ...

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
    def field(self, name: str) -> Any: ...
    def pretty(self, indent_str: str = "  ") -> str: ...
    def iter_subtrees(self) -> Iterator[Tree]: ...
    def iter_subtrees_topdown(self) -> Iterator[Tree]: ...
    def find_pred(self, pred: Callable[[Tree], bool]) -> Iterator[Tree]: ...
    def find_data(self, data: str) -> Iterator[Tree]: ...
    def scan_values(self, pred: Callable[[Any], bool]) -> Iterator[Any]: ...
    def copy(self) -> Tree: ...
    def __copy__(self) -> Tree: ...
    def __eq__(self, other: object) -> bool: ...
    def __ne__(self, other: object) -> bool: ...
    def __hash__(self) -> int: ...

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

@final
class DiscardType:
    """The type of `Discard`."""

Discard: DiscardType
NO_VALUE: object

def rebuild_tree(shape: list[int], values: list[Any]) -> Tree: ...
def is_view(tree: Tree) -> bool: ...
def scan(
    transformer: object,
    view: Tree,
    make_plan: Callable[
        [type, list[tuple[str, bool, tuple[tuple[str, bool], ...]]]],
        tuple[list[tuple[int, str, int, tuple[str, ...], int]], bool, bool],
    ],
) -> tuple[Any, ...]: ...
def token_value(token: Token) -> Any: ...
