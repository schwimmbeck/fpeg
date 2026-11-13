// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Any grammar text and any input: reading, compiling and parsing end without a panic. The text
//! up to the first NUL is the grammar, the rest the input.

#![no_main]

use fastpeg::{Grammar, Limits, Parser};
use libfuzzer_sys::fuzz_target;

/// Steps a parse may take, so that a grammar with exponential time ends quickly.
const MAX_STEPS: u64 = 1_000_000;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else {
        return;
    };
    let (grammar, input) = text.split_once('\0').unwrap_or((text, ""));
    let Ok(grammar) = Grammar::from_text(grammar) else {
        return;
    };
    let Ok(parser) = Parser::new(&grammar) else {
        return;
    };
    let limits = Limits {
        max_steps: MAX_STEPS,
        ..Limits::default()
    };
    for rule in grammar.rules.iter().filter(|r| r.params.is_empty()).take(3) {
        if let Ok(tree) = parser.parse_with(input, &rule.name.text, limits) {
            let _ = tree.to_lines(input, true);
        }
    }
});
