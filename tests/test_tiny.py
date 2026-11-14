# SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
# SPDX-License-Identifier: MIT OR Apache-2.0

"""The tiny language, run twice: evaluated from its tree, and compiled to LLVM IR.

Two Interpreters walk the tree of a program. `Run` computes its result. `Compile` writes LLVM
IR with llvmlite, which is then run as machine code. Each program in the samples states the
result it must give, and both ways have to give it.

A program computes with 64-bit integers that wrap around. Division rounds towards zero and a
remainder has the sign of its dividend. A comparison and a logical operator give 1 or 0, and
anything but 0 counts as true. The statements outside functions are the program, and its
`return` gives the result. A function sees its parameters and what it declares, a block has
variables of its own. Strings are left out.
"""

import ctypes
import operator
import re
from collections.abc import Callable
from pathlib import Path
from typing import Any, NamedTuple

import fpeg
import pytest

SAMPLES = Path(__file__).parent.parent / "crates" / "fpeg" / "grammars" / "samples" / "tiny"
TINY = fpeg.Grammar.open(SAMPLES.parent.parent / "tiny.fpeg")
RESULT = re.compile(r"// result: (-?\d+)\n")

# Python reserves the names of these rules, so the methods for them carry an underscore and
# get their rule's name after the class.
RESERVED = ("if", "while", "return")

COMPARE: dict[str, Callable[[int, int], bool]] = {
    "==": operator.eq,
    "!=": operator.ne,
    "<": operator.lt,
    "<=": operator.le,
    ">": operator.gt,
    ">=": operator.ge,
}


def programs() -> list[tuple[str, fpeg.Tree, int]]:
    """The samples that state a result, each with its tree and that result."""
    found = []
    for path in sorted(SAMPLES.glob("*.tiny")):
        text = path.read_text(encoding="utf-8")
        stated = RESULT.match(text)
        tree = TINY.parse(text)
        if stated and isinstance(tree, fpeg.Tree):
            found.append((path.name, tree, int(stated[1])))
    return found


def wrap(value: int) -> int:
    """What 64 bits keep of an integer, read as a signed number."""
    return (value + 2**63) % 2**64 - 2**63


def divide(a: int, b: int) -> int:
    """The quotient rounded towards zero, as the machine divides."""
    quotient = abs(a) // abs(b)
    return -quotient if (a < 0) != (b < 0) else quotient


class Returned(NamedTuple):
    """What a statement gives when a `return` ran in it."""

    value: int


class Run(fpeg.Interpreter):
    """Evaluates a program. A statement gives `Returned` to end its function, else `None`."""

    def __init__(self) -> None:
        self.functions: dict[str, fpeg.Tree] = {}
        self.scopes: list[dict[str, int]] = []

    def start(self, items: list[fpeg.Tree]) -> int:
        functions = [item for item in items if item.data == "function"]
        self.functions = {str(function.field("name")): function for function in functions}
        returned = self.run([item for item in items if item.data != "function"])
        return returned.value if returned else 0

    def run(self, statements: list[fpeg.Tree]) -> Returned | None:
        """Run statements with variables of their own, up to a `return`."""
        self.scopes.append({})
        try:
            for statement in statements:
                returned = self.visit(statement)
                if returned is not None:
                    return returned
            return None
        finally:
            self.scopes.pop()

    def scope(self, name: fpeg.Token) -> dict[str, int]:
        """The innermost scope that declares a variable."""
        for scope in reversed(self.scopes):
            if name in scope:
                return scope
        raise NameError(f"{name} is not declared")

    def block(self, statements: list[fpeg.Tree]) -> Returned | None:
        return self.run(statements)

    def let(self, name: fpeg.Token, value: Any) -> None:
        self.scopes[-1][str(name)] = self.visit(value)

    def assign(self, target: fpeg.Token, value: Any) -> None:
        self.scope(target)[str(target)] = self.visit(value)

    def if_(self, test: Any, then: fpeg.Tree, otherwise: fpeg.Tree | None) -> Returned | None:
        if self.visit(test):
            return self.visit(then)
        return None if otherwise is None else self.visit(otherwise)

    def while_(self, test: Any, body: fpeg.Tree) -> Returned | None:
        while self.visit(test):
            returned = self.visit(body)
            if returned is not None:
                return returned
        return None

    def return_(self, value: Any) -> Returned:
        return Returned(0 if value is None else self.visit(value))

    def expression_statement(self, expr: Any) -> None:
        self.visit(expr)

    def binary(self, left: Any, op: str, right: Any) -> int:
        # The right side of `and` and `or` is visited only where it decides.
        if op == "and":
            return int(bool(self.visit(left)) and bool(self.visit(right)))
        if op == "or":
            return int(bool(self.visit(left)) or bool(self.visit(right)))
        a, b = self.visit(left), self.visit(right)
        if op == "+":
            return wrap(a + b)
        if op == "-":
            return wrap(a - b)
        if op == "*":
            return wrap(a * b)
        return wrap(divide(a, b) if op == "/" else a - b * divide(a, b))

    def compare(self, left: Any, op: str, right: Any) -> int:
        return int(COMPARE[op](self.visit(left), self.visit(right)))

    def unary(self, op: str, operand: Any) -> int:
        value = self.visit(operand)
        return int(not value) if op == "not" else wrap(-value)

    def call(self, func: Any, args: list[Any]) -> int:
        function = self.functions[str(func)]
        names = [str(name) for name in function.field("params")]
        values = [self.visit(arg) for arg in args]
        if len(names) != len(values):
            raise TypeError(f"{func} takes {len(names)} arguments")
        # A function sees its parameters, and nothing of where it is called.
        outer, self.scopes = self.scopes, [dict(zip(names, values, strict=True))]
        try:
            returned = self.visit(function.field("body"))
        finally:
            self.scopes = outer
        return returned.value if returned else 0

    def true(self) -> int:
        return 1

    def false(self) -> int:
        return 0

    def NUMBER(self, token: fpeg.Token) -> int:
        return wrap(int(token))

    def NAME(self, token: fpeg.Token) -> int:
        return self.scope(token)[str(token)]


class Compile(fpeg.Interpreter):
    """Writes a program as LLVM IR: a function for each function, and `main` for the program.

    An expression gives the value that holds its result. A variable is a slot on the stack.
    `ir` is the module `llvmlite.ir`, which only the test that compiles needs.
    """

    def __init__(self, ir: Any) -> None:
        self.ir = ir
        self.i64 = ir.IntType(64)
        self.module = ir.Module(name="tiny")
        self.functions: dict[str, Any] = {}
        self.builder: Any = None
        self.scopes: list[dict[str, Any]] = []

    def start(self, items: list[fpeg.Tree]) -> Any:
        functions = [item for item in items if item.data == "function"]
        # Every function is declared before any body, so that each can call each.
        for function in functions:
            takes = self.ir.FunctionType(self.i64, [self.i64] * len(function.field("params")))
            name = str(function.field("name"))
            self.functions[name] = self.ir.Function(self.module, takes, name=f"tiny_{name}")
        for function in functions:
            self.visit(function)
        main = self.ir.Function(self.module, self.ir.FunctionType(self.i64, []), name="main")
        self.fill(main, [], [item for item in items if item.data != "function"])
        return self.module

    def fill(self, function: Any, params: list[str], statements: list[fpeg.Tree]) -> None:
        """Write the body of a function: its parameters as variables, then its statements."""
        self.builder = self.ir.IRBuilder(function.append_basic_block("entry"))
        self.scopes = [{}]
        for name, argument in zip(params, function.args, strict=True):
            self.declare(name, argument)
        self.run(statements)
        if not self.builder.block.is_terminated:
            self.builder.ret(self.i64(0))

    def run(self, statements: list[fpeg.Tree]) -> None:
        """Write statements with variables of their own. What follows a `return` never runs."""
        self.scopes.append({})
        for statement in statements:
            if self.builder.block.is_terminated:
                break
            self.visit(statement)
        self.scopes.pop()

    def declare(self, name: str, value: Any) -> None:
        # Slots are made at the entry of the function, so that a loop does not grow the stack.
        with self.builder.goto_entry_block():
            slot = self.builder.alloca(self.i64, name=name)
        self.builder.store(value, slot)
        self.scopes[-1][name] = slot

    def slot(self, name: fpeg.Token) -> Any:
        for scope in reversed(self.scopes):
            if name in scope:
                return scope[str(name)]
        raise NameError(f"{name} is not declared")

    def truth(self, value: Any) -> Any:
        """One bit: whether a value counts as true."""
        return self.builder.icmp_signed("!=", value, self.i64(0))

    def function(self, name: fpeg.Token, params: list[fpeg.Token], body: fpeg.Tree) -> None:
        self.fill(self.functions[str(name)], [str(param) for param in params], [body])

    def block(self, statements: list[fpeg.Tree]) -> None:
        self.run(statements)

    def let(self, name: fpeg.Token, value: Any) -> None:
        self.declare(str(name), self.visit(value))

    def assign(self, target: fpeg.Token, value: Any) -> None:
        self.builder.store(self.visit(value), self.slot(target))

    def if_(self, test: Any, then: fpeg.Tree, otherwise: fpeg.Tree | None) -> None:
        taken = self.builder.append_basic_block("then")
        other = self.builder.append_basic_block("else")
        end = self.builder.append_basic_block("end")
        self.builder.cbranch(self.truth(self.visit(test)), taken, other)
        for block, branch in ((taken, then), (other, otherwise)):
            self.builder.position_at_end(block)
            if branch is not None:
                self.visit(branch)
            if not self.builder.block.is_terminated:
                self.builder.branch(end)
        self.builder.position_at_end(end)

    def while_(self, test: Any, body: fpeg.Tree) -> None:
        again = self.builder.append_basic_block("test")
        inside = self.builder.append_basic_block("body")
        end = self.builder.append_basic_block("end")
        self.builder.branch(again)
        self.builder.position_at_end(again)
        self.builder.cbranch(self.truth(self.visit(test)), inside, end)
        self.builder.position_at_end(inside)
        self.visit(body)
        if not self.builder.block.is_terminated:
            self.builder.branch(again)
        self.builder.position_at_end(end)

    def return_(self, value: Any) -> None:
        self.builder.ret(self.i64(0) if value is None else self.visit(value))

    def expression_statement(self, expr: Any) -> None:
        self.visit(expr)

    def binary(self, left: Any, op: str, right: Any) -> Any:
        if op in ("and", "or"):
            return self.logical(left, op == "and", right)
        a, b = self.visit(left), self.visit(right)
        write = {
            "+": self.builder.add,
            "-": self.builder.sub,
            "*": self.builder.mul,
            "/": self.builder.sdiv,
            "%": self.builder.srem,
        }
        return write[str(op)](a, b)

    def logical(self, left: Any, both: bool, right: Any) -> Any:
        """Write `and` or `or`: the right side runs only where the left does not decide."""
        first = self.truth(self.visit(left))
        decided = self.builder.block
        more = self.builder.append_basic_block("right")
        end = self.builder.append_basic_block("end")
        if both:
            self.builder.cbranch(first, more, end)
        else:
            self.builder.cbranch(first, end, more)
        self.builder.position_at_end(more)
        second = self.truth(self.visit(right))
        computed = self.builder.block
        self.builder.branch(end)
        self.builder.position_at_end(end)
        result = self.builder.phi(self.ir.IntType(1))
        result.add_incoming(first, decided)
        result.add_incoming(second, computed)
        return self.builder.zext(result, self.i64)

    def compare(self, left: Any, op: str, right: Any) -> Any:
        a, b = self.visit(left), self.visit(right)
        return self.builder.zext(self.builder.icmp_signed(str(op), a, b), self.i64)

    def unary(self, op: str, operand: Any) -> Any:
        value = self.visit(operand)
        if op == "not":
            return self.builder.zext(self.builder.icmp_signed("==", value, self.i64(0)), self.i64)
        return self.builder.neg(value)

    def call(self, func: Any, args: list[Any]) -> Any:
        function = self.functions[str(func)]
        if len(function.args) != len(args):
            raise TypeError(f"{func} takes {len(function.args)} arguments")
        return self.builder.call(function, [self.visit(arg) for arg in args])

    def true(self) -> Any:
        return self.i64(1)

    def false(self) -> Any:
        return self.i64(0)

    def NUMBER(self, token: fpeg.Token) -> Any:
        return self.i64(wrap(int(token)))

    def NAME(self, token: fpeg.Token) -> Any:
        return self.builder.load(self.slot(token))


for interpreter in (Run, Compile):
    for reserved in RESERVED:
        setattr(interpreter, reserved, getattr(interpreter, f"{reserved}_"))


def run_compiled(binding: Any, module: Any) -> int:
    """Check a module of IR, compile it to machine code and call its `main`."""
    binding.initialize_native_target()
    binding.initialize_native_asmprinter()
    parsed = binding.parse_assembly(str(module))
    parsed.verify()
    machine = binding.Target.from_default_triple().create_target_machine()
    with binding.create_mcjit_compiler(parsed, machine) as engine:
        engine.finalize_object()
        main = ctypes.CFUNCTYPE(ctypes.c_int64)(engine.get_function_address("main"))
        return main()


PROGRAMS = programs()
each_program = pytest.mark.parametrize(
    ("tree", "expected"),
    [(tree, expected) for _, tree, expected in PROGRAMS],
    ids=[name for name, _, _ in PROGRAMS],
)


def test_the_samples_hold_programs_that_state_their_results() -> None:
    assert len(PROGRAMS) >= 7, [name for name, _, _ in PROGRAMS]


@each_program
def test_an_evaluated_program_gives_the_result_it_states(tree: fpeg.Tree, expected: int) -> None:
    assert Run().visit(tree) == expected


@each_program
def test_a_compiled_program_gives_the_result_it_states(tree: fpeg.Tree, expected: int) -> None:
    ir = pytest.importorskip("llvmlite.ir")
    binding = pytest.importorskip("llvmlite.binding")
    assert run_compiled(binding, Compile(ir).visit(tree)) == expected
