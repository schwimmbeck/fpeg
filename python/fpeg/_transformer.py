# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The Transformer: methods named like the rules and terminals of a grammar, called children first.

The signature of a method says what it gets. This module reads the signatures once per class and
grammar into a plan, and the native scan of `fpeg._fpeg` follows the plan over the tree. Trees
built in Python, which have no grammar, take the slower walk of `Transformer._transform_owned`.
"""

from __future__ import annotations

import difflib
import inspect
import keyword
import sys
import types
import weakref
from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass
from typing import Any, ClassVar, Literal, TypeVar, cast

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

# How columns count, as the native scan reads it.
COLUMN_UNITS = {"code points": 0, "utf-8": 1, "utf-16": 2}
# The largest base of lines or columns either way, which keeps every position in 63 bits.
MAX_POSITION_BASE = 2**31

_Callable = TypeVar("_Callable")


@dataclass(frozen=True)
class Positions:
    """Which attributes of a value receive the position of the node or token it was made for.

    `types` are the classes whose instances get them. Lines count from `line_base`, columns
    from `column_base` in the `column_unit`: code points, UTF-8 bytes or UTF-16 units. A
    value that has the first attribute already, also as a default of its class, keeps its own.
    A record checks its fields when it is made.
    """

    start_line: str
    start_column: str
    end_line: str
    end_column: str
    types: type | tuple[type, ...]
    line_base: int = 1
    column_base: int = 0
    column_unit: Literal["code points", "utf-8", "utf-16"] = "code points"

    def __post_init__(self) -> None:
        # The fields are checked as what a caller may pass, whatever their annotations say.
        names = cast(
            tuple[object, ...], (self.start_line, self.start_column, self.end_line, self.end_column)
        )
        if not all(isinstance(name, str) and name.isidentifier() for name in names):
            raise TypeError(f"the attributes of Positions are names, not {names!r}")
        if len(set(names)) < len(names):
            raise ValueError(f"the attributes of Positions are four names, not {names!r}")
        given: object = self.types
        types = cast(tuple[object, ...], given) if isinstance(given, tuple) else (given,)
        if not types or not all(isinstance(t, type) for t in types):
            raise TypeError(
                f"the types of Positions are a class or a tuple of them, not {self.types!r}"
            )
        object.__setattr__(self, "types", types)
        for name in ("line_base", "column_base"):
            base = getattr(self, name)
            if isinstance(base, bool) or not isinstance(base, int):
                raise TypeError(f"{name} of Positions is an int, not {base!r}")
            if abs(base) > MAX_POSITION_BASE:
                raise ValueError(
                    f"{name} of Positions is at most {MAX_POSITION_BASE} either way, not {base}"
                )
        if self.column_unit not in COLUMN_UNITS:
            units = ", ".join(repr(unit) for unit in COLUMN_UNITS)
            raise ValueError(f"column_unit of Positions is {units}, not {self.column_unit!r}")


def _ast_positions() -> Positions:
    """Make the preset `"ast"`: the attributes of Python's `ast`, imported only when asked for."""
    import ast

    return Positions(
        "lineno", "col_offset", "end_lineno", "end_col_offset", ast.AST, column_unit="utf-8"
    )


class _VArgs:
    """A callable that is no function, such as `float`, with the options of `v_args`."""

    def __init__(self, function: Callable[..., Any], options: tuple[bool, bool, bool]) -> None:
        self.function = function
        self.__fpeg_v_args__ = options

    def __call__(self, *args: Any, **kwargs: Any) -> Any:
        return self.function(*args, **kwargs)


def v_args(
    inline: bool = False, meta: bool = False, tree: bool = False
) -> Callable[[_Callable], _Callable]:
    """Say by decorator how methods are called.

    `inline` passes the children as arguments, `meta` passes the node's position first and
    `tree` passes a tree of the transformed children. On a class it applies to every method
    without options of its own. fpeg reads the same from a signature without a decorator:
    `*children` for inline, and a keyword-only `meta` parameter for the position.
    """
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
    node without a method gets the value of its rule's builder (`=>` in the grammar), else of
    `__default__` when a subclass overrides it, else it stays a tree. The signature of a method
    says what it gets: the fields of the node by name, `*children` for the children, one other
    parameter for the list of children, and keyword-only `meta`, `node` or `children` beside.
    `return fpeg.Discard` removes the node from its parent, and a method that yields puts each
    value into the parent in the node's place. In a tree that a parse recovered, `__error__`
    gives the value of an `<error>` node and `__missing__` that of a `<missing>` token, which
    otherwise stay as they are.

    `class T(Transformer, strict=True)` makes every public method of `T` name a rule or terminal
    of the grammar it meets. `class T(Transformer, positions="ast")` gives each value made for
    a node or token of a parse its position, where the value is an `ast.AST` that has none yet,
    and a `Positions` record names other attributes. Subclasses keep the strictness and the
    positions of their base.
    """

    __fpeg_strict__: ClassVar[bool] = False
    __fpeg_reserved__: ClassVar[frozenset[str]]
    __fpeg_positions__: ClassVar[Positions | None] = None

    def __init_subclass__(
        cls,
        *,
        strict: bool | None = None,
        positions: Positions | Literal["ast"] | None = None,
        **kwargs: Any,
    ) -> None:
        super().__init_subclass__(**kwargs)
        if strict is not None:
            cls.__fpeg_strict__ = strict
        if positions == "ast":
            cls.__fpeg_positions__ = _ast_positions()
        elif isinstance(positions, Positions):
            cls.__fpeg_positions__ = positions
        elif positions is not None:
            raise TypeError(f"positions takes 'ast' or a Positions, not {positions!r}")

    def __init__(self, visit_tokens: bool = True) -> None:
        """Make a transformer. Without `visit_tokens` the methods of terminals are not called."""
        self.__visit_tokens__ = visit_tokens

    def transform(self, tree: Any) -> Any:
        """Return the value of a tree or token, or anything else as it is.

        At the top, where no parent takes them, the values of a method that yields come back as
        a list, and `Discard` as an empty list.
        """
        values = self._values(tree)
        return values if isinstance(values, list) else values[0]

    def __default__(self, data: Any, children: list[Any], meta: Any) -> Any:
        """Make the value of a node without a method or builder, when a subclass overrides it."""
        return Tree(data, children, meta)

    def __default_token__(self, token: Token) -> Any:
        """Make the value of a token without a method or builder, when a subclass overrides it."""
        return token

    def _values(self, item: Any) -> Sequence[Any]:
        """Return what an item gives its parent: one value in a tuple, or a list it spliced."""
        if isinstance(item, Tree):
            if _fpeg.is_view(item):
                return _fpeg.scan(self, item, _make_plan)
            return self._transform_owned(item)
        if isinstance(item, Token):
            return self._transform_token(item)
        return (item,)

    def _transform_token(self, token: Token) -> Sequence[Any]:
        cls = type(self)
        # A token made in Python may have any object as its type.
        name: Any = token.type
        visit = getattr(self, "__visit_tokens__", True)
        handled = handler(cls, name) if visit and isinstance(name, str) else None
        if handled is not None:
            mode = token_convention(cls, handled)
            method = getattr(self, handled)
            call: Callable[[], Any] = (
                (lambda: method()) if mode == NOTHING else (lambda: method(token))
            )
            where = f"in {cls.__qualname__}.{handled}, for the token {name}"
            # A method that yields runs while its values are taken.
            return _noted(lambda: _results(call()), where)
        where = f"in the builder of {name}, for a token"
        built = _noted(lambda: _fpeg.token_value(token), where)
        if built is not _NO_VALUE:
            # A function that a builder calls may yield too.
            return _noted(lambda: _results(built), where)
        if getattr(self, "__visit_tokens__", True) and _overrides(cls, "__default_token__"):
            where = f"in {cls.__qualname__}.__default_token__, for the token {name}"
            return _noted(lambda: _results(self.__default_token__(token)), where)
        return (token,)

    def _transform_owned(self, root: Tree) -> Sequence[Any]:
        """Transform a tree built in Python, children first, without recursion."""
        values: list[Any] = []
        on_path: set[int] = set()
        # An item to transform, or a tree whose children's values start at a position of values.
        pending: list[tuple[Any, int]] = [(root, -1)]
        # The root comes last, and its values are what the walk gives.
        while True:
            item, start = pending.pop()
            if start >= 0:
                on_path.discard(id(item))
                children = values[start:]
                del values[start:]
                own = self._owned_value(item, children)
                if item is root:
                    return own
                values.extend(own)
            elif isinstance(item, Tree) and not _fpeg.is_view(item):
                if id(item) in on_path:
                    raise ValueError("the tree contains itself")
                on_path.add(id(item))
                pending.append((item, len(values)))
                pending.extend((child, -1) for child in reversed(item.children))
            else:
                values.extend(self._values(item))

    def _owned_value(self, tree: Tree, children: list[Any]) -> Sequence[Any]:
        cls = type(self)
        # A tree built in Python may have any object as data.
        name: Any = tree.data
        handled = handler(cls, name) if isinstance(name, str) else None
        if handled is not None:
            mode, _, extras = _owned_convention(cls, handled)
            method = getattr(self, handled)
            args = _arguments(mode, children, tree)
            kwargs = _extras(extras, tree, children)
            where = f"in {cls.__qualname__}.{handled}, for the tree {name}"
            return _noted(lambda: _results(method(*args, **kwargs)), where)
        if _overrides(cls, "__default__"):
            where = f"in {cls.__qualname__}.__default__, for the tree {name}"
            return _noted(lambda: _results(self.__default__(name, children, tree.meta)), where)
        if len(children) == len(tree.children) and all(
            a is b for a, b in zip(children, tree.children, strict=True)
        ):
            return (tree,)
        return (Tree(name, children, tree.meta),)


# What `_fpeg.token_value` gives for a token without a builder.
_NO_VALUE = _fpeg.NO_VALUE


def reserved_names(base: type) -> frozenset[str]:
    """Return the names a subclass cannot use as handlers, since its base class needs them."""
    return frozenset(name for cls in base.__mro__ for name in vars(cls))


Transformer.__fpeg_reserved__ = reserved_names(Transformer)


def _results(result: Any) -> Sequence[Any]:
    """Return what a result gives the parent: one value in a tuple, or a list it spliced."""
    if result is Discard:
        return []
    if isinstance(result, types.GeneratorType):
        # A yielded Discard puts nothing, as a returned one does.
        return [value for value in cast(Iterable[Any], result) if value is not Discard]
    return (result,)


def _noted(call: Callable[[], Any], where: str) -> Any:
    try:
        return call()
    except BaseException as error:
        # A note cannot be added where the notes are no list, and trying would replace the error.
        if isinstance(getattr(error, "__notes__", []), list):
            error.add_note(f"fpeg: {where}")
        raise


def _arguments(mode: int, children: list[Any], tree: Tree) -> tuple[Any, ...]:
    if mode == LIST:
        return (children,)
    if mode == INLINE:
        return tuple(children)
    if mode == NOTHING:
        return ()
    if mode == META:
        return (tree.meta, children)
    if mode == META_INLINE:
        return (tree.meta, *children)
    return (Tree(tree.data, children, tree.meta),)


def _extras(extras: int, tree: Tree, children: list[Any]) -> dict[str, Any]:
    kwargs: dict[str, Any] = {}
    if extras & EXTRAS["meta"]:
        kwargs["meta"] = tree.meta
    if extras & EXTRAS["node"]:
        kwargs["node"] = tree
    if extras & EXTRAS["children"]:
        kwargs["children"] = children
    return kwargs


def static_attribute(cls: type, name: str) -> Any:
    """Return what the class or one of its bases holds under `name`, or `None`.

    Unlike `inspect.getattr_static`, it does not look at the metaclass, whose methods, such as
    `mro`, a rule may be named like.
    """
    for base in cls.__mro__:
        if name in vars(base):
            return vars(base)[name]
    return None


def is_handler(cls: type, name: str) -> bool:
    """Tell whether the class has a method for the rule or terminal `name`."""
    if name in getattr(cls, "__fpeg_reserved__", ()) or not name.isidentifier():
        return False
    attribute = static_attribute(cls, name)
    return attribute is not None and (
        callable(attribute) or isinstance(attribute, (staticmethod, classmethod))
    )


def handler(cls: type, name: str) -> str | None:
    """Return the name of the method of the class for the rule or terminal `name`, if it has one.

    No method in a class body can be named like a Python keyword, and none can take the name
    of a method the class needs itself, such as `transform`. A rule of such a name has its
    method under the name with an underscore added, as `if_` for `if`, which is how PEP 8
    writes such names. A rule that an import brings in for the rules it takes, `m.r`, has its
    method under `m__r`.
    """
    name = method_name(name)
    if is_handler(cls, name):
        return name
    if _taken(cls, name) and is_handler(cls, name + "_"):
        return name + "_"
    return None


def method_name(name: str) -> str:
    """Return the method name of the rule or terminal `name`, `m__r` for an import's `m.r`.

    The node of what a recovery matched has its method `__error__`, and the token of one that
    matched nothing `__missing__`.
    """
    if name in _RECOVERED:
        return _RECOVERED[name]
    if "." in name and all(part.isidentifier() for part in name.split(".")):
        return name.replace(".", "__")
    return name


# The methods of the kinds that a recovering parse makes.
_RECOVERED = {"<error>": "__error__", "<missing>": "__missing__"}


def answered(cls: type, method: str, name: str, answering: dict[str, str]) -> None:
    """Refuse a method that two rules or terminals reach, whose calls one way cannot serve."""
    other = answering.setdefault(method, name)
    if other != name:
        raise TypeError(
            f"{cls.__qualname__}.{method} answers both {other} and {name}, rename one of them"
        )


def _taken(cls: type, name: str) -> bool:
    return keyword.iskeyword(name) or name in getattr(cls, "__fpeg_reserved__", ())


def _overrides(cls: type, name: str) -> bool:
    return static_attribute(cls, name) is not vars(Transformer)[name]


def _unwrap(method: Any) -> Any:
    """Return the function a staticmethod or classmethod wraps, or the object itself."""
    if isinstance(method, (staticmethod, classmethod)):
        return cast(Any, method).__func__
    return method


def function_of(cls: type, name: str) -> tuple[Callable[..., Any] | None, Any]:
    """Return the function behind a method and its `v_args` options."""
    attribute = static_attribute(cls, name)
    if isinstance(attribute, _VArgs):
        return None, attribute.__fpeg_v_args__
    function = _unwrap(attribute)
    options = getattr(function, "__fpeg_v_args__", None) or getattr(cls, "__fpeg_v_args__", None)
    if not isinstance(function, types.FunctionType):
        return None, options
    return function, options


if sys.version_info >= (3, 14):
    import annotationlib

    def _signature(function: Callable[..., Any]) -> inspect.Signature:
        # The plan needs no annotations, and evaluating them fails on a name that only a
        # type checker imports.
        return inspect.signature(function, annotation_format=annotationlib.Format.STRING)

else:

    def _signature(function: Callable[..., Any]) -> inspect.Signature:
        return inspect.signature(function)


def _parameters(cls: type, name: str, function: Callable[..., Any]) -> list[inspect.Parameter]:
    """Return the parameters of a method after `self` or `cls`."""
    parameters = list(_signature(function).parameters.values())
    if not isinstance(static_attribute(cls, name), staticmethod):
        parameters = parameters[1:]
    return parameters


def token_convention(cls: type, name: str) -> int:
    """Say how a method of a terminal is called: with the token, or with nothing."""
    function, _ = function_of(cls, name)
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


def node_convention(
    cls: type, name: str, fields: dict[str, bool] | None
) -> tuple[int, tuple[str, ...], int]:
    """Say how a method of a node is called: its mode, the fields it takes and its extras.

    `fields` is `None` for a tree built in Python, which has none.
    """
    function, options = function_of(cls, name)
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
    if fields is None:
        known = "a tree built in Python or rebuilt by a transform has no fields"
    elif fields:
        known = f"the fields of {name} are {', '.join(fields)}"
    else:
        known = f"{name} has no fields"
    raise TypeError(
        f"{cls.__qualname__}.{name} takes {', '.join(names)}, and {known}. A method takes "
        "fields by name, one other parameter, or *children"
    )


def _option_mode(inline: bool, meta: bool, tree: bool) -> int:
    if tree:
        return TREE
    if meta:
        return META_INLINE if inline else META
    return INLINE if inline else LIST


# By class, weakly, so that a class made for one task goes when it is no longer used.
_owned_conventions: weakref.WeakKeyDictionary[type, dict[str, tuple[int, tuple[str, ...], int]]] = (
    weakref.WeakKeyDictionary()
)


def _owned_convention(cls: type, name: str) -> tuple[int, tuple[str, ...], int]:
    """Say how a method is called for a tree built in Python, read once per class and name."""
    by_name = _owned_conventions.setdefault(cls, {})
    if name not in by_name:
        by_name[name] = node_convention(cls, name, None)
    return by_name[name]


def _make_plan(
    cls: type, kinds: list[tuple[str, bool, tuple[tuple[str, bool], ...]]]
) -> tuple[
    list[tuple[int, str, int, tuple[str, ...], int]],
    bool,
    bool,
    tuple[str, str, str, str, int, int, int] | None,
]:
    """Make the plan of a transformer class for a grammar.

    It says for each kind with a method how the method is called, and which positions the
    values get. The native scan asks for it once per class and grammar.
    """
    names = {name for name, _, _ in kinds}
    if getattr(cls, "__fpeg_strict__", False):
        check_strict(cls, names)
    entries: list[tuple[int, str, int, tuple[str, ...], int]] = []
    conventions: dict[tuple[str, bool], tuple[int, tuple[str, ...], int]] = {}
    answering: dict[str, str] = {}
    for kind, (name, token, fields) in enumerate(kinds):
        method = handler(cls, name)
        if method is None:
            continue
        answered(cls, method, name, answering)
        if (method, token) not in conventions:
            if token:
                conventions[(method, token)] = (token_convention(cls, method), (), 0)
            else:
                conventions[(method, token)] = node_convention(cls, method, dict(fields))
        mode, field_names, extras = conventions[(method, token)]
        entries.append((kind, method, mode, field_names, extras))
    positions: Positions | None = getattr(cls, "__fpeg_positions__", None)
    stamp = None
    if positions is not None:
        stamp = (
            positions.start_line,
            positions.start_column,
            positions.end_line,
            positions.end_column,
            positions.line_base,
            positions.column_base,
            COLUMN_UNITS[positions.column_unit],
        )
    return entries, _overrides(cls, "__default__"), _overrides(cls, "__default_token__"), stamp


def check_strict(cls: type, names: Iterable[str]) -> None:
    """Raise `TypeError` for a public method of the class that names no rule or terminal."""
    known = {method_name(name) for name in names}
    known |= {name + "_" for name in known if _taken(cls, name)}
    # An override of a method of the base class names no rule, and needs none.
    reserved: frozenset[str] = getattr(cls, "__fpeg_reserved__", frozenset[str]())
    for name in sorted(_public_methods(cls) - reserved):
        if name not in known:
            close = difflib.get_close_matches(name, known, n=1)
            hint = f", did you mean {close[0]}?" if close else ""
            raise TypeError(
                f"{cls.__qualname__}.{name} names no rule or terminal of the grammar{hint}"
            )


def _public_methods(cls: type) -> set[str]:
    methods: set[str] = set()
    for base in cls.__mro__:
        # The classes of fpeg itself name no rules.
        if base is object or "__fpeg_reserved__" in vars(base):
            continue
        methods.update(n for n, v in vars(base).items() if not n.startswith("_") and _is_method(v))
    return methods


def _is_method(attribute: Any) -> bool:
    # A classmethod object is not callable itself, only once bound.
    return callable(attribute) or isinstance(attribute, classmethod)
