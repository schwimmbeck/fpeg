# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""Python's own `ast` from the Python grammar that comes with fpeg, positions included.

`parse(source)` gives what `ast.parse(source)` gives, which the tests check over CPython's
library. `ToAst` is the transformer behind it, to extend together with the grammar: a grammar
that starts with `%extends "python"` and a subclass of `ToAst` with methods for its new rules.
Both build the `ast` of Python 3.15 and need it.
"""

import ast as python_ast
import sys
import unicodedata
from typing import Any, cast

import fpeg

# The ast of Python 3.15, which the type checker, set to the oldest Python fpeg runs on, does
# not know whole.
ast: Any = python_ast

__all__ = ["GRAMMAR", "ToAst", "ToAstBase", "parse"]

#: The Python grammar that comes with fpeg, Python 3.15.
GRAMMAR = fpeg.Grammar('%extends "python"')

# The Python whose ast holds every node the transformer makes. An older one lacks some.
_AST_PYTHON = (3, 15)

_BINARY = {
    "+": ast.Add,
    "-": ast.Sub,
    "*": ast.Mult,
    "/": ast.Div,
    "//": ast.FloorDiv,
    "%": ast.Mod,
    "**": ast.Pow,
    "<<": ast.LShift,
    ">>": ast.RShift,
    "&": ast.BitAnd,
    "|": ast.BitOr,
    "^": ast.BitXor,
    "@": ast.MatMult,
}
_AUGMENTED = {op + "=": kind for op, kind in _BINARY.items()}
_UNARY = {"+": ast.UAdd, "-": ast.USub, "~": ast.Invert}
_COMPARE = {
    "==": ast.Eq,
    "!=": ast.NotEq,
    "<": ast.Lt,
    "<=": ast.LtE,
    ">": ast.Gt,
    ">=": ast.GtE,
    "in": ast.In,
    "is": ast.Is,
}
_SIMPLE_ESCAPES = {
    "\\": "\\",
    "'": "'",
    '"': '"',
    "a": "\a",
    "b": "\b",
    "f": "\f",
    "n": "\n",
    "r": "\r",
    "t": "\t",
    "v": "\v",
}
# The hex digits that follow each letter of an escape by code.
_HEX_ESCAPES = {"x": 2, "u": 4, "U": 8}


def _name_of(value: Any) -> str:
    """Return the identifier of a NAME, which the transformer gives as a `Name`."""
    return value.id if isinstance(value, ast.Name) else str(value)


def _with_context(expr: Any, context: Any) -> Any:
    """Give a target and the targets inside it the context of a store or a deletion."""
    todo = [expr]
    while todo:
        node = todo.pop()
        if isinstance(node, (ast.Name, ast.Attribute, ast.Subscript)):
            node.ctx = context()
        elif isinstance(node, ast.Starred):
            node.ctx = context()
            todo.append(node.value)
        elif isinstance(node, (ast.Tuple, ast.List)):
            node.ctx = context()
            todo.extend(node.elts)
    return expr


def _flat(statements: list[Any]) -> list[Any]:
    """Return the statements with each list among them, at any depth, in its place.

    A line of simple statements gives a list of them, and the method of a subclass may give a
    list for one statement.
    """
    out: list[Any] = []
    pending = statements[::-1]
    while pending:
        statement = pending.pop()
        if isinstance(statement, list):
            pending.extend(cast(list[Any], statement)[::-1])
        else:
            out.append(statement)
    return out


def _block(statements: list[Any]) -> list[Any]:
    """Return the statements of a block or a module, each compound one ending with its last."""
    out = _flat(statements)
    for statement in out:
        _statement_end(statement)
    return out


def _statement_end(statement: Any) -> tuple[int | None, int | None]:
    """Return where a statement ends, a compound one where its last statement ends.

    The end of a compound statement is set here. The node of a block also holds the line
    breaks after that statement.
    """
    for name in ("finalbody", "orelse", "handlers", "cases", "body"):
        items = getattr(statement, name, None)
        if items:
            end = _statement_end(items[-1])
            if "end_lineno" not in statement._attributes:
                return end
            # A statement that a method built inside has no end, and leaves the stamped one.
            if end[0] is not None:
                statement.end_lineno, statement.end_col_offset = end
            break
    return statement.end_lineno, statement.end_col_offset


def _spanning(value: Any, first: Any, last: Any) -> Any:
    """Give a value the position from the start of one node to the end of another."""
    value.lineno, value.col_offset = first.lineno, first.col_offset
    value.end_lineno, value.end_col_offset = last.end_lineno, last.end_col_offset
    return value


def _parameter_arg(name: Any, annotation: Any) -> Any:
    """Return a parameter from its name to its annotation.

    Its node also holds its stars or default.
    """
    return _spanning(ast.arg(_name_of(name), annotation), name, annotation or name)


def _no_arguments() -> Any:
    return ast.arguments([], [], None, [], [], None, [])


def _arguments_of(items: list[Any]) -> Any:
    """Sort the parameters of a function or a lambda into the lists of `ast.arguments`."""
    posonly: list[Any] = []
    args: list[Any] = []
    defaults: list[Any] = []
    vararg = kwarg = None
    kwonly: list[Any] = []
    kw_defaults: list[Any] = []
    keyword_only = False
    for item in items:
        if item == "/":
            posonly, args = args, []
            continue
        if item == "*":
            keyword_only = True
            continue
        stars, default = _take(item, "fpeg_star", 0), _take(item, "fpeg_default")
        if stars == 1:
            vararg = item
            keyword_only = True
        elif stars == 2:
            kwarg = item
        elif keyword_only:
            kwonly.append(item)
            kw_defaults.append(default)
        else:
            args.append(item)
            if default is not None:
                defaults.append(default)
    return ast.arguments(posonly, args, vararg, kwonly, kw_defaults, kwarg, defaults)


def _take(node: Any, marker: str, default: Any = None) -> Any:
    """Return a marker that one method left on a node for another, and remove it."""
    value = getattr(node, marker, default)
    if hasattr(node, marker):
        delattr(node, marker)
    return value


def _dotted(parts: list[Any]) -> Any:
    """Names joined by dots as attributes of the first, each spanning the names up to it."""
    value = parts[0]
    for part in parts[1:]:
        value = _spanning(ast.Attribute(value, _name_of(part), ast.Load()), value, part)
    return value


def _line_feeds(text: str) -> str:
    """Source text with each line break as one line feed, as CPython reads it."""
    return text.replace("\r\n", "\n").replace("\r", "\n")


def _decode(text: str, raw: bool, braces: bool) -> str:
    """Return the value of the text of a string part.

    Escapes are taken where it is not raw, and doubled braces made single in an f-string or
    t-string.
    """
    out: list[str] = []
    i = 0
    while i < len(text):
        c = text[i]
        if braces and c in "{}" and text.startswith(c * 2, i):
            out.append(c)
            i += 2
            continue
        if c != "\\" or raw:
            out.append(c)
            i += 1
            continue
        nxt = text[i + 1 : i + 2]
        if nxt in _SIMPLE_ESCAPES:
            out.append(_SIMPLE_ESCAPES[nxt])
            i += 2
        elif nxt == "\r":
            i += 3 if text.startswith("\r\n", i + 1) else 2
        elif nxt == "\n":
            i += 2
        elif "0" <= nxt <= "7":
            j = i + 1
            while j < len(text) and j < i + 4 and "0" <= text[j] <= "7":
                j += 1
            out.append(chr(int(text[i + 1 : j], 8)))
            i = j
        elif nxt in _HEX_ESCAPES or (nxt == "N" and text.startswith("{", i + 2)):
            # An escape that Python cannot decode is a syntax error, as in a string without `f`.
            try:
                if nxt == "N":
                    end = text.index("}", i + 3)
                    out.append(unicodedata.lookup(text[i + 3 : end]))
                    i = end + 1
                else:
                    size = _HEX_ESCAPES[nxt]
                    out.append(chr(int(text[i + 2 : i + 2 + size], 16)))
                    i += 2 + size
            except (KeyError, ValueError) as error:
                raise SyntaxError(f"(unicode error) {error}") from None
        else:
            out.append("\\")
            i += 1
    return "".join(out)


def _finished(pieces: list[Any], raw: bool, braces: bool) -> list[Any]:
    """Return the values of an f-string, a t-string or a format spec.

    The texts are decoded, the format specs of fields finished, the text of a field with "="
    put before the field, and adjacent texts joined.
    """
    out: list[Any] = []
    for piece in pieces:
        if isinstance(piece, ast.Constant):
            piece.value = _decode(_line_feeds(piece.value), raw, braces)
            out.append(piece)
            continue
        if piece.format_spec is not None:
            piece.format_spec.values = _finished(piece.format_spec.values, raw, False)
        debug = _take(piece, "fpeg_debug")
        if not braces:
            # A field inside a format spec stays a field, in a t-string too.
            _take(piece, "fpeg_source")
        if debug is not None:
            out.append(_text_at(debug, piece.lineno, piece.col_offset + 1))
        out.append(piece)
    return _joined(out)


def _joined(pieces: list[Any]) -> list[Any]:
    """Join each run of adjacent texts into one constant that spans the run.

    Empty texts are left out.
    """
    out: list[Any] = []
    run: list[Any] = []
    for piece in [*pieces, None]:
        if isinstance(piece, ast.Constant):
            run.append(piece)
            continue
        text = "".join(p.value for p in run)
        if text:
            out.append(_spanning(ast.Constant(text), run[0], run[-1]))
        run = []
        if piece is not None:
            out.append(piece)
    return out


def _text_at(text: str, lineno: int, col_offset: int) -> Any:
    """Return a constant of source text that starts at a position and ends with the text."""
    lines = text.split("\n")
    end = len(lines[-1].encode()) + (col_offset if len(lines) == 1 else 0)
    constant = ast.Constant(text)
    constant.lineno, constant.col_offset = lineno, col_offset
    constant.end_lineno, constant.end_col_offset = lineno + len(lines) - 1, end
    return constant


def _field_source(node: Any, ends: tuple[str, ...]) -> str:
    """Return the text of a replacement field from after its brace.

    It ends where the first of the fields `ends` or the closing brace starts.
    """
    text = node.text[1:]
    end = len(text) - 1
    for name in ends:
        item = node.field(name)
        if item is not None:
            start = item.start_pos if isinstance(item, fpeg.Token) else item.meta.start_pos
            end = min(end, start - node.meta.start_pos - 1)
    return _line_feeds(text[:end])


def _interpolation(value: Any) -> Any:
    """Turn a replacement field of a t-string into an `Interpolation` with its source text."""
    if not isinstance(value, ast.FormattedValue):
        return value
    source = _take(value, "fpeg_source")
    out = ast.Interpolation(value.value, source, value.conversion, value.format_spec)
    return ast.copy_location(out, value)


class ToAstBase(fpeg.Transformer):
    """The methods that turn the tree of the Python grammar into Python's `ast`.

    They read the positions of the values they get, which a subclass gives them. They build
    the `ast` of Python 3.15 and need it, so that an older Python raises `RuntimeError`.
    """

    def __init__(self, visit_tokens: bool = True) -> None:
        """Make a transformer, or raise `RuntimeError` on a Python older than 3.15."""
        if sys.version_info < _AST_PYTHON:
            running = f"{sys.version_info.major}.{sys.version_info.minor}"
            message = f"fpeg.python builds the ast of Python 3.15 and needs it, not {running}"
            raise RuntimeError(message)
        super().__init__(visit_tokens)

    # Statements

    def start(self, statements: Any) -> Any:
        return ast.Module(body=_block(statements), type_ignores=[])

    def simple_statements(self, items: Any) -> Any:
        return _flat(items)

    def indented_block(self, statements: Any) -> Any:
        return _block(statements)

    def annotated_assign(self, target: Any, annotation: Any, value: Any, *, node: Any) -> Any:
        written = node.field("target")
        simple = int(isinstance(target, ast.Name) and not isinstance(written, fpeg.Tree))
        return ast.AnnAssign(_with_context(target, ast.Store), annotation, value, simple)

    def assign(self, targets: Any, value: Any) -> Any:
        return ast.Assign([_with_context(t, ast.Store) for t in targets], value)

    def augmented_assign(self, target: Any, op: Any, value: Any) -> Any:
        return ast.AugAssign(_with_context(target, ast.Store), _AUGMENTED[str(op)](), value)

    def type_alias(self, name: Any, params: Any, value: Any) -> Any:
        return ast.TypeAlias(_with_context(name, ast.Store), params or [], value)

    def return_(self, value: Any) -> Any:
        return ast.Return(value)

    def raise_(self, exception: Any, cause: Any) -> Any:
        return ast.Raise(exception, cause)

    def pass_(self) -> Any:
        return ast.Pass()

    def break_(self) -> Any:
        return ast.Break()

    def continue_(self) -> Any:
        return ast.Continue()

    def del_(self, targets: Any, *, node: Any) -> Any:
        bare = getattr(node.field("targets"), "data", None) == "tuple_expression"
        items = targets.elts if bare else [targets]
        return ast.Delete([_with_context(t, ast.Del) for t in items])

    def yield_statement(self, value: Any) -> Any:
        return ast.Expr(value)

    def assert_(self, test: Any, message: Any) -> Any:
        return ast.Assert(test, message)

    def global_(self, names: Any) -> Any:
        return ast.Global([_name_of(n) for n in names])

    def nonlocal_(self, names: Any) -> Any:
        return ast.Nonlocal([_name_of(n) for n in names])

    def expression_statement(self, value: Any) -> Any:
        return ast.Expr(value)

    def import_name(self, lazy: Any, names: Any) -> Any:
        return ast.Import(names, is_lazy=int(lazy is not None))

    def dotted_as_name(self, name: str, alias: Any) -> Any:
        return ast.alias(name, None if alias is None else _name_of(alias))

    def dotted_name(self, parts: Any) -> str:
        return ".".join(_name_of(p) for p in parts)

    def import_from(self, lazy: Any, levels: Any, module: Any, names: Any) -> Any:
        level = sum(len(str(dots)) for dots in levels)
        listed = cast(list[Any], names) if isinstance(names, list) else [names]
        return ast.ImportFrom(module, listed, level, is_lazy=int(lazy is not None))

    def import_all(self) -> Any:
        return ast.alias("*")

    def import_list(self, items: Any) -> Any:
        return items

    def import_as_name(self, name: Any, alias: Any) -> Any:
        return ast.alias(_name_of(name), None if alias is None else _name_of(alias))

    def decorated(self, decorators: Any, definition: Any) -> Any:
        if isinstance(definition, list):
            raise TypeError("a decorated definition is one statement, not a list of them")
        # Before any that the method of a subclass gave the definition, which stay last.
        definition.decorator_list = [*decorators, *definition.decorator_list]
        return definition

    def decorator(self, value: Any) -> Any:
        return value

    def function_def(
        self, name: Any, params: Any, parameters: Any, returns: Any, body: Any, *, node: Any
    ) -> Any:
        # The field async names no parameter, so the node tells.
        kind = ast.AsyncFunctionDef if node.field("async") is not None else ast.FunctionDef
        arguments = parameters or _no_arguments()
        return kind(_name_of(name), arguments, body, [], returns, None, params or [])

    def class_def(self, name: Any, params: Any, bases: Any, body: Any) -> Any:
        none: tuple[list[Any], list[Any]] = ([], [])
        args, keywords = bases or none
        return ast.ClassDef(_name_of(name), args, keywords, body, [], params or [])

    def if_(self, test: Any, body: Any, branches: Any, orelse: Any) -> Any:
        rest: list[Any] = orelse or []
        for branch in reversed(branches):
            branch.orelse = rest
            rest = [branch]
        return ast.If(test, body, rest)

    def elif_(self, test: Any, body: Any) -> Any:
        return ast.If(test, body, [])

    def while_(self, test: Any, body: Any, orelse: Any) -> Any:
        return ast.While(test, body, orelse or [])

    def for_(self, targets: Any, iter: Any, body: Any, orelse: Any, *, node: Any) -> Any:
        kind = ast.AsyncFor if node.field("async") is not None else ast.For
        return kind(_with_context(targets, ast.Store), iter, body, orelse or [])

    def with_(self, items: Any, body: Any, *, node: Any) -> Any:
        kind = ast.AsyncWith if node.field("async") is not None else ast.With
        return kind(items, body)

    def with_list(self, items: Any) -> Any:
        return items

    def with_item(self, context: Any, target: Any) -> Any:
        return ast.withitem(context, None if target is None else _with_context(target, ast.Store))

    def try_(self, body: Any, handlers: Any, orelse: Any, final: Any, *, node: Any) -> Any:
        star = any(getattr(h, "data", None) == "except_star" for h in node.field("handlers"))
        kind = ast.TryStar if star else ast.Try
        for handler in handlers:
            _statement_end(handler)
        return kind(body, handlers, orelse or [], final or [])

    def except_(self, types: Any, name: Any, body: Any) -> Any:
        return ast.ExceptHandler(types, None if name is None else _name_of(name), body)

    def except_star(self, types: Any, name: Any, body: Any) -> Any:
        return ast.ExceptHandler(types, None if name is None else _name_of(name), body)

    def match(self, subject: Any, cases: Any) -> Any:
        return ast.Match(subject, cases)

    def case(self, pattern: Any, guard: Any, body: Any) -> Any:
        return ast.match_case(pattern, guard, body)

    # Patterns

    def sequence_pattern(self, items: Any) -> Any:
        return ast.MatchSequence(items)

    def as_pattern(self, pattern: Any, target: Any) -> Any:
        return ast.MatchAs(pattern, _name_of(target))

    def or_pattern(self, alternatives: Any) -> Any:
        return ast.MatchOr(alternatives)

    def literal_pattern(self, value: Any) -> Any:
        if isinstance(value, fpeg.Token):
            return ast.MatchSingleton({"None": None, "True": True, "False": False}[str(value)])
        return ast.MatchValue(value)

    def complex_number(self, real: Any, op: Any, imaginary: Any) -> Any:
        return ast.BinOp(real, _BINARY[str(op)](), imaginary)

    def signed_number(self, sign: Any, number: Any) -> Any:
        if sign is None:
            return number
        return ast.UnaryOp(_UNARY[str(sign)](), number)

    def class_pattern(self, children: Any) -> Any:
        # The field class names no parameter, and comes first.
        class_, arguments = children[0], children[1:]
        positional = [a for a in arguments if not isinstance(a, tuple)]
        keywords = [cast(tuple[Any, Any], a) for a in arguments if isinstance(a, tuple)]
        return ast.MatchClass(
            class_, positional, [k for k, _ in keywords], [p for _, p in keywords]
        )

    def keyword_pattern(self, name: Any, pattern: Any) -> Any:
        return (_name_of(name), pattern)

    def value_pattern(self, parts: Any) -> Any:
        return ast.MatchValue(_dotted(parts))

    def wildcard_pattern(self) -> Any:
        return ast.MatchAs()

    def capture_pattern(self, name: Any) -> Any:
        return ast.MatchAs(name=_name_of(name))

    def group_pattern(self, pattern: Any) -> Any:
        return pattern

    def star_pattern(self, target: Any) -> Any:
        name = _name_of(target)
        return ast.MatchStar(None if name == "_" else name)

    def mapping_pattern(self, items: Any) -> Any:
        pairs = [cast(tuple[Any, Any], i) for i in items if isinstance(i, tuple)]
        rest = next((i for i in items if isinstance(i, str)), None)
        return ast.MatchMapping([k for k, _ in pairs], [p for _, p in pairs], rest)

    def key_pattern(self, key: Any, pattern: Any) -> Any:
        if isinstance(key, ast.MatchSingleton):
            value = ast.Constant(key.value)
            ast.copy_location(value, key)
            return (value, pattern)
        return (key.value, pattern)

    def double_star_pattern(self, target: Any) -> str:
        return _name_of(target)

    def pattern_name(self, parts: Any) -> Any:
        return _dotted(parts)

    # Parameters and arguments

    def parameters(self, items: Any) -> Any:
        return _arguments_of(items)

    def lambda_parameters(self, items: Any) -> Any:
        return _arguments_of(items)

    def positional_only(self) -> str:
        return "/"

    def parameter(self, name: Any, annotation: Any, default: Any) -> Any:
        arg = _parameter_arg(name, annotation)
        arg.fpeg_default = default
        return arg

    def var_positional(self, name: Any, annotation: Any) -> Any:
        if name is None:
            return "*"
        arg = _parameter_arg(name, annotation)
        arg.fpeg_star = 1
        return arg

    def var_keyword(self, name: Any, annotation: Any) -> Any:
        arg = _parameter_arg(name, annotation)
        arg.fpeg_star = 2
        return arg

    def type_params(self, items: Any) -> Any:
        return items

    def param_spec(self, name: Any, default: Any) -> Any:
        return ast.ParamSpec(_name_of(name), default)

    def type_var_tuple(self, name: Any, default: Any) -> Any:
        return ast.TypeVarTuple(_name_of(name), default)

    def type_var(self, name: Any, bound: Any, default: Any) -> Any:
        return ast.TypeVar(_name_of(name), bound, default)

    def arguments(self, items: Any) -> Any:
        keywords = [i for i in items if isinstance(i, ast.keyword)]
        return [i for i in items if not isinstance(i, ast.keyword)], keywords

    def starred(self, value: Any) -> Any:
        return ast.Starred(value, ast.Load())

    def double_starred(self, value: Any) -> Any:
        return ast.keyword(None, value)

    def keyword(self, name: Any, value: Any) -> Any:
        return ast.keyword(_name_of(name), value)

    # Expressions

    def tuple_expression(self, items: Any) -> Any:
        return ast.Tuple(items, ast.Load())

    def named_expression(self, target: Any, value: Any) -> Any:
        return ast.NamedExpr(_with_context(target, ast.Store), value)

    def yield_from(self, value: Any) -> Any:
        return ast.YieldFrom(value)

    def yield_(self, value: Any) -> Any:
        return ast.Yield(value)

    def lambda_(self, parameters: Any, body: Any) -> Any:
        return ast.Lambda(parameters or _no_arguments(), body)

    def if_expression(self, body: Any, test: Any, orelse: Any) -> Any:
        return ast.IfExp(test, body, orelse)

    def bool_or(self, values: Any) -> Any:
        return ast.BoolOp(ast.Or(), values)

    def bool_and(self, values: Any) -> Any:
        return ast.BoolOp(ast.And(), values)

    def not_(self, operand: Any) -> Any:
        return ast.UnaryOp(ast.Not(), operand)

    def compare(self, left: Any, operators: Any, comparators: Any) -> Any:
        return ast.Compare(left, operators, comparators)

    def not_in(self) -> Any:
        return ast.NotIn()

    def is_not(self) -> Any:
        return ast.IsNot()

    def operator(self, op: Any) -> Any:
        return _COMPARE[str(op)]()

    def binary(self, left: Any, op: Any, right: Any, base: Any, exponent: Any) -> Any:
        if base is not None:
            return ast.BinOp(base, ast.Pow(), exponent)
        return ast.BinOp(left, _BINARY[str(op)](), right)

    def unary(self, op: Any, operand: Any) -> Any:
        return ast.UnaryOp(_UNARY[str(op)](), operand)

    def await_(self, value: Any) -> Any:
        return ast.Await(value)

    def call(self, func: Any, arguments: Any) -> Any:
        if isinstance(arguments, ast.GeneratorExp):
            return ast.Call(func, [arguments], [])
        none: tuple[list[Any], list[Any]] = ([], [])
        args, keywords = arguments or none
        return ast.Call(func, args, keywords)

    def subscript(self, value: Any, slices: Any) -> Any:
        return ast.Subscript(value, slices, ast.Load())

    def slices(self, items: Any, *, node: Any) -> Any:
        if len(items) == 1 and not node.text.rstrip().endswith(","):
            return items[0]
        return ast.Tuple(items, ast.Load())

    def slice(self, lower: Any, upper: Any, step: Any) -> Any:
        return ast.Slice(lower, upper, step)

    def attribute(self, value: Any, attribute: Any) -> Any:
        return ast.Attribute(value, _name_of(attribute), ast.Load())

    def true(self) -> Any:
        return ast.Constant(True)

    def false(self) -> Any:
        return ast.Constant(False)

    def none(self) -> Any:
        return ast.Constant(None)

    def ellipsis(self) -> Any:
        return ast.Constant(Ellipsis)

    def NAME(self, token: fpeg.Token) -> Any:
        # Python reads identifiers in NFKC.
        return ast.Name(unicodedata.normalize("NFKC", token), ast.Load())

    def NUMBER(self, token: fpeg.Token) -> Any:
        return ast.Constant(ast.literal_eval(str(token)))

    def tuple(self, items: Any) -> Any:
        return ast.Tuple(items, ast.Load())

    def group(self, value: Any) -> Any:
        return value

    def generator_expression(self, element: Any, clauses: Any) -> Any:
        return ast.GeneratorExp(element, clauses)

    def list(self, items: Any) -> Any:
        return ast.List(items, ast.Load())

    def list_comprehension(self, element: Any, clauses: Any) -> Any:
        return ast.ListComp(element, clauses)

    def dict(self, items: Any) -> Any:
        keys = [None if isinstance(i, ast.keyword) else i[0] for i in items]
        values = [i.value if isinstance(i, ast.keyword) else i[1] for i in items]
        return ast.Dict(keys, values)

    def pair(self, key: Any, value: Any) -> Any:
        return (key, value)

    def set(self, items: Any) -> Any:
        return ast.Set(items)

    def dict_comprehension(self, key: Any, value: Any, element: Any, clauses: Any) -> Any:
        if element is not None:
            return ast.DictComp(element.value, None, clauses)
        return ast.DictComp(key, value, clauses)

    def set_comprehension(self, element: Any, clauses: Any) -> Any:
        return ast.SetComp(element, clauses)

    def comprehension(self, clauses: Any) -> Any:
        return clauses

    def for_if(self, targets: Any, iter: Any, conditions: Any, *, node: Any) -> Any:
        target = _with_context(targets, ast.Store)
        return ast.comprehension(target, iter, conditions, int(node.field("async") is not None))

    # Strings

    def STRING(self, token: fpeg.Token) -> Any:
        return ast.Constant(ast.literal_eval(str(token)))

    def BYTES(self, token: fpeg.Token) -> Any:
        return ast.literal_eval(str(token))

    def strings(self, parts: Any, *, node: Any) -> Any:
        if isinstance(parts[0], bytes):
            return ast.Constant(b"".join(parts))
        if all(isinstance(p, ast.Constant) for p in parts):
            kind = "u" if node.text[:1] in "uU" else None
            return ast.Constant("".join(p.value for p in parts), kind)
        kind = ast.TemplateStr if isinstance(parts[0], ast.TemplateStr) else ast.JoinedStr
        return kind(_joined([v for p in parts for v in (p.values if isinstance(p, kind) else [p])]))

    # The f-string or t-string decodes the texts in it, since it knows whether it is raw.

    def F_TEXT(self, token: fpeg.Token) -> Any:
        return ast.Constant(str(token))

    def F_SPEC_TEXT(self, token: fpeg.Token) -> Any:
        return ast.Constant(str(token))

    def fstring(self, prefix: Any, parts: Any) -> Any:
        values = _finished(parts, "r" in str(prefix).lower(), True)
        if "t" in str(prefix).lower():
            return ast.TemplateStr([_interpolation(v) for v in values])
        for value in values:
            _take(value, "fpeg_source")
        return ast.JoinedStr(values)

    def replacement_field(
        self, value: Any, debug: Any, conversion: Any, format: Any, *, node: Any
    ) -> Any:
        code = ord(conversion) if conversion is not None else -1
        if debug is not None and conversion is None and format is None:
            code = ord("r")
        formatted = ast.FormattedValue(value, code, format)
        # An interpolation keeps the text of its expression, and "=" puts it before the field.
        formatted.fpeg_source = _field_source(node, ("debug", "conversion", "format"))
        if debug is not None:
            formatted.fpeg_debug = _field_source(node, ("conversion", "format"))
        return formatted

    def conversion(self, name: Any) -> str:
        return str(name)

    def format_spec(self, parts: Any) -> Any:
        return ast.JoinedStr(parts)


class ToAst(ToAstBase, positions="ast"):
    """The tree of the Python grammar as Python's `ast`, with positions stamped natively."""


def parse(
    source: str,
    *,
    max_steps: int | None = None,
    max_depth: int | None = None,
    max_memory: int | None = None,
) -> Any:
    """Return the `ast` of a source as the Python grammar reads it, positions included.

    The limits are those of `Grammar.parse`. It needs Python 3.15, whose `ast` it builds, and
    raises `RuntimeError` on an older one.
    """
    tree = GRAMMAR.parse(source, max_steps=max_steps, max_depth=max_depth, max_memory=max_memory)
    return ToAst().transform(tree)
