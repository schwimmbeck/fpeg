# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""A transformer from the tree of the Python grammar to Python's `ast`, which makes CPython's
parser the oracle of the grammar.

The tests compare what it gives with what `ast.parse` gives, by `ast.dump` and by the positions
of the nodes, which the Transformer stamps natively.
"""

import ast as python_ast
from pathlib import Path
from typing import Any

import fpeg

# The ast of Python 3.15, which the type checker, set to the oldest Python fpeg runs on, does
# not know whole.
ast: Any = python_ast

GRAMMAR = fpeg.Grammar.open(
    Path(__file__).parent.parent / "crates" / "fpeg" / "grammars" / "python.fpeg"
)

BINARY = {
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
UNARY = {"+": ast.UAdd, "-": ast.USub, "~": ast.Invert}
COMPARE = {
    "==": ast.Eq,
    "!=": ast.NotEq,
    "<": ast.Lt,
    "<=": ast.LtE,
    ">": ast.Gt,
    ">=": ast.GtE,
    "in": ast.In,
    "is": ast.Is,
}


def name_of(value: Any) -> str:
    """The identifier of a NAME, which the transformer gives as a `Name`."""
    return value.id if isinstance(value, ast.Name) else str(value)


def with_context(expr: Any, context: Any) -> Any:
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


def block(statements: list[Any]) -> list[Any]:
    """The statements of a block or a module, where a line of simple statements gives a list of
    them."""
    out: list[Any] = []
    for statement in statements:
        out.extend(statement if isinstance(statement, list) else [statement])
    return out


def spanning(value: Any, first: Any, last: Any) -> Any:
    """Give a value the position from the start of one node to the end of another."""
    value.lineno, value.col_offset = first.lineno, first.col_offset
    value.end_lineno, value.end_col_offset = last.end_lineno, last.end_col_offset
    return value


def parameter_arg(name: Any, annotation: Any) -> Any:
    """A parameter from its name to its annotation. Its node also holds its stars or default."""
    return spanning(ast.arg(name_of(name), annotation), name, annotation or name)


def no_arguments() -> Any:
    return ast.arguments([], [], None, [], [], None, [])


def arguments_of(items: list[Any]) -> Any:
    """The parameters of a function or a lambda, sorted into the lists of `ast.arguments`."""
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
        elif item == "*":
            keyword_only = True
        elif getattr(item, "fpeg_star", 0) == 1:
            vararg = item
            keyword_only = True
        elif getattr(item, "fpeg_star", 0) == 2:
            kwarg = item
        elif keyword_only:
            kwonly.append(item)
            kw_defaults.append(item.fpeg_default)
        else:
            args.append(item)
            if item.fpeg_default is not None:
                defaults.append(item.fpeg_default)
    return ast.arguments(posonly, args, vararg, kwonly, kw_defaults, kwarg, defaults)


class ToAst(fpeg.Transformer, positions="ast"):
    """The tree of the Python grammar as Python's `ast`."""

    # Statements

    def start(self, statements: Any) -> Any:
        return ast.Module(body=block(statements), type_ignores=[])

    def simple_statements(self, items: Any) -> Any:
        return items

    def expression_statement(self, value: Any) -> Any:
        return ast.Expr(value)

    # Parameters and arguments

    def lambda_parameters(self, items: Any) -> Any:
        return arguments_of(items)

    def positional_only(self) -> str:
        return "/"

    def parameter(self, name: Any, annotation: Any, default: Any) -> Any:
        arg = parameter_arg(name, annotation)
        arg.fpeg_default = default
        return arg

    def var_positional(self, name: Any, annotation: Any) -> Any:
        if name is None:
            return "*"
        arg = parameter_arg(name, annotation)
        arg.fpeg_star = 1
        return arg

    def var_keyword(self, name: Any, annotation: Any) -> Any:
        arg = parameter_arg(name, annotation)
        arg.fpeg_star = 2
        return arg

    def arguments(self, items: Any) -> Any:
        keywords = [i for i in items if isinstance(i, ast.keyword)]
        return [i for i in items if not isinstance(i, ast.keyword)], keywords

    def starred(self, value: Any) -> Any:
        return ast.Starred(value, ast.Load())

    def double_starred(self, value: Any) -> Any:
        return ast.keyword(None, value)

    def keyword(self, name: Any, value: Any) -> Any:
        return ast.keyword(name_of(name), value)

    # Expressions

    def tuple_expression(self, items: Any) -> Any:
        return ast.Tuple(items, ast.Load())

    def named_expression(self, target: Any, value: Any) -> Any:
        return ast.NamedExpr(with_context(target, ast.Store), value)

    def yield_from(self, value: Any) -> Any:
        return ast.YieldFrom(value)

    def yield_(self, value: Any) -> Any:
        return ast.Yield(value)

    def lambda_(self, parameters: Any, body: Any) -> Any:
        return ast.Lambda(parameters or no_arguments(), body)

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
        return COMPARE[str(op)]()

    def binary(self, left: Any, op: Any, right: Any, base: Any, exponent: Any) -> Any:
        if base is not None:
            return ast.BinOp(base, ast.Pow(), exponent)
        return ast.BinOp(left, BINARY[str(op)](), right)

    def unary(self, op: Any, operand: Any) -> Any:
        return ast.UnaryOp(UNARY[str(op)](), operand)

    def await_(self, value: Any) -> Any:
        return ast.Await(value)

    def call(self, func: Any, arguments: Any) -> Any:
        if isinstance(arguments, ast.GeneratorExp):
            return ast.Call(func, [arguments], [])
        args, keywords = arguments or ([], [])
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
        return ast.Attribute(value, name_of(attribute), ast.Load())

    def true(self) -> Any:
        return ast.Constant(True)

    def false(self) -> Any:
        return ast.Constant(False)

    def none(self) -> Any:
        return ast.Constant(None)

    def ellipsis(self) -> Any:
        return ast.Constant(Ellipsis)

    def NAME(self, token: fpeg.Token) -> Any:
        return ast.Name(str(token), ast.Load())

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
        target = with_context(targets, ast.Store)
        return ast.comprehension(target, iter, conditions, int(node.field("async") is not None))


def parse(source: str) -> Any:
    """The `ast` of a source as the Python grammar reads it, positions included."""
    return ToAst().transform(GRAMMAR.parse(source))
