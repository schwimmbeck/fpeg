# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The Transformer: methods named like the rules and terminals of a grammar, called children first.

The signature of a method says what it gets. This module reads the signatures once per class and
grammar into a plan, and the native scan of `fpeg._fpeg` follows the plan over the tree.
"""

from __future__ import annotations

import inspect
import types
from collections.abc import Callable
from typing import Any, cast

from fpeg import _fpeg
from fpeg._fpeg import Token, Tree

# How a plan calls a method, as the native scan reads it.
LIST = 1
INLINE = 2
NOTHING = 4
TOKEN = 8


class Transformer:
    """Turn a tree into values, with methods named like the rules and terminals of a grammar.

    A method gives the value of the nodes or tokens of its name, called children first. A
    node without a method gets the value of `__default__` when a subclass overrides it, else
    it stays a tree. The signature of a method
    says what it gets: `*children` for the children, one parameter for the list of children,
    or nothing.
    """

    def __init__(self, visit_tokens: bool = True) -> None:
        """Make a transformer. Without `visit_tokens` the methods of terminals are not called."""
        self.__visit_tokens__ = visit_tokens

    def transform(self, tree: Any) -> Any:
        """Return the value of a tree or token, or anything else as it is."""
        return self._values(tree)[0]

    def __default__(self, data: Any, children: list[Any], meta: Any) -> Any:
        """Make the value of a node without a method, when a subclass overrides it."""
        return Tree(data, children, meta)

    def __default_token__(self, token: Token) -> Any:
        """Make the value of a token without a method, when a subclass overrides it."""
        return token

    def _values(self, item: Any) -> tuple[Any, ...]:
        """Return the value an item gives its parent, in a tuple."""
        if isinstance(item, Tree):
            return _fpeg.scan(self, item, _make_plan)
        if isinstance(item, Token):
            return self._transform_token(item)
        return (item,)

    def _transform_token(self, token: Token) -> tuple[Any, ...]:
        cls = type(self)
        name = token.type
        if getattr(self, "__visit_tokens__", True) and _is_handler(cls, name):
            mode = _token_convention(cls, name)
            method = getattr(self, name)
            call: Callable[[], Any] = (
                (lambda: method()) if mode == NOTHING else (lambda: method(token))
            )
            return (call(),)
        if _overrides(cls, "__default_token__"):
            return (self.__default_token__(token),)
        return (token,)


# The names a subclass cannot use as handlers, since the base class needs them.
_BASE_NAMES = frozenset(vars(Transformer)) | frozenset(dir(object))


def _is_handler(cls: type, name: str) -> bool:
    """Tell whether the class has a method for the rule or terminal `name`."""
    if name in _BASE_NAMES or not name.isidentifier():
        return False
    attribute = inspect.getattr_static(cls, name, None)
    return attribute is not None and (
        callable(attribute) or isinstance(attribute, (staticmethod, classmethod))
    )


def _overrides(cls: type, name: str) -> bool:
    return inspect.getattr_static(cls, name) is not vars(Transformer)[name]


def _unwrap(method: Any) -> Any:
    """Return the function a staticmethod or classmethod wraps, or the object itself."""
    if isinstance(method, (staticmethod, classmethod)):
        return cast(Any, method).__func__
    return method


def _function(cls: type, name: str) -> Callable[..., Any] | None:
    """Return the function behind a method, or `None` for another callable such as `float`."""
    function = _unwrap(inspect.getattr_static(cls, name))
    return function if isinstance(function, types.FunctionType) else None


def _parameters(cls: type, name: str, function: Callable[..., Any]) -> list[inspect.Parameter]:
    """Return the parameters of a method after `self` or `cls`."""
    parameters = list(inspect.signature(function).parameters.values())
    if not isinstance(inspect.getattr_static(cls, name), staticmethod):
        parameters = parameters[1:]
    return parameters


def _token_convention(cls: type, name: str) -> int:
    """Say how a method of a terminal is called: with the token, or with nothing."""
    function = _function(cls, name)
    if function is None:
        # Another callable, such as `float`, gets the token.
        return TOKEN
    parameters = _parameters(cls, name, function)
    if any(p.kind == p.VAR_POSITIONAL for p in parameters):
        return TOKEN
    positional = [p for p in parameters if p.kind in (p.POSITIONAL_ONLY, p.POSITIONAL_OR_KEYWORD)]
    required = [p for p in positional if p.default is p.empty]
    if not positional:
        return NOTHING
    if len(required) <= 1:
        return TOKEN
    raise TypeError(
        f"{cls.__qualname__}.{name} takes {len(required)} arguments, and a terminal's method "
        "takes its token"
    )


def _node_convention(cls: type, name: str) -> int:
    """Say how a method of a node is called."""
    function = _function(cls, name)
    if function is None:
        # Another callable, such as `list`, gets the list of children, as in Lark.
        return LIST
    parameters = _parameters(cls, name, function)
    for parameter in parameters:
        if parameter.kind == parameter.KEYWORD_ONLY and parameter.default is parameter.empty:
            raise TypeError(
                f"{cls.__qualname__}.{name} takes the keyword {parameter.name}, and fpeg passes "
                "nothing by keyword"
            )
    positional = [p for p in parameters if p.kind in (p.POSITIONAL_ONLY, p.POSITIONAL_OR_KEYWORD)]
    if any(p.kind == p.VAR_POSITIONAL for p in parameters):
        if positional:
            raise TypeError(
                f"{cls.__qualname__}.{name} takes parameters before its *parameter, which fpeg "
                "cannot fill"
            )
        return INLINE
    if not positional:
        return NOTHING
    if len(positional) == 1:
        return LIST
    names = ", ".join(p.name for p in positional)
    raise TypeError(
        f"{cls.__qualname__}.{name} takes {names}. Take the children as one list or as *children"
    )


def _make_plan(
    cls: type, kinds: list[tuple[str, bool]]
) -> tuple[list[tuple[int, str, int]], bool, bool]:
    """Make the plan of a transformer class for a grammar.

    It says for each kind with a method how the method is called. The native scan asks for
    it once per class and grammar.
    """
    entries: list[tuple[int, str, int]] = []
    conventions: dict[tuple[str, bool], int] = {}
    for kind, (name, token) in enumerate(kinds):
        if not _is_handler(cls, name):
            continue
        if (name, token) not in conventions:
            if token:
                conventions[(name, token)] = _token_convention(cls, name)
            else:
                conventions[(name, token)] = _node_convention(cls, name)
        entries.append((kind, name, conventions[(name, token)]))
    return entries, _overrides(cls, "__default__"), _overrides(cls, "__default_token__")
