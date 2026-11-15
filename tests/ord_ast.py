# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""A transformer from the tree of the ORD grammar to the Python `ast` that ORD runs as.

ORD extends the Python grammar, and this transformer the one of Python: a cell becomes a class
over the `Cell` of the language's runtime, a view generator a function its decorator registers,
and a statement that places an instance or declares a pin, port, net or path a call on the
context of the view, with a `with` around its body.
"""

import ast as python_ast
from pathlib import Path
from typing import Any

import fpeg
from fpeg import python

# The ast of Python 3.15, which the type checker, set to the oldest Python fpeg runs on, does not
# know whole.
ast: Any = python_ast

GRAMMAR_PATH = Path(__file__).parent.parent / "crates" / "fpeg" / "grammars" / "ord.fpeg"
GRAMMAR = fpeg.Grammar.open(GRAMMAR_PATH)

# The pin type and the side a pin faces by default, by the word that declares it.
PINS = {"input": ("In", "West"), "output": ("Out", "East"), "inout": ("Inout", "South")}


def name_of(value: Any) -> str:
    return value.id if isinstance(value, ast.Name) else str(value)


def no_arguments() -> Any:
    return ast.arguments([], [], None, [], [], None, [])


def name(identifier: str) -> Any:
    return ast.Name(identifier, ast.Load())


def attribute(value: Any, attr: str, context: Any = None) -> Any:
    return ast.Attribute(value, attr, context or ast.Load())


def core(attr: str) -> Any:
    """An attribute of the core of the runtime, which a module of ORD finds by this name."""
    return attribute(name("__ordec_core__"), attr)


def view(attr: str) -> Any:
    """An attribute of the context of the view being built, `__ord_context__`."""
    return attribute(name("__ord_context__"), attr)


def call(func: Any, *args: Any, **keywords: Any) -> Any:
    return ast.Call(func, list(args), [ast.keyword(k, v) for k, v in keywords.items()])


def path(target: Any) -> list[Any]:
    """The parts of what a statement places: a constant for each name, a value for each index."""
    if isinstance(target, ast.Name):
        return [ast.Constant(target.id)]
    if isinstance(target, ast.Attribute):
        return [*path(target.value), ast.Constant(target.attr)]
    return [*path(target.value), target.slice]


def stored(target: Any) -> Any:
    """A copy of a target to assign to."""
    copy = ast.Name(target.id, ast.Store()) if isinstance(target, ast.Name) else target
    return copy


class ToAst(python.ToAst):
    """The tree of the ORD grammar as the Python `ast` that ORD runs as."""

    def celldef(self, name: Any, body: Any) -> Any:
        return ast.ClassDef(name_of(name), [core("Cell")], [], body, [], [])

    def viewgen(self, name: Any, parameters: Any, returns: Any, body: Any) -> Any:
        arguments = parameters or no_arguments()
        return ast.FunctionDef(name_of(name), arguments, body, [core("viewgen")], returns, None, [])

    def constrain(self, test: Any) -> Any:
        return ast.Expr(call(view("constrain"), test))

    def placed(self, kind: Any, target: Any, body: Any, meta: Any) -> list[Any]:
        """The statements that place one target: the call that adds it and the `with` around the
        body."""
        kind_name = kind if isinstance(kind, str) else getattr(kind, "id", None)
        parts = ast.Tuple(path(target), ast.Load())
        if kind_name in PINS:
            pin_type, side = PINS[kind_name]
            pin = call(
                core("Pin"),
                pintype=attribute(core("PinType"), pin_type),
                orient=attribute(core("D4"), side),
            )
            value = call(view("add"), parts, pin)
        elif kind_name == "port":
            value = call(view("add_port"), parts)
        elif kind_name in ("net", "path"):
            node = core("Net" if kind_name == "net" else "PathNode")
            value = call(view("add"), parts, call(node))
        else:
            located = {"src_line": ast.Constant(meta.line), "src_column": ast.Constant(meta.column)}
            value = call(view("add_element"), parts, kind, **located)
        # A target of more than one part names what exists already.
        exists = len(parts.elts) > 1
        statement = ast.Expr(value) if exists else ast.Assign([stored(target)], value)
        if body is None:
            return [statement]
        context = ast.withitem(call(attribute(target, "ctx")))
        return [statement, ast.With([context], body)]

    def node_block(self, kind: Any, target: Any, body: Any, *, meta: Any) -> Any:
        return self.placed(kind, target, body, meta)

    def node_line(self, kind: Any, targets: Any, *, meta: Any) -> Any:
        return [s for target in targets for s in self.placed(kind, target, None, meta)]

    def net_block(self, target: Any, body: Any, *, meta: Any) -> Any:
        return self.placed("net", target, body, meta)

    def net_line(self, targets: Any, *, meta: Any) -> Any:
        return [s for target in targets for s in self.placed("net", target, None, meta)]

    def path_block(self, target: Any, body: Any, *, meta: Any) -> Any:
        return self.placed("path", target, body, meta)

    def path_line(self, targets: Any, *, meta: Any) -> Any:
        return [s for target in targets for s in self.placed("path", target, None, meta)]

    def anonymous(self, kind: Any, target: Any, body: Any, meta: Any) -> list[Any]:
        located = {"src_line": ast.Constant(meta.line), "src_column": ast.Constant(meta.column)}
        value = call(view("add_element"), ast.Constant(None), kind, **located)
        statement = ast.Assign([stored(target)], value)
        if body is None:
            return [statement]
        context = ast.withitem(call(attribute(target, "ctx")))
        return [statement, ast.With([context], body)]

    def anonymous_block(self, kind: Any, target: Any, body: Any, *, meta: Any) -> Any:
        return self.anonymous(kind, target, body, meta)

    def anonymous_line(self, kind: Any, targets: Any, *, meta: Any) -> Any:
        return [s for target in targets for s in self.anonymous(kind, target, None, meta)]

    def target_attribute(self, target: Any, name: Any) -> Any:
        return attribute(target, name_of(name))

    def target_item(self, target: Any, slices: Any) -> Any:
        return ast.Subscript(target, slices, ast.Load())

    def getparam(self, value: Any, name: Any) -> Any:
        return attribute(attribute(value, "params"), name_of(name))

    def root_getparam(self, name: Any) -> Any:
        # ORD stores into the parameter of the root, whatever the expression does with it.
        return attribute(attribute(call(view("root")), "params"), name_of(name), ast.Store())

    def root(self, name: Any) -> Any:
        root = call(view("root"))
        if name is None:
            root.fpeg_bare_root = True
            return root
        return attribute(root, name_of(name))

    def assign(self, targets: Any, value: Any) -> Any:
        if len(targets) == 1 and getattr(targets[0], "fpeg_bare_root", False):
            return ast.Expr(call(view("set_root"), value))
        return super().assign(targets, value)

    def SI_NUMBER(self, token: Any) -> Any:
        return call(core("R"), ast.Constant(str(token)))


def parse(source: str) -> Any:
    return ToAst().transform(GRAMMAR.parse(source))
