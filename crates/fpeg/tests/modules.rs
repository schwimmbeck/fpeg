// SPDX-FileCopyrightText: 2026 Dominik Schwimmbeck
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Grammars that extend and import others: the rules they replace for every reference, where
//! `...` puts the inherited alternatives, the directives they add, the rules an import brings in
//! and under which names, and the problems of modules with their codes.

use std::collections::HashMap;

use fpeg::{Diagnostic, DiagnosticCode, Grammar, ModuleFiles, Modules, Parser};

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
            "aliases of the inherited alternatives and takes none",
        ),
        (
            "%extends \"json\"\n?value: ... => list",
            Inherited,
            "builders of the inherited alternatives and takes none",
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
    // A name that is not plain names no module, even one given by that name.
    for (text, modules) in [
        ("%extends \"jsn\"\nstart: \"a\"", &[][..]),
        ("%import \"2d\" (x)\nstart: x", &[("2d", "x: \"x\"")][..]),
        ("%import \"a.b\" (x)\nstart: x", &[("a.b", "x: \"x\"")][..]),
    ] {
        let errors = linked(text, modules).unwrap_err();
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].code, DiagnosticCode::UnknownModule);
    }
}

#[test]
fn imports_of_one_module_through_several_paths_share_its_private_rules() {
    let calc = "%import common (NUMBER, WS)\nstart: NUMBER+\n%ignore WS";
    let text =
        "%extends \"calc\"\n%import common (SIGNED_NUMBER)\nstart: (NUMBER | SIGNED_NUMBER)+";
    let joined = linked(text, &[("calc", calc)]).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(Parser::new(&joined).is_ok());
    // A whole module takes its own rules by name, and those private to it under its name.
    let bits = "%import common (INT)\nbit: INT";
    let text = "%import bits\n%import common (NUMBER)\nstart: bit NUMBER";
    let joined = linked(text, &[("bits", bits)]).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(Parser::new(&joined).is_ok());
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

/// A grammar text joined with modules given by name, or what the joining finds.
fn linked(text: &str, modules: &[(&str, &str)]) -> Result<Grammar, Vec<Diagnostic>> {
    let modules: HashMap<String, String> = (modules.iter())
        .map(|(name, text)| ((*name).to_owned(), (*text).to_owned()))
        .collect();
    let grammar = Grammar::from_text(text).unwrap_or_else(|errors| panic!("{errors:?}"));
    grammar.link(&modules)
}

#[test]
fn a_grammar_extends_and_imports_the_modules_it_is_given_before_the_library() {
    let base = "start: item+\nitem: WORD\nWORD: [a-z]+\n%ignore \" \"+";
    // A module named like one of the library's takes its place.
    let common = "NUMBER: [0-9]+ \"!\"";
    let text = "%extends \"base\"\n%import common (NUMBER)\nitem: ... | NUMBER";
    let joined = linked(text, &[("base", base), ("common", common)]).expect("it joins");
    let parser = Parser::new(&joined).unwrap_or_else(|errors| panic!("{errors:?}"));
    let tree = parser
        .parse("ab 12!", "start")
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        tree.to_text("ab 12!", false),
        "(start (item WORD\"ab\") (item NUMBER\"12!\"))"
    );
}

#[test]
fn a_grammar_that_takes_itself_in_is_an_error() {
    let modules = [
        ("a", "%extends \"b\"\nx: \"x\""),
        ("b", "%import a (x)\ny: x"),
    ];
    let errors = linked("%extends \"a\"\nstart: \"s\"", &modules).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, DiagnosticCode::ModuleCycle);
}

#[test]
fn a_problem_in_a_module_given_stands_at_the_directive_with_its_place_in_the_module() {
    let text = "%extends \"base\"\nstart: item";
    let joined = linked(text, &[("base", "item: \"a\"\nother: missing")]).expect("it joins");
    let Err(errors) = Parser::new(&joined) else {
        panic!("an unknown rule");
    };
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(
        errors[0].message,
        "in the grammar base, line 2, column 8: unknown rule missing"
    );
    assert_eq!(
        &text[errors[0].span.start as usize..errors[0].span.end as usize],
        "\"base\""
    );
}

#[test]
fn module_files_are_the_grammar_files_of_a_folder_by_plain_names() {
    let folder = std::env::temp_dir().join(format!("fpeg-modules-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("a folder");
    std::fs::write(folder.join("base.fpeg"), "\u{feff}start: \"a\"").expect("a file");
    let files = ModuleFiles::new(&folder);
    assert_eq!(files.text("base").as_deref(), Some("start: \"a\""));
    assert_eq!(files.text("missing"), None);
    // A name that is a path would reach out of the folder.
    let inner = folder.join("inner");
    std::fs::create_dir_all(&inner).expect("a folder");
    assert_eq!(ModuleFiles::new(&inner).text("../base"), None);
    std::fs::remove_dir_all(&folder).expect("the folder goes");
}

#[test]
fn an_imported_rule_keeps_to_its_own_module() {
    // A name the module lacks is its error, and takes no rule of the grammar that imports it.
    let errors = linked("%import m (x)\nstart: x\ny: \"q\"", &[("m", "x: y")]).unwrap_err();
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].code, DiagnosticCode::UnknownRule);
    // A private rule's alias names a private kind, which the grammar's own alias does not meet.
    let module = "pub: item\nitem: \"x\" -> leaf";
    let joined = linked(
        "%import m (pub)\nstart: pub leaf\nleaf: \"y\"",
        &[("m", module)],
    );
    let parser = Parser::new(&joined.expect("it joins")).unwrap_or_else(|e| panic!("{e:?}"));
    let tree = parser
        .parse("xy", "start")
        .unwrap_or_else(|e| panic!("{e:?}"));
    assert_eq!(
        tree.to_text("xy", false),
        "(start (pub (m.leaf ~\"x\")) (leaf ~\"y\"))"
    );
    // Only the flags of the rules taken join, so no other flag of the module is left unused.
    let module = "%flag a b\nx: %on(a, z)\nz: %if(a) \"x\"\ny: %if(b) \"y\"";
    let joined = linked("%import m (x)\nstart: x", &[("m", module)]).expect("it joins");
    assert_eq!(fpeg::check(&joined), []);
}
