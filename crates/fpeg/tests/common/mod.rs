// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Helpers that several test files share.

#![allow(
    dead_code,
    reason = "each test file is a crate of its own and uses only some of these helpers"
)]

pub mod cases;
pub mod generator;

use std::path::{Path, PathBuf};

use fpeg::Grammar;

/// The folder of the grammars that come with the library, which holds their samples in
/// `samples/<name>/`.
pub fn grammars() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("grammars")
}

/// The grammar files of [`grammars`], sorted.
pub fn grammar_files() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(grammars())
        .expect("the grammars directory exists")
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| path.extension().is_some_and(|e| e == "fpeg"))
        .collect();
    paths.sort();
    paths
}

/// Steps a parse takes at least before it starts again with every rule remembered, the budget
/// of the machine for an input shorter than a thousand bytes.
pub const RESTART_MIN_STEPS: u64 = 1_000_000;

/// Levels of nesting in the pathological input, each of which doubles the work of a grammar
/// that matches a rule twice at every level.
pub const PATHOLOGICAL_LEVELS: usize = 24;

/// Brackets around brackets, [`PATHOLOGICAL_LEVELS`] deep, each pair followed by a `y`.
pub fn pathological_input() -> String {
    (0..PATHOLOGICAL_LEVELS).fold("wy".to_owned(), |inner, _| format!("({inner})y"))
}

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
