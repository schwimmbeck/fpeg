// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! Limits stop the reading of a grammar or a parse with an error value, never with a crash.

use fastpeg::grammar::builder::{GrammarBuilder, def, empty, lit};
use fastpeg::grammar::{Expr, ExprKind, Grammar, MAX_EXPRESSION_DEPTH};
use fastpeg::reference::{Limit, Limits, ParseErrorKind, Reference};

/// A stack too small for printing that recursed once per level of a deep tree.
const PRINT_STACK_BYTES: usize = 64 * 1024;

fn nesting() -> Reference {
    let grammar = Grammar::from_text("start: nest\nnest: \"(\" nest \")\" | \"x\"\n")
        .expect("the grammar reads");
    Reference::new(&grammar).expect("the grammar resolves")
}

fn nested(depth: usize) -> String {
    format!("{}x{}", "(".repeat(depth), ")".repeat(depth))
}

/// The messages of reading and resolving a grammar, empty if it resolves.
fn grammar_errors(text: &str) -> Vec<String> {
    let errors = match Grammar::from_text(text) {
        Err(errors) => errors,
        Ok(grammar) => Reference::new(&grammar).err().unwrap_or_default(),
    };
    errors.into_iter().map(|e| e.message).collect()
}

#[test]
fn nesting_deeper_than_the_depth_limit_is_an_error_not_a_stack_overflow() {
    let reference = nesting();
    // Just under the default depth, so the default is shown to fit the interpreter's stack.
    assert!(reference.parse(&nested(1_900), "start").is_ok());
    let error = reference
        .parse(&nested(100_000), "start")
        .expect_err("too deep");
    assert_eq!(error.kind, ParseErrorKind::Limit(Limit::Depth));
}

#[test]
fn the_step_limit_stops_a_parse() {
    let limits = Limits {
        max_steps: 1_000,
        ..Limits::default()
    };
    let error = nesting()
        .parse_with(&nested(500), "start", limits)
        .expect_err("too many steps");
    assert_eq!(error.kind, ParseErrorKind::Limit(Limit::Steps));
}

#[test]
fn grammars_that_nest_too_deeply_are_errors_not_stack_overflows() {
    let suffixes = format!("start: \"a\"{}\n", "?".repeat(5_000));
    let groups = |depth| {
        format!(
            "start: {}\"a\"{}\n",
            "(\"b\" ".repeat(depth),
            ")".repeat(depth)
        )
    };
    let growing = "start: t<\"b\">\nt<x>: \"a\" t<\"c\" x> | x\n";
    // Deep enough for the converter's check, and deep enough for the front end's own limit.
    for text in [&suffixes, &groups(150), &groups(5_000), growing] {
        let errors = grammar_errors(text);
        let shown = text.get(..60).unwrap_or(text);
        assert!(
            errors.iter().any(|e| e.contains("nest")),
            "{shown}: {errors:?}"
        );
    }
}

#[test]
fn templates_whose_arguments_double_stop_after_a_few_instances() {
    let errors = grammar_errors("start: t<\"b\">\nt<x>: \"a\" t<x x> | x\n");
    assert!(
        errors.iter().any(|e| e.contains("too many instances")),
        "{errors:?}"
    );
}

#[test]
fn deeply_nested_rule_bodies_stop_at_the_depth_limit_instead_of_overflowing() {
    // Each activation nests almost as deeply as a rule body may, so matches nest much deeper
    // than rule activations do.
    let optionals = "?".repeat(MAX_EXPRESSION_DEPTH - 10);
    let text = format!("start: r\nr: (\"(\" r \")\"){optionals} \"x\"\n");
    let grammar = Grammar::from_text(&text).expect("the grammar reads");
    let reference = Reference::new(&grammar).expect("the grammar resolves");
    let error = reference
        .parse(&nested(1_500), "start")
        .expect_err("too deep");
    assert_eq!(error.kind, ParseErrorKind::Limit(Limit::Depth));
}

#[test]
fn more_flags_or_capture_names_than_ids_are_errors() {
    let count = usize::from(u16::MAX) + 2;
    let mut flags = GrammarBuilder::new();
    for i in 0..count {
        flags.flag(&format!("f{i}"));
    }
    flags.rule("start", lit("a"));
    let mut captures = GrammarBuilder::new();
    let defs = (0..count).map(|i| def(&format!("c{i}"), empty())).collect();
    captures.rule("start", Expr::new(ExprKind::Sequence(defs)));
    for (builder, message) in [
        (flags, "too many flags"),
        (captures, "too many capture names"),
    ] {
        let errors = Reference::new(&builder.build()).expect_err("too many ids");
        assert!(
            errors.iter().any(|e| e.message.contains(message)),
            "{errors:?}"
        );
    }
}

#[test]
fn a_tree_as_deep_as_its_input_prints_without_recursion() {
    let grammar = Grammar::from_text("start: sum\nsum: sum \"+\" \"1\" | \"1\"\n")
        .expect("the grammar reads");
    let input = format!("1{}", "+1".repeat(500));
    let tree = Reference::new(&grammar)
        .expect("the grammar resolves")
        .parse(&input, "start")
        .expect("the input parses");
    let printed = std::thread::Builder::new()
        .stack_size(PRINT_STACK_BYTES)
        .spawn(move || tree.to_text(&input, false))
        .expect("the thread starts")
        .join()
        .expect("printing finishes");
    assert!(printed.starts_with(&format!("(start {}", "(sum ".repeat(500))));
}
