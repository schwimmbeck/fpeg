// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Generated grammars and inputs on the reference interpreter. Resolving and parsing never panic
//! and always end, every grammar prints to text that reads back to the same grammar, and
//! memoization never changes a result. `common/generator.rs` makes the grammars.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::check_printing;
use common::generator::{check_seeds, generate, reference_limits as limits};
use fastpeg::{ParseError, ParseErrorKind, Reference, Tree};

/// Resolving and parsing do not panic, and the grammar prints to text that reads back the same.
fn check_robust(seed: u64) -> Option<String> {
    let (grammar, inputs) = generate(seed);
    // The text front end runs on the reference interpreter too.
    match catch_unwind(|| check_printing(&grammar)) {
        Ok(None) => {}
        Ok(Some(problem)) => return Some(problem),
        Err(_) => return Some(format!("panicked reading back:\n{grammar}")),
    }
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        if let Ok(reference) = Reference::new(&grammar) {
            for input in &inputs {
                let _ = reference.parse_with(input, "start", limits());
            }
        }
    }));
    outcome
        .err()
        .map(|_| format!("panicked on one of {inputs:?}:\n{grammar}"))
}

/// Parsing with and without the memo gives the same tree or the same error.
fn check_memo(seed: u64) -> Option<String> {
    let (grammar, inputs) = generate(seed);
    let reference = Reference::new(&grammar).ok()?;
    for input in &inputs {
        let shown = |result: Result<Tree, ParseError>| match result {
            Ok(tree) => Some(tree.to_text(input, true)),
            Err(error) if matches!(error.kind, ParseErrorKind::Limit(_)) => None,
            Err(error) => Some(error.to_string()),
        };
        let memoized = shown(reference.parse_with(input, "start", limits()));
        let plain = shown(reference.parse_unmemoized(input, "start", limits()));
        if let (Some(memoized), Some(plain)) = (memoized, plain)
            && memoized != plain
        {
            return Some(format!(
                "on {input:?} the memo gives\n{memoized}\nand no memo gives\n{plain}\n{grammar}"
            ));
        }
    }
    None
}

#[test]
fn generated_grammars_resolve_parse_and_print_without_failing() {
    check_seeds(check_robust);
}

#[test]
fn memoization_never_changes_a_result() {
    check_seeds(check_memo);
}
