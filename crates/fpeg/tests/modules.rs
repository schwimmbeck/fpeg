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
    // A module that defines a rule twice, which nothing resolves on its own.
    let module = "x: \"a\" y\nx: \"b\"\ny: \"c\"";
    let errors = linked("%import m (x)\nstart: x", &[("m", module)]).expect_err("x twice");
    let twice = |d: &Diagnostic| d.message.ends_with("the rule x is defined twice");
    assert!(errors.iter().any(twice), "{errors:?}");
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
    let text = |files: &ModuleFiles, name: &str| files.text(name).expect("it reads");
    assert_eq!(text(&files, "base").as_deref(), Some("start: \"a\""));
    assert_eq!(text(&files, "missing"), None);
    // A name that is a path would reach out of the folder.
    let inner = folder.join("inner");
    std::fs::create_dir_all(&inner).expect("a folder");
    assert_eq!(text(&ModuleFiles::new(&inner), "../base"), None);
    // A file that cannot be read is an error, so that the library's json does not stand in.
    std::fs::write(folder.join("json.fpeg"), [0xff, 0xfe]).expect("a file");
    std::fs::create_dir_all(folder.join("dir.fpeg")).expect("a folder");
    assert!(files.text("dir").is_err());
    let grammar = Grammar::from_text("%extends \"json\"").expect("it reads");
    let errors = grammar.link(&files).expect_err("json.fpeg is no UTF-8");
    assert!(
        errors[0]
            .message
            .starts_with("the grammar json cannot be read")
    );
    std::fs::remove_dir_all(&folder).expect("the folder goes");
}

/// How long a read of a module may take before the test takes it as waiting for a writer.
#[cfg(unix)]
const PIPE_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(unix)]
#[test]
fn a_pipe_named_like_a_module_is_an_error_at_once() {
    let folder = std::env::temp_dir().join(format!("fpeg-pipe-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("a folder");
    let made = std::process::Command::new("mkfifo")
        .arg(folder.join("base.fpeg"))
        .status()
        .expect("mkfifo runs");
    assert!(made.success());
    let files = ModuleFiles::new(&folder);
    let (sender, receiver) = std::sync::mpsc::channel();
    // Opening a pipe for reading waits for a writer, so the read runs on a thread of its own.
    std::thread::spawn(move || sender.send(files.text("base").is_err()));
    let refused = receiver
        .recv_timeout(PIPE_WAIT)
        .expect("the read waits for no writer");
    assert!(refused);
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

#[test]
fn the_problems_of_modules_stand_in_the_order_of_the_text() {
    let module = "x: \"a\"? | \"b\"";
    let text = "%import m (x)\nstart: x | y\ny: \"c\"? | \"d\"";
    let joined = linked(text, &[("m", module)]).expect("it joins");
    let lines: Vec<u32> = (fpeg::check(&joined).iter())
        .map(|d| d.span.start)
        .collect();
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[0] < lines[1], "{lines:?}");
}

/// Templates of a module that never use their parameter, each a warning to place in the module.
const MANY_PLACED: usize = 16_000;
/// How many times fewer the first measurement places.
const FEWER_PLACED: usize = 8;
/// The most that placing all the warnings may take against placing the fewer: about the ratio of
/// the counts where each module is indexed once, its square where once for each warning.
const MAX_PLACING_TIME_FACTOR: f64 = 20.0;
/// Runs of each measurement, the best of which counts.
const PLACING_RUNS: usize = 3;

/// The best time to check an extension of a module with `templates` warnings.
fn placing_seconds(templates: usize) -> f64 {
    let base: String = (0..templates)
        .map(|i| format!("t{i}<x>: \"a\"\n"))
        .collect();
    let joined = linked("%extends \"base\"\nstart: \"s\"", &[("base", &base)]).expect("it joins");
    let mut best = f64::INFINITY;
    for _ in 0..PLACING_RUNS {
        let started = std::time::Instant::now();
        assert_eq!(fpeg::check(&joined).len(), templates);
        best = best.min(started.elapsed().as_secs_f64());
    }
    best
}

#[test]
fn placing_the_problems_of_a_module_takes_time_in_proportion_to_their_number() {
    let fewer = placing_seconds(MANY_PLACED / FEWER_PLACED);
    let all = placing_seconds(MANY_PLACED);
    let factor = all / fewer;
    assert!(
        factor < MAX_PLACING_TIME_FACTOR,
        "{fewer:.3} s, then {all:.3} s"
    );
}

/// Imports of missing modules in the larger join, which a quadratic join takes seconds over.
const MANY_IMPORTS: usize = 40_000;
/// Flags of an extension and of its base in the larger join.
const MANY_FLAGS: usize = 20_000;
/// How many times fewer imports and flags the smaller join has.
const SMALLER_JOIN: usize = 8;
/// The most that the larger join may take against the smaller: about the ratio of their sizes
/// where joining takes linear time, its square where quadratic.
const MAX_JOIN_TIME_FACTOR: f64 = 20.0;
/// Runs of each join, the best of which counts, so that neither the speed of a machine nor the
/// tests beside it decide.
const JOIN_RUNS: usize = 3;

/// The best time of [`JOIN_RUNS`] joins of a grammar with `imports` imports of missing modules
/// and of an extension and its base with `flags` flags each.
fn join_seconds(imports: usize, flags: usize) -> f64 {
    let imports: String = (0..imports)
        .map(|i| format!("%import m{i} (x)\n"))
        .collect();
    let flags = |prefix: &str| -> String {
        let names: Vec<String> = (0..flags).map(|i| format!("{prefix}{i}")).collect();
        format!("%flag {}\n", names.join(" "))
    };
    let base = format!("{}start: \"a\"", flags("b"));
    let extension = format!("%extends \"base\"\n{}", flags("e"));
    let mut best = f64::INFINITY;
    for _ in 0..JOIN_RUNS {
        let started = std::time::Instant::now();
        let grammar = Grammar::from_text(&imports).expect("the grammar reads");
        assert!(grammar.link(&HashMap::new()).is_err());
        let grammar = Grammar::from_text(&extension).expect("the grammar reads");
        let joined = grammar.link(&HashMap::from([("base".to_owned(), base.clone())]));
        assert!(joined.is_ok());
        best = best.min(started.elapsed().as_secs_f64());
    }
    best
}

#[test]
fn joining_takes_linear_time_in_imports_and_flags() {
    let smaller = join_seconds(MANY_IMPORTS / SMALLER_JOIN, MANY_FLAGS / SMALLER_JOIN);
    let larger = join_seconds(MANY_IMPORTS, MANY_FLAGS);
    let factor = larger / smaller;
    assert!(
        factor < MAX_JOIN_TIME_FACTOR,
        "{smaller:.3} s, then {larger:.3} s"
    );
}

#[test]
fn common_s_escaped_string_stands_on_one_line() {
    let grammar = Grammar::from_text("%import common (ESCAPED_STRING)\nstart: ESCAPED_STRING")
        .expect("it reads");
    let parser = Parser::new(&grammar).unwrap_or_else(|e| panic!("{e:?}"));
    assert!(parser.parse("\"a\\\"b\"", "start").is_ok());
    assert!(parser.parse("\"a\nb\"", "start").is_err());
    // A backslash escapes no line break either.
    assert!(parser.parse("\"a\\\nb\"", "start").is_err());
}

#[test]
fn an_extension_joins_and_replaces_the_directives_of_its_base() {
    let base = "%flag a\n%keywords soft \"if\"\n%word [a-z]\n%tabs 4\n%recover l: \"x\"\n%recover m: ()\nstart: \"x\"";
    let text = "%extends \"base\"\n%flag a b\n%keywords soft \"if\" \"do\"\n%word [a-z0-9]\n%recover l: \"y\"";
    let joined = linked(text, &[("base", base)]).expect("it joins");
    // The extension's recovery of l takes the place of the base's, whose text lies past it.
    let own = |span: fpeg::Span| (span.end as usize) <= text.len();
    let recovers: Vec<(&str, bool)> = (joined.recovers.iter())
        .map(|r| (r.label.text.as_str(), own(r.expr.span)))
        .collect();
    assert_eq!(recovers, [("m", false), ("l", true)]);
    let flags: Vec<&str> = joined.flags.iter().map(|f| f.text.as_str()).collect();
    assert_eq!(flags, ["a", "b"]);
    let words: Vec<&str> = (joined.soft_keywords.iter())
        .map(|w| w.value.as_str())
        .collect();
    assert_eq!(words, ["if", "do"]);
    // The extension's word set takes the place of the base's, whose text lies past the grammar's.
    let word = joined.word.as_ref().expect("a word set");
    assert!((word.span.end as usize) <= text.len(), "{word:?}");
    // The extension gives no tab width, so the base's stays.
    assert_eq!(joined.tabs.map(|tabs| tabs.value), Some(4));
}
