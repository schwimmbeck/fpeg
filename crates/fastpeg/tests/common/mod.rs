// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Helpers that several test files share.

// Each test file is its own crate and uses a different part of these helpers.
#![allow(
    dead_code,
    reason = "every test file uses only some of the shared helpers"
)]

pub mod cases;
pub mod generator;

use fastpeg::Grammar;

/// The grammar's debug form without source spans, so that two readings of different texts
/// compare equal when they mean the same grammar.
fn shape(grammar: &Grammar) -> String {
    let debug = format!("{grammar:?}");
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug.as_str();
    while let Some(at) = rest.find("Span { start: ") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let end = after.find(" }").map_or(after.len(), |e| e + 2);
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// Printing must give text that reads back to the same grammar.
pub fn check_printing(grammar: &Grammar) -> Option<String> {
    let printed = grammar.to_string();
    match Grammar::from_text(&printed) {
        Ok(again) if shape(&again) == shape(grammar) => None,
        Ok(_) => Some(format!(
            "the printed grammar reads back differently:\n{printed}"
        )),
        Err(errors) => Some(format!(
            "the printed grammar does not read back: {errors:?}\n{printed}"
        )),
    }
}
