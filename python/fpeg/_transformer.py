# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The Transformer: methods named like the rules and terminals of a grammar, called children first.

The signature of a method says what it gets. This module reads the signatures once per class and
grammar into a plan, and the native scan of `fpeg._fpeg` follows the plan over the tree.
"""

from __future__ import annotations

import inspect
import types
from collections.abc import Callable, Iterable
from typing import Any, TypeVar, cast

from fpeg import _fpeg
from fpeg._fpeg import Discard, Token, Tree

# How a plan calls a method, as the native scan reads it.
LIST = 1
INLINE = 2
FIELDS = 3
NOTHING = 4
META = 5
META_INLINE = 6
TREE = 7
TOKEN = 8

# The keyword-only parameters a method may take beside its children or fields, as bits.
EXTRAS = {"meta": 1, "node": 2, "children": 4}

_Callable = TypeVar("_Callable")


class _VArgs:
    """A callable that is no function, such as `float`, with the options of `v_args`."""

    def __init__(self, function: Callable[..., Any], options: tuple[bool, bool, bool]) -> None:
        self.function = function
        self.__fpeg_v_args__ = options

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        return self.function(*args, **kwargs)


def v_args(
    inline: bool = False, meta: bool = False, tree: bool = False, wrapper: None = None
) -> Callable[[_Callable], _Callable]:
    """Say how methods are called, as Lark's `v_args` does, for code ported from Lark.

    `inline` passes the children as arguments, `meta` passes the node's position first and
    `tree` passes a tree of the transformed children. On a class it applies to every method
    without options of its own. fpeg reads the same from a signature without a decorator:
    `*children` for inline, and a keyword-only `meta` parameter for the position.
    """
    if wrapper is not None:
        raise TypeError("fpeg's v_args takes no wrapper")
    options = (inline, meta, tree)

    def decorate(target: _Callable) -> _Callable:
        if isinstance(target, type):
            target.__fpeg_v_args__ = options  # type: ignore[attr-defined] # a class carries the default
            return target
        function = _unwrap(target)
        if isinstance(function, types.FunctionType):
            function.__fpeg_v_args__ = options  # type: ignore[attr-defined] # read by the plan
            return target
        return _VArgs(target, options)  # type: ignore[arg-type,return-value] # a builtin keeps no attributes

    return decorate


class Transformer:
    """Turn a tree into values, with methods named like the rules and terminals of a grammar.

    A method gives the value of the nodes or tokens of its name, called children first. A
    node without a method gets the value of `__default__` when a subclass overrides it, else
    it stays a tree. The signature of a method
    says what it gets: the fields of the node by name, `*children` for the children, one other
    parameter for the list of children, and keyword-only `meta`, `node` or `children` beside.
    `return fpeg.Discard` removes the node from its parent, and a method that yields puts each
    value into the parent in the node's place.
    """

    def __init__(self, visit_tokens: bool = True) -> None:
        """Make a transformer. Without `visit_tokens` the methods of terminals are not called."""
        self.__visit_tokens__ = visit_tokens

    def transform(self, tree: Any) -> Any:
        """Return the value of a tree or token, or anything else as it is.

        At the top, where no parent takes them, the values of a method that yields come back as
        a list, and `Discard` as an empty list.
        """
        values = self._values(tree)
        return values[0] if len(values) == 1 else list(values)

    def __default__(self, data: Any, children: list[Any], meta: Any) -> Any:
        """Make the value of a node without a method, when a subclass overrides it."""
        return Tree(data, children, meta)

    def __default_token__(self, token: Token) -> Any:
        """Make the value of a token without a method, when a subclass overrides it."""
        return token

    def _values(self, item: Any) -> tuple[Any, ...]:
        """Return the values an item gives its parent: one, none for `Discard`, or several."""
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
            return _results(call())
        if _overrides(cls, "__default_token__"):
            return _results(self.__default_token__(token))
        return (token,)


# The names a subclass cannot use as handlers, since the base class needs them.
_BASE_NAMES = frozenset(vars(Transformer)) | frozenset(dir(object))


def _results(result: Any) -> tuple[Any, ...]:
    """Return the values a result gives the parent."""
    if result is Discard:
        return ()
    if isinstance(result, types.GeneratorType):
        return tuple(cast(Iterable[Any], result))
    return (result,)


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


def _function(cls: type, name: str) -> tuple[Callable[..., Any] | None, Any]:
    """Return the function behind a method and its `v_args` options."""
    attribute = inspect.getattr_static(cls, name)
    if isinstance(attribute, _VArgs):
        return None, attribute.__fpeg_v_args__
    function = _unwrap(attribute)
    options = getattr(function, "__fpeg_v_args__", None) or getattr(cls, "__fpeg_v_args__", None)
    if not isinstance(function, types.FunctionType):
        return None, options
    return function, options


def _parameters(cls: type, name: str, function: Callable[..., Any]) -> list[inspect.Parameter]:
    """Return the parameters of a method after `self` or `cls`."""
    parameters = list(inspect.signature(function).parameters.values())
    if not isinstance(inspect.getattr_static(cls, name), staticmethod):
        parameters = parameters[1:]
    return parameters


def _token_convention(cls: type, name: str) -> int:
    """Say how a method of a terminal is called: with the token, or with nothing."""
    function, _ = _function(cls, name)
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


def _node_convention(
    cls: type, name: str, fields: dict[str, bool]
) -> tuple[int, tuple[str, ...], int]:
    """Say how a method of a node is called: its mode, the fields it takes and its extras."""
    function, options = _function(cls, name)
    if function is None:
        inline, meta, tree = options or (False, False, False)
        return _option_mode(inline, meta, tree), (), 0
    parameters = _parameters(cls, name, function)
    extras = 0
    for parameter in parameters:
        if parameter.kind == parameter.KEYWORD_ONLY:
            if parameter.name not in EXTRAS:
                raise TypeError(
                    f"{cls.__qualname__}.{name} takes the keyword {parameter.name}, and a method "
                    f"takes only {', '.join(EXTRAS)} by keyword"
                )
            extras |= EXTRAS[parameter.name]
    if options:
        return _option_mode(*options), (), extras
    positional = [p for p in parameters if p.kind in (p.POSITIONAL_ONLY, p.POSITIONAL_OR_KEYWORD)]
    if any(p.kind == p.VAR_POSITIONAL for p in parameters):
        if positional:
            raise TypeError(
                f"{cls.__qualname__}.{name} takes parameters before its *parameter, which fpeg "
                "cannot fill"
            )
        return INLINE, (), extras
    if not positional:
        return NOTHING, (), extras
    names = tuple(p.name for p in positional)
    if fields and all(n in fields for n in names):
        return FIELDS, names, extras
    if len(names) == 1:
        return LIST, (), extras
    known = f"the fields of {name} are {', '.join(fields)}" if fields else f"{name} has no fields"
    raise TypeError(
        f"{cls.__qualname__}.{name} takes {', '.join(names)}, and {known}. Take the children as "
        "one list, as *children, or by field"
    )


def _option_mode(inline: bool, meta: bool, tree: bool) -> int:
    if tree:
        return TREE
    if meta:
        return META_INLINE if inline else META
    return INLINE if inline else LIST


def _make_plan(
    cls: type, kinds: list[tuple[str, bool, tuple[tuple[str, bool], ...]]]
) -> tuple[list[tuple[int, str, int, tuple[str, ...], int]], bool, bool]:
    """Make the plan of a transformer class for a grammar.

    It says for each kind with a method how the method is called. The native scan asks for
    it once per class and grammar.
    """
    entries: list[tuple[int, str, int, tuple[str, ...], int]] = []
    conventions: dict[tuple[str, bool], tuple[int, tuple[str, ...], int]] = {}
    for kind, (name, token, fields) in enumerate(kinds):
        if not _is_handler(cls, name):
            continue
        if (name, token) not in conventions:
            if token:
                conventions[(name, token)] = (_token_convention(cls, name), (), 0)
            else:
                conventions[(name, token)] = _node_convention(cls, name, dict(fields))
        mode, field_names, extras = conventions[(name, token)]
        entries.append((kind, name, mode, field_names, extras))
    return entries, _overrides(cls, "__default__"), _overrides(cls, "__default_token__")
