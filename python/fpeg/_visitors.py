# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The Interpreter and the Visitor: methods named like rules, called by a walk in Python.

A method gets the parts of its node as parsed, not yet visited, and what it gets its signature
says, read as the Transformer reads it, once per class and grammar. The Interpreter visits what
its methods ask for and recurses as deeply as the tree nests. The Visitor calls a method for
every node and does not recurse.
"""

from __future__ import annotations

from typing import Any, ClassVar, Final, cast

from fpeg import _fpeg
from fpeg._fpeg import Token, Tree
from fpeg._transformer import (
    EXTRAS,
    FIELDS,
    INLINE,
    NOTHING,
    check_strict,
    function_of,
    handler,
    node_convention,
    reserved_names,
    static_attribute,
    token_convention,
)

# How a method of a node is called: its mode, the fields it takes and its extras.
_Convention = tuple[int, tuple[str, ...], int]

# The method of a node, by its name, and how it is called.
_Handler = tuple[str, _Convention]

# What a call gives for a node without a method.
_NO_METHOD: Final = object()

_NO_VALUE = _fpeg.NO_VALUE

# The attributes under which a class keeps how its methods are called: those of trees built in
# Python, which have no grammar, and those of terminals. No identifier, so that no rule and no
# method can have their names.
_OWNED: Final = "_fpeg owned"
_TOKENS: Final = "_fpeg tokens"


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


def _refuse_v_args(cls: type, name: str) -> None:
    _, options = function_of(cls, name)
    if options:
        raise TypeError(
            f"{cls.__qualname__}.{name} has v_args, which says how a Transformer calls a method"
        )


def _refuse(item: Any) -> None:
    """Refuse an absent optional field and a field of several items, visited by mistake."""
    if item is None or isinstance(item, list):
        what = "None, as an absent optional field is" if item is None else "a list"
        raise TypeError(f"visit takes a node or a token, and got {what}")


def _convention(cls: type, name: str, fields: dict[str, bool] | None) -> _Convention:
    _refuse_v_args(cls, name)
    return node_convention(cls, name, fields)


def _make_plan(
    cls: type, kinds: list[tuple[str, bool, tuple[tuple[str, bool], ...]]]
) -> dict[str, _Handler]:
    """Say for each kind of node with a method which method it is and how it is called.

    `fpeg._fpeg.walk_plan` asks for it once per class and grammar.
    """
    if getattr(cls, "__fpeg_strict__", False):
        check_strict(cls, {name for name, _, _ in kinds})
    plan: dict[str, _Handler] = {}
    for name, token, fields in kinds:
        method = handler(cls, name)
        if method is None:
            continue
        if not token and name not in plan:
            plan[name] = (method, _convention(cls, method, dict(fields)))
        elif token and issubclass(cls, Visitor):
            raise TypeError(
                f"{cls.__qualname__}.{method} is named like a terminal, and a Visitor calls "
                "methods for nodes only"
            )
    return plan


def _owned_handler(cls: type, name: Any) -> _Handler | None:
    """Say which method a tree built in Python has and how it is called, if there is one."""
    if not isinstance(name, str):
        return None
    by_name = _kept(cls, _OWNED)
    if name not in by_name:
        method = handler(cls, name)
        by_name[name] = None if method is None else (method, _convention(cls, method, None))
    return by_name[name]


def _token_handler(cls: type, name: Any) -> tuple[str, int] | None:
    """Say which method a terminal has and how it is called, if there is one."""
    if not isinstance(name, str):
        return None
    by_name = _kept(cls, _TOKENS)
    if name not in by_name:
        method = handler(cls, name)
        if method is not None:
            _refuse_v_args(cls, method)
        by_name[name] = None if method is None else (method, token_convention(cls, method))
    return by_name[name]


def _note(error: BaseException, where: str, line: int | None, column: int | None) -> None:
    """Say on an error where it came from, once: the innermost method says it."""
    notes: object = getattr(error, "__notes__", [])
    # A note cannot be added where the notes are no list, and trying would replace the error.
    if not isinstance(notes, list):
        return
    if any(
        isinstance(note, str) and note.startswith("fpeg: ") for note in cast(list[object], notes)
    ):
        return
    place = "" if line is None else f" at line {line}, column {column}"
    error.add_note(f"fpeg: {where}{place}")


def _call(walker: object, node: Tree) -> Any:
    """Call the method of a node as its signature asks, or return `_NO_METHOD`."""
    cls = type(walker)
    name = node.data
    plan = _fpeg.walk_plan(cls, node, _make_plan)
    # A tree built in Python has no grammar to plan for.
    found = _owned_handler(cls, name) if plan is None else plan.get(name)
    if found is None:
        return _NO_METHOD
    method, (mode, fields, extras) = found
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
            return getattr(walker, method)(*args)
        kwargs: dict[str, Any] = {}
        if extras & EXTRAS["meta"]:
            kwargs["meta"] = node.meta
        if extras & EXTRAS["node"]:
            kwargs["node"] = node
        if extras & EXTRAS["children"]:
            kwargs["children"] = node.children
        return getattr(walker, method)(*args, **kwargs)
    except BaseException as error:
        meta = node.meta
        where = f"in {cls.__qualname__}.{method}, for the node {name}"
        _note(error, where, meta.line, meta.column)
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
            if result is not _NO_METHOD:
                return result
            try:
                return self.__default__(item)
            except BaseException as error:
                cls = type(self)
                # The base visits the children, whose methods note their own errors.
                if static_attribute(cls, "__default__") is not vars(Interpreter)["__default__"]:
                    meta = item.meta
                    where = f"in {cls.__qualname__}.__default__, for the node {item.data}"
                    _note(error, where, meta.line, meta.column)
                raise
        if isinstance(item, Token):
            return self._visit_token(item)
        _refuse(item)
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
        found = _token_handler(cls, name)
        try:
            if found is None:
                built = _fpeg.token_value(token)
                return token if built is _NO_VALUE else built
            method, mode = found
            bound = getattr(self, method)
            return bound() if mode == NOTHING else bound(token)
        except BaseException as error:
            where = f"the builder of {name}" if found is None else f"{cls.__qualname__}.{found[0]}"
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
        """Call the method of every node, children before their parents, and return the tree.

        `None` and a list are refused, as an `Interpreter` refuses them.
        """
        _refuse(tree)
        if isinstance(tree, Tree):
            for node in tree.iter_subtrees():
                _call(self, node)
        return tree

    def visit_topdown(self, tree: Any) -> Any:
        """Call the method of every node, parents before their children, and return the tree."""
        _refuse(tree)
        if isinstance(tree, Tree):
            for node in tree.iter_subtrees_topdown():
                _call(self, node)
        return tree


Interpreter.__fpeg_reserved__ = reserved_names(Interpreter)
Visitor.__fpeg_reserved__ = reserved_names(Visitor)
