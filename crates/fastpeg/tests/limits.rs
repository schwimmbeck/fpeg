// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT

//! Limits stop a parse with an error value, never with a crash.

use fastpeg::grammar::Grammar;
use fastpeg::reference::{Limit, Limits, ParseErrorKind, Reference};

fn nesting() -> Reference {
    let grammar = Grammar::from_text("start: nest\nnest: \"(\" nest \")\" | \"x\"\n")
        .expect("the grammar reads");
    Reference::new(&grammar).expect("the grammar resolves")
}

fn nested(depth: usize) -> String {
    format!("{}x{}", "(".repeat(depth), ")".repeat(depth))
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
