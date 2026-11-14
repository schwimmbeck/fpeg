# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The Interpreter: methods named like the rules of a grammar, each visiting what it asks for.

A method gets the parts of its node as parsed, not yet visited, and what it gets its signature
says, read as the Transformer reads it, once per class and grammar. The walk is in Python and
recurses as deeply as the tree nests.
"""

from __future__ import annotations

import weakref
from typing import Any, ClassVar, Final

from fpeg import _fpeg
from fpeg._fpeg import Token, Tree
from fpeg._transformer import (
    EXTRAS,
    FIELDS,
    INLINE,
    NOTHING,
    function_of,
    is_handler,
    node_convention,
    reserved_names,
    token_convention,
)

# How a method of a node is called: its mode, the fields it takes and its extras.
_Convention = tuple[int, tuple[str, ...], int]

# What a call gives for a node without a method.
_NO_METHOD: Final = object()

_NO_VALUE = _fpeg.NO_VALUE

# By class, weakly, so that a class made for one task goes when it is no longer used: how the
# methods of trees built in Python are called, which have no grammar, and those of terminals.
_owned: weakref.WeakKeyDictionary[type, dict[str, _Convention | None]] = weakref.WeakKeyDictionary()
_tokens: weakref.WeakKeyDictionary[type, dict[str, int | None]] = weakref.WeakKeyDictionary()


def _convention(cls: type, name: str, fields: dict[str, bool] | None) -> _Convention:
    _, options = function_of(cls, name)
    if options:
        raise TypeError(
            f"{cls.__qualname__}.{name} has v_args, which says how a Transformer calls a method"
        )
    return node_convention(cls, name, fields)


def _make_plan(
    cls: type, kinds: list[tuple[str, bool, tuple[tuple[str, bool], ...]]]
) -> dict[str, _Convention]:
    """Say for each kind of node with a method how the method is called.

    `fpeg._fpeg.walk_plan` asks for it once per class and grammar.
    """
    plan: dict[str, _Convention] = {}
    for name, token, fields in kinds:
        if not token and name not in plan and is_handler(cls, name):
            plan[name] = _convention(cls, name, dict(fields))
    return plan


def _owned_convention(cls: type, name: Any) -> _Convention | None:
    """Say how the method for a tree built in Python is called, if the class has one."""
    if not isinstance(name, str):
        return None
    by_name = _owned.setdefault(cls, {})
    if name not in by_name:
        by_name[name] = _convention(cls, name, None) if is_handler(cls, name) else None
    return by_name[name]


def _token_mode(cls: type, name: Any) -> int | None:
    """Say how the method of a terminal is called, if the class has one."""
    if not isinstance(name, str):
        return None
    by_name = _tokens.setdefault(cls, {})
    if name not in by_name:
        by_name[name] = token_convention(cls, name) if is_handler(cls, name) else None
    return by_name[name]


def _note(error: BaseException, where: str, line: int | None, column: int | None) -> None:
    """Say on an error where it came from, once: the innermost method says it."""
    notes = getattr(error, "__notes__", ())
    if any(isinstance(note, str) and note.startswith("fpeg: ") for note in notes):
        return
    place = "" if line is None else f" at line {line}, column {column}"
    error.add_note(f"fpeg: {where}{place}")


def _call(walker: object, node: Tree) -> Any:
    """Call the method of a node as its signature asks, or return `_NO_METHOD`."""
    cls = type(walker)
    name = node.data
    plan = _fpeg.walk_plan(cls, node, _make_plan)
    # A tree built in Python has no grammar to plan for.
    convention = _owned_convention(cls, name) if plan is None else plan.get(name)
    if convention is None:
        return _NO_METHOD
    mode, fields, extras = convention
    try:
        if mode == FIELDS:
            args: tuple[Any, ...] = tuple(node.field(field) for field in fields)
        elif mode == INLINE:
            args = tuple(node.children)
        elif mode == NOTHING:
            args = ()
        else:
            args = (node,)
        if not extras:
            return getattr(walker, name)(*args)
        kwargs: dict[str, Any] = {}
        if extras & EXTRAS["meta"]:
            kwargs["meta"] = node.meta
        if extras & EXTRAS["node"]:
            kwargs["node"] = node
        if extras & EXTRAS["children"]:
            kwargs["children"] = node.children
        return getattr(walker, name)(*args, **kwargs)
    except BaseException as error:
        meta = node.meta
        _note(error, f"in {cls.__qualname__}.{name}, for the node {name}", meta.line, meta.column)
        raise


class Interpreter:
    """Walk a tree from the top, with methods named like the rules and terminals of a grammar.

    `visit(node)` calls the method named like the node, which decides what to visit of it and
    when: it gets the parts of the node as parsed, and visits one with `self.visit(part)`. The
    signature of a method says what it gets: the fields of the node by name, `*children` for the
    children, one other parameter for the node, and keyword-only `meta`, `node` or `children`
    beside. A node without a method gives the list of its visited children.

    A token gives what the method named like its terminal returns, else the value of the
    terminal's builder (`=>` in the grammar), else itself. The builders of rules are not applied.

    The walk recurses in Python, so a tree nested deeper than Python's recursion limit allows
    ends it with `RecursionError`. A `Transformer` does not recurse.
    """

    __fpeg_reserved__: ClassVar[frozenset[str]]

    def visit(self, item: Any) -> Any:
        """Return what the method of a node or token returns, or anything else as it is.

        `None` and a list are refused: an optional field that is absent is `None` and a field of
        several items is a list, and visiting either as if it were an item hides a mistake.
        """
        if isinstance(item, Tree):
            result = _call(self, item)
            return self.__default__(item) if result is _NO_METHOD else result
        if isinstance(item, Token):
            return self._visit_token(item)
        if item is None or isinstance(item, list):
            what = "None, as an absent optional field is" if item is None else "a list"
            raise TypeError(f"visit takes a node or a token, and got {what}")
        return item

    def visit_children(self, node: Tree) -> list[Any]:
        """Return the visible children of a node, each visited."""
        visit = self.visit
        return [
            visit(child) if isinstance(child, (Tree, Token)) else child for child in node.children
        ]

    def __default__(self, node: Tree) -> Any:
        """Give the value of a node without a method: the list of its visited children."""
        return self.visit_children(node)

    def _visit_token(self, token: Token) -> Any:
        cls = type(self)
        name: Any = token.type
        mode = _token_mode(cls, name)
        try:
            if mode is None:
                built = _fpeg.token_value(token)
                return token if built is _NO_VALUE else built
            method = getattr(self, name)
            return method() if mode == NOTHING else method(token)
        except BaseException as error:
            where = f"the builder of {name}" if mode is None else f"{cls.__qualname__}.{name}"
            _note(error, f"in {where}, for the token {name}", token.line, token.column)
            raise


Interpreter.__fpeg_reserved__ = reserved_names(Interpreter)
