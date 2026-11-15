// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Grammars that extend and import others: the rules they replace for every reference, where
//! `...` puts the inherited alternatives, the directives they add, the rules an import brings in
//! and under which names, and the problems of modules with their codes.

use fpeg::{Diagnostic, DiagnosticCode, Grammar, Parser};

/// The tree of an input from `start`, as the tests print trees.
fn tree(text: &str, input: &str) -> String {
    let grammar = Grammar::from_text(text).unwrap_or_else(|errors| panic!("{errors:?}"));
    let parser = Parser::new(&grammar).unwrap_or_else(|errors| panic!("{errors:?}"));
    let tree = parser
        .parse(input, "start")
        .unwrap_or_else(|e| panic!("{e:?}"));
    tree.to_text(input, false)
}

/// What `check` finds in a grammar text.
fn found(text: &str) -> Vec<Diagnostic> {
    let grammar = Grammar::from_text(text).unwrap_or_else(|errors| panic!("{errors:?}"));
    fpeg::check(&grammar)
}

#[test]
fn a_rule_defined_again_replaces_the_inherited_one_for_every_reference() {
    // The base's array and member refer to ?value, which the extension replaces.
    let text = "%extends \"json\"\n?value: ... | NAME\nNAME: [a-z]+";
    assert_eq!(
        tree(text, "[1, abc]"),
        "(array ~\"[\" INT\"1\" ~\",\" NAME\"abc\" ~\"]\")"
    );
}

#[test]
fn the_inherited_alternatives_stand_where_the_extension_writes_them() {
    // Ordered choice tells which alternative comes first: a name that is also a literal of the
    // base's matches as whichever stands first.
    let first = "%extends \"json\"\n?value: WORD | ...\nWORD: [a-z]+";
    let last = "%extends \"json\"\n?value: ... | WORD\nWORD: [a-z]+";
    assert_eq!(tree(first, "true"), "WORD\"true\"");
    assert_eq!(tree(last, "true"), "(value ~\"true\")");
}

#[test]
fn the_ignored_text_of_an_extension_comes_after_the_base_s() {
    let text = "%extends \"json\"\n%ignore \"//\" [^\\n]*";
    assert_eq!(
        tree(text, "[1, // one\n 2]"),
        "(array ~\"[\" INT\"1\" ~\",\" INT\"2\" ~\"]\")"
    );
}

#[test]
fn a_misplaced_inherited_body_is_an_error() {
    use DiagnosticCode::{Inherited, NotYet};
    for (text, code, message) in [
        (
            "%extends \"json\"\n?value: \"nan\" ...",
            Inherited,
            "whole alternative",
        ),
        (
            "%extends \"json\"\n?value: ... | ...",
            Inherited,
            "at most once",
        ),
        (
            "%extends \"json\"\n?value: ... -> other",
            Inherited,
            "takes none",
        ),
        (
            "%extends \"json\"\n?value<x>: ... | x",
            Inherited,
            "other parameters",
        ),
        (
            "%extends \"json\"\nother: ...\nstart: other",
            Inherited,
            "this rule replaces",
        ),
        // Python's expression is a precedence block.
        (
            "%extends \"python\"\nexpression: ... | \"x\"",
            NotYet,
            "for a precedence block",
        ),
        (
            "%extends \"json\"\nvalue: %precedence {\n    sum: ...\n}",
            NotYet,
            "in a precedence",
        ),
    ] {
        let found = found(text);
        let error = found.iter().find(|d| d.code == code);
        assert!(
            error.is_some_and(|d| d.message.contains(message)),
            "{text}: {found:?}"
        );
    }
}

#[test]
fn a_grammar_that_no_module_provides_is_an_error() {
    let found = found("%extends \"jsn\"\nstart: \"a\"");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code, DiagnosticCode::UnknownModule);
}

#[test]
fn a_grammar_extends_at_most_one_other() {
    let errors = Grammar::from_text("%extends \"json\"\n%extends \"python\"").unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, DiagnosticCode::Duplicate);
}

#[test]
fn a_problem_in_a_module_s_rules_stands_at_the_directive_that_takes_the_module_in() {
    // A CHAR that may match nothing makes the base's STRING repeat it, a warning of that rule.
    let text = "%extends \"json\"\nCHAR: [a-z]?";
    let found = found(text);
    let in_json: Vec<&Diagnostic> = (found.iter())
        .filter(|d| d.message.starts_with("in the grammar json, line "))
        .collect();
    assert!(!in_json.is_empty(), "{found:?}");
    for diagnostic in in_json {
        assert_eq!(
            &text[diagnostic.span.start as usize..diagnostic.span.end as usize],
            "\"json\""
        );
    }
}

#[test]
fn an_import_takes_the_rules_it_names_and_what_they_need_under_private_names() {
    let text = "%import common (NUMBER)\nstart: NUMBER+\n%ignore \" \"+";
    assert_eq!(tree(text, "1 2.5"), "(start NUMBER\"1\" NUMBER\"2.5\")");
    // json's ?value needs the array and the number, which carry the name of their grammar.
    let text = "%import json (value)\nstart: value";
    assert_eq!(
        tree(text, "[1]"),
        "(start (json.array ~\"[\" json.INT\"1\" ~\"]\"))"
    );
}

#[test]
fn imports_of_one_grammar_share_what_they_need() {
    // INT and NUMBER both need DIGIT, which comes once.
    let text = "%import common (INT)\n%import common (NUMBER)\nstart: INT \"-\" NUMBER";
    assert_eq!(tree(text, "1-2"), "(start INT\"1\" ~\"-\" NUMBER\"2\")");
}

#[test]
fn the_levels_of_an_imported_precedence_block_take_a_private_name() {
    // Python's expression is a block with a level sum, which the grammar's own sum does not meet.
    let text = "%import python (expression)\nstart: expression | sum\nsum: \"x\"\n%ignore \" \"+";
    let found = found(text);
    assert!(
        found.iter().all(|d| d.code != DiagnosticCode::Duplicate),
        "{found:?}"
    );
    assert!(tree(text, "1 + 2").contains("python.NUMBER"));
}

#[test]
fn an_import_of_a_name_defined_twice_or_not_at_all_is_an_error() {
    for (text, code) in [
        (
            "%import common (NUMBER)\nstart: NUMBER\nNUMBER: [0-9]+",
            DiagnosticCode::ImportClash,
        ),
        (
            "%import common (NUMBR)\nstart: \"a\"",
            DiagnosticCode::UnknownImport,
        ),
    ] {
        let found = found(text);
        assert!(found.iter().any(|d| d.code == code), "{text}: {found:?}");
    }
}
