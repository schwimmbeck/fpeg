// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Any grammar the parsing machine runs gives, on any input, the tree or the error the reference
//! interpreter gives, and a grammar that one engine rejects the other rejects alike. The text up
//! to the first NUL is the grammar, the rest the input. Parses where either engine hits a limit
//! are skipped, since the two count steps differently.

#![no_main]

use fpeg::{DiagnosticCode, Grammar, Limits, ParseError, ParseErrorKind, Parser, Reference, Tree};
use libfuzzer_sys::fuzz_target;

/// Steps a parse may take on the reference interpreter, so that slow grammars end quickly.
const REFERENCE_MAX_STEPS: u64 = 100_000;

/// Steps a parse may take on the machine, whose steps do less than the interpreter's.
const MACHINE_MAX_STEPS: u64 = 1_000_000;

/// A parse as text: the tree with spans, or the error with its expected set. None at a limit.
fn shown(result: Result<Tree, ParseError>, input: &str) -> Option<String> {
    match result {
        Ok(tree) => Some(tree.to_text(input, true)),
        Err(error) if matches!(error.kind, ParseErrorKind::Limit(_)) => None,
        Err(error) => Some(format!("{error:?}")),
    }
}

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let (grammar, input) = text.split_once('\0').unwrap_or((text, ""));
    let Ok(grammar) = Grammar::from_text(grammar) else {
        return;
    };
    let (reference, parser) = match (Reference::new(&grammar), Parser::new(&grammar)) {
        (Ok(reference), Ok(parser)) => (reference, parser),
        // Only the machine bounds the size of what it compiles.
        (Ok(_), Err(errors)) if errors.iter().all(|e| e.code == DiagnosticCode::TooLarge) => {
            return;
        }
        (Err(expected), Err(errors)) => {
            assert_eq!(expected, errors, "the engines reject it differently");
            return;
        }
        (expected, errors) => panic!(
            "one engine rejects it: {:?} {:?}",
            expected.err(),
            errors.err()
        ),
    };
    for rule in grammar.rules.iter().filter(|r| r.params.is_empty()).take(3) {
        let start = &rule.name.text;
        let limits = |max_steps| Limits {
            max_steps,
            ..Limits::default()
        };
        let expected = shown(
            reference.parse_with(input, start, limits(REFERENCE_MAX_STEPS)),
            input,
        );
        let actual = shown(
            parser.parse_with(input, start, limits(MACHINE_MAX_STEPS)),
            input,
        );
        if let (Some(expected), Some(actual)) = (expected, actual) {
            assert_eq!(expected, actual, "from {start} on {input:?}");
        }
    }
});
