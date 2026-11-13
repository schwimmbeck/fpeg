// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The warnings of `check`: each is given where it holds and left out where it does not. The
//! pages of the codes show one grammar for each, which `tests/diagnostics.rs` holds true.

use std::path::Path;

use fpeg::{Grammar, Severity};

/// The codes `check` gives for a grammar text, in the order of their places.
fn codes(text: &str) -> Vec<&'static str> {
    let grammar = Grammar::from_text(text).unwrap_or_else(|errors| panic!("{errors:?}"));
    let found = fpeg::check(&grammar);
    found
        .iter()
        .map(|diagnostic| diagnostic.code.code())
        .collect()
}

#[test]
fn the_acceptance_grammars_give_no_warning() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("grammars");
    let mut checked = 0;
    for entry in std::fs::read_dir(&root).expect("the grammars directory exists") {
        let path = entry.expect("a directory entry").path();
        if path.extension().is_none_or(|extension| extension != "fpeg") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("the grammar is readable");
        let grammar = Grammar::from_text(&text).expect("the grammar reads");
        // What the parser does not run yet is an error, and no reason to keep quiet about more.
        let warnings: Vec<_> = (fpeg::check(&grammar).into_iter())
            .filter(|diagnostic| diagnostic.severity() == Severity::Warning)
            .collect();
        assert!(warnings.is_empty(), "{}: {warnings:?}", path.display());
        checked += 1;
    }
    assert!(checked >= 10, "{checked} grammars");
}

#[test]
fn a_grammar_that_does_not_resolve_gives_its_errors_alone() {
    assert_eq!(codes("start: missing | \"a\"? | \"b\"\n"), ["E201"]);
}

#[test]
fn an_alternative_after_one_that_cannot_fail_is_never_tried() {
    for (text, expected) in [
        ("start: \"a\"? | \"b\"\n", vec!["W101"]),
        ("start: \"a\"* | \"b\" | \"c\"\n", vec!["W101"]),
        ("start: () | \"b\"\n", vec!["W101"]),
        // A labeled expression matches or ends the parse.
        ("start: \"a\" ^ \"an a\" | \"b\"\n", vec!["W101"]),
        ("start: x=(\"a\" ** \",\") | \"b\"\n", vec!["W101"]),
        // Inside a group, and in the ignored text.
        ("start: (\"a\"? | \"b\") \"c\"\n", vec!["W101"]),
        (
            "start: \"a\"\n%ignore \" \"*\n%ignore \"#\"\n",
            vec!["W101"],
        ),
        // These can fail.
        ("start: \"a\"+ | \"b\"\n", vec![]),
        ("start: \"a\" ++ \",\" | \"b\"\n", vec![]),
        ("start: !\"a\" | \"b\"\n", vec![]),
        ("start: \"b\" | \"a\"?\n", vec![]),
    ] {
        assert_eq!(codes(text), expected, "{text}");
    }
}

#[test]
fn a_level_whose_last_alternative_cannot_fail_cuts_off_the_levels_after_it() {
    let block = |sign: &str| {
        format!("start: e\ne: %precedence {{\n    signed: {sign} -> signed\n    atom: \"1\"\n}}\n")
    };
    assert_eq!(codes(&block("\"-\"? atom")), Vec::<&str>::new());
    assert_eq!(codes(&block("\"-\"?")), ["W101"]);
}
