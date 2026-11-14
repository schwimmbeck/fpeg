# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The Interpreter and the Visitor: methods named like rules, called by a walk in Python.

A method gets the parts of its node as parsed, not yet visited, and what it gets its signature
says, read as the Transformer reads it, once per class and grammar. The Interpreter visits what
its methods ask for and recurses as deeply as the tree nests. The Visitor calls a method for
every node and does not recurse.
"""

from __future__ import annotations

from typing import Any, ClassVar, Final

from fpeg import _fpeg
from fpeg._fpeg import Token, Tree
from fpeg._transformer import (
    EXTRAS,
    FIELDS,
    INLINE,
    NOTHING,
    check_strict,
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

# The attributes under which a class keeps how its methods are called: those of trees built in
# Python, which have no grammar, and those of terminals.
_OWNED: Final = "_fpeg_owned"
_TOKENS: Final = "_fpeg_tokens"


def _kept(cls: type, attribute: str) -> dict[str, Any]:
    """Return the dict a class keeps under an attribute of its own, which goes with the class.

    A weak dictionary by class costs a weak reference at every look, which is a fifth of a walk
    over tokens.
    """
    kept: dict[str, Any] | None = cls.__dict__.get(attribute)
    if kept is None:
        kept = {}
        setattr(cls, attribute, kept)
    return kept


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
    if getattr(cls, "__fpeg_strict__", False):
        check_strict(cls, {name for name, _, _ in kinds})
    plan: dict[str, _Convention] = {}
    for name, token, fields in kinds:
        if not is_handler(cls, name):
            continue
        if not token and name not in plan:
            plan[name] = _convention(cls, name, dict(fields))
        elif token and issubclass(cls, Visitor):
            raise TypeError(
                f"{cls.__qualname__}.{name} is named like a terminal, and a Visitor calls "
                "methods for nodes only"
            )
    return plan


def _owned_convention(cls: type, name: Any) -> _Convention | None:
    """Say how the method for a tree built in Python is called, if the class has one."""
    if not isinstance(name, str):
        return None
    by_name = _kept(cls, _OWNED)
    if name not in by_name:
        by_name[name] = _convention(cls, name, None) if is_handler(cls, name) else None
    return by_name[name]


def _token_mode(cls: type, name: Any) -> int | None:
    """Say how the method of a terminal is called, if the class has one."""
    if not isinstance(name, str):
        return None
    by_name = _kept(cls, _TOKENS)
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


class _Walker:
    """What the Interpreter and the Visitor share: `strict=True` as a class keyword."""

    __fpeg_strict__: ClassVar[bool] = False
    __fpeg_reserved__: ClassVar[frozenset[str]]

    def __init_subclass__(cls, *, strict: bool = False, **kwargs: Any) -> None:
        super().__init_subclass__(**kwargs)
        cls.__fpeg_strict__ = strict


class Interpreter(_Walker):
    """Walk a tree from the top, with methods named like the rules and terminals of a grammar.

    `visit(node)` calls the method named like the node, which decides what to visit of it and
    when: it gets the parts of the node as parsed, and visits one with `self.visit(part)`. The
    signature of a method says what it gets: the fields of the node by name, `*children` for the
    children, one other parameter for the node, and keyword-only `meta`, `node` or `children`
    beside. A node without a method gives the list of its visited children.

    A token gives what the method named like its terminal returns, else the value of the
    terminal's builder (`=>` in the grammar), else itself. The builders of rules are not applied.

    `class I(Interpreter, strict=True)` makes every public method of `I` name a rule or terminal
    of the grammar it meets. The walk recurses in Python, so a tree nested deeper than Python's
    recursion limit allows ends it with `RecursionError`. A `Transformer` does not recurse.
    """

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


class Visitor(_Walker):
    """Call a method for every node of a tree, with methods named like the rules of a grammar.

    `visit(tree)` calls the method named like each node, children before their parents, and
    `visit_topdown(tree)` parents before their children. A method gets the parts of its node as
    parsed, by its signature, as a method of an `Interpreter` does. What it returns is dropped:
    a visitor looks at a tree, to collect from it or to check it, and gives it back unchanged.
    Tokens have no methods.

    `class V(Visitor, strict=True)` makes every public method of `V` name a rule of the grammar
    it meets. The walk does not recurse, so a tree may nest as deeply as it likes.
    """

    def visit(self, tree: Any) -> Any:
        """Call the method of every node, children before their parents, and return the tree."""
        if isinstance(tree, Tree):
            for node in tree.iter_subtrees():
                _call(self, node)
        return tree

    def visit_topdown(self, tree: Any) -> Any:
        """Call the method of every node, parents before their children, and return the tree."""
        if isinstance(tree, Tree):
            for node in tree.iter_subtrees_topdown():
                _call(self, node)
        return tree


Interpreter.__fpeg_reserved__ = reserved_names(Interpreter)
Visitor.__fpeg_reserved__ = reserved_names(Visitor)
