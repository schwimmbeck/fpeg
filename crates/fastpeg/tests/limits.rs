// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Limits stop the reading of a grammar or a parse with an error value, never with a crash.

use fastpeg::grammar::builder::{GrammarBuilder, def, empty, field, lit, on, r, r_args};
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
fn the_step_limit_stops_a_parse_also_inside_a_lookahead() {
    let limits = Limits {
        max_steps: 1_000,
        ..Limits::default()
    };
    let error = nesting()
        .parse_with(&nested(500), "start", limits)
        .expect_err("too many steps");
    assert_eq!(error.kind, ParseErrorKind::Limit(Limit::Steps));
    // A limit is no failure that a lookahead turns into success or failure.
    let looking = Grammar::from_text("start: !nest nest\nnest: \"(\" nest \")\" | \"x\"\n")
        .expect("the grammar reads");
    let error = Reference::new(&looking)
        .expect("the grammar resolves")
        .parse_with(&nested(500), "start", limits)
        .expect_err("too many steps");
    assert_eq!(error.kind, ParseErrorKind::Limit(Limit::Steps));
}

#[test]
fn scanning_text_counts_against_the_step_limit() {
    let limits = Limits {
        max_steps: 2_000_000,
        ..Limits::default()
    };
    let compared = format!(
        "{}-{}",
        "a".repeat(1_000),
        ("a".repeat(999) + "b").repeat(200)
    );
    let cases = [
        // Every column check scans its line from the start.
        ("start: S\nS: \"a\" (!%same \"a\")*\n", "a".repeat(20_000)),
        // A reserved terminal looks up its match in every growing step.
        (
            "start: T | \"kw\"\n@reserved\nT: T \"x\" | \"x\"\n",
            "x".repeat(5_000),
        ),
        // A %ref compares a long captured text at every position.
        (
            "start: %def(c, A) \"-\" (%ref(c) | \"a\" | \"b\")*\nA: \"a\"*\n",
            compared,
        ),
    ];
    for (text, input) in cases {
        let grammar = Grammar::from_text(text).expect("the grammar reads");
        let error = Reference::new(&grammar)
            .expect("the grammar resolves")
            .parse_with(&input, "start", limits)
            .expect_err("too much scanning");
        assert_eq!(error.kind, ParseErrorKind::Limit(Limit::Steps), "{text}");
    }
}

#[test]
fn a_grammar_reports_at_most_a_hundred_errors() {
    let names: Vec<String> = (0..1_000).map(|i| format!("missing{i}")).collect();
    let errors = grammar_errors(&format!("start: {}\n", names.join(" ")));
    assert_eq!(errors.len(), 100);
}

#[test]
fn copying_results_counts_against_the_step_limit() {
    // Left recursion copies its growing result in every step, quadratic in the input length.
    let grammar = Grammar::from_text("start: sum\nsum: sum \"+\" \"1\" | \"1\"\n")
        .expect("the grammar reads");
    let input = format!("1{}", "+1".repeat(3_000));
    let limits = Limits {
        max_steps: 1_000_000,
        ..Limits::default()
    };
    let error = Reference::new(&grammar)
        .expect("the grammar resolves")
        .parse_with(&input, "start", limits)
        .expect_err("too much copying");
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
fn templates_that_expand_too_far_are_errors_before_they_take_the_memory() {
    let doubling = "start: t<\"b\">\nt<x>: \"a\" t<x x> | x\n".to_owned();
    // An argument made of many copies of the parameter, built only if it fits.
    let copies = format!("start: t<\"a\">\nt<p>: t<({})>\n", ["p"; 250].join(" "));
    // A valid grammar whose last template lowers a copy of a large argument per use.
    let mut uses = String::from("start: d1<\"a\">\n");
    for i in 1..11 {
        uses.push_str(&format!("d{i}<p>: d{}<(p p)>\n", i + 1));
    }
    uses.push_str("d11<p>: x<(p p)>\n");
    uses.push_str(&format!("x<p>: {}\n", ["p"; 1_000].join(" ")));
    // A parameter given arguments is an error, but each use still counts as a copy.
    let with_arguments = uses.replace(&["p"; 1_000].join(" "), &["p<\"a\">"; 200].join(" "));
    // A precedence block cannot take parameters, but each instance would lower all of it.
    let alternatives = (0..400).map(|i| format!("p<\"{i}\">")).collect::<Vec<_>>();
    let block = format!(
        "start: {}\np<x>: %precedence {{\n    sum: {} -> sum\n    atom: \"b\"\n}}\n",
        alternatives.join(" | "),
        ["\"a\""; 2_000].join(" ")
    );
    // An instance refused in the skip expression of a grammar without plain rules, so that no
    // rule stands where the refused one would have been.
    let in_skip = format!(
        "%ignore x=T<({})>\nT<p>: {}\n",
        ["\"a\""; 60].join(" "),
        ["p"; 2_000].join(" ")
    );
    for text in [doubling, copies, uses, with_arguments, block, in_skip] {
        let errors = grammar_errors(&text);
        assert!(
            errors.iter().any(|e| e.contains("too many expressions")),
            "{errors:?}"
        );
    }
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
fn more_flags_fields_capture_names_or_instances_than_allowed_are_errors() {
    let count = usize::from(u16::MAX) + 2;
    // The start rule uses the last flag declared.
    let flag_grammar = |flags: usize| {
        let mut builder = GrammarBuilder::new();
        for i in 0..flags {
            builder.flag(&format!("f{i}"));
        }
        builder.rule("start", on(&format!("f{}", flags - 1), lit("a")));
        builder
    };
    Reference::new(&flag_grammar(64).build()).expect("64 flags are allowed");
    // A flag beyond the limit is declared all the same, so its use adds no error.
    let errors = Reference::new(&flag_grammar(70).build()).expect_err("too many flags");
    assert_eq!(errors.len(), 1, "{errors:?}");
    let flags = flag_grammar(65);
    let mut fields = GrammarBuilder::new();
    let named = (0..count)
        .map(|i| field(&format!("x{i}"), lit("a")))
        .collect();
    fields.rule("start", Expr::new(ExprKind::Sequence(named)));
    let mut captures = GrammarBuilder::new();
    let defs = (0..count).map(|i| def(&format!("c{i}"), empty())).collect();
    captures.rule("start", Expr::new(ExprKind::Sequence(defs)));
    // Small instances, so that their number and not their size is too large.
    let mut instances = GrammarBuilder::new();
    let uses = (0..1_001)
        .map(|i| r_args("t", [lit(&format!("a{i}"))]))
        .collect();
    instances.rule("start", Expr::new(ExprKind::Choice(uses)));
    instances.rule("t", r("x")).params(["x"]);
    for (builder, message) in [
        (flags, "at most 64 flags"),
        (fields, "too many field names"),
        (captures, "too many capture names"),
        (instances, "too many instances"),
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
